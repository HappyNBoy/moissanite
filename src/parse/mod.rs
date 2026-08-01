mod vars;

use anyhow::{anyhow, bail, Context as _, Result};
use crate::Context;
use crate::output::*;
use crate::parse::vars::{FnState, StackEntry, TrueVariable, Variable};
use crate::reader::{BinaryReader};
use crate::types::{InstructionKind, ModuleMagic, Section, TableType, Value, ValueType, WasmVersion};

pub fn parse_module(reader: &mut BinaryReader, ctx: &mut Context) -> Result<()> {
    reader.read::<ModuleMagic>()?;
    reader.read::<WasmVersion>()?;
    let mut last_section = Section::Custom;
    while !reader.is_empty() {
        let section = reader.read::<u32>()?.try_into()?;

        if section == Section::Data {
            return Ok(());
        }

        if section == Section::Custom {
            println!("Ignoring custom section");
            reader.skip_scope()?;
            continue;
        } else if section <= last_section {
            bail!(
                "Invalid section order; section {section:?} appears after section {last_section:?}"
            );
        }
        last_section = section;
        if section == Section::Export || section == Section::Start {
            println!("Ignoring section {section:?}");
            reader.skip_scope()?;
        } else {
            reader.scope_in(|reader| {
                parse_section(section, reader, ctx)
            }).with_context(|| format!("Failed to parse section {section:?}"))?;
        }
    }
    Ok(())
}

pub fn parse_section(section: Section, reader: &mut BinaryReader, ctx: &mut Context) -> Result<()> {
    match section {
        Section::Custom => unreachable!(),
        Section::Type => {
            ctx.function_types = reader.read()?;
        },
        Section::Import => {
            if reader.read::<u32>()? != 0 {
                bail!("Module must be standalone with no imports");
            }
        },
        Section::Function => {
            ctx.functions = reader.read()?;
            for func in &ctx.functions {
                if *func as usize >= ctx.function_types.len() {
                    bail!(
                        "Invalid function type index {func} out of {} function types",
                        ctx.function_types.len()
                    );
                }
            }
            ctx.out = Output::init(&ctx.functions, &ctx.function_types);
        },
        Section::Table => {
            ctx.tables = reader.read::<Box<[TableType]>>()?
                .into_iter()
                .map(|t| t.limit)
                .collect();
            for (i, l) in ctx.tables.iter().enumerate() {
                ctx.out.init.push(list_with_len(CName::Table(i as u32).into(), VarScope::Global, l.min));
            }
        },
        Section::Memory => {
            ctx.memories = reader.read()?;
            for (i, l) in ctx.memories.iter().enumerate() {
                ctx.out.init.push(CodeBlock::Block(CodeBlockInner::Repeat {
                    action: RepeatAction::Multiple,
                    args: chest_args(vec![
                        var_item(0, CName::ConstI.into(), VarScope::Line),
                        num_item(1, CName::CountValue(l.min).into()),
                    ]),
                }));
                ctx.out.init.push(CodeBlock::Bracket {
                    repeat: true,
                    close: false,
                });
                ctx.out.init.push(list_with_len(Name::MemoryI(i as u32), VarScope::Global, PAGE_SIZE_LEN));
                ctx.out.init.push(CodeBlock::Bracket {
                    repeat: true,
                    close: true,
                });
            }
        },
        Section::Global => {
            ctx.globals = reader.read()?;
            ctx.globals.iter().enumerate()
                .for_each(|(i, g)| {
                    ctx.out.init.push(var_block(VarAction::Set, chest_args(vec![
                        var_item(0, CName::Global(i as u32).into(), VarScope::Global),
                        num_item(1, CName::Value(g.value).into()),
                    ])));
                });
        },
        Section::Export | Section::Start => {
            panic!("Attempt to parse a section that should be skipped");
        },
        Section::Element => {
            let elem_count: u32 = reader.read()?;
            for _ in 0..elem_count {
                let table: u32 = reader.read()?;
                let offset: u32 = match reader.read_const_expr()? {
                    Value::I32(x) => x as u32,
                    v => bail!("The offset expr in an element must return I32, instead returns {:?}", ValueType::from(v))
                };
                let values = reader.read_vec::<u32>()?;

                // create a temporary elem list
                let count = values.len();
                ctx.out.init.create_list(CName::ConstTmp.into(), VarScope::Line,
                                      values.map(|x| x.map(|x|
                                          ItemData::Number { value: CName::Value(Value::I32(x as i32)).into() })
                                      )
                )?;
                // copy over the numbers one by one
                ctx.out.init.push(CodeBlock::Block(CodeBlockInner::Repeat {
                    action: RepeatAction::Multiple,
                    args: chest_args(vec![
                        var_item(0, CName::ConstI.into(), VarScope::Line),
                        num_item(1, CName::CountValue(count as u32).into()),
                    ]),
                }));
                ctx.out.init.push(CodeBlock::Bracket {
                    repeat: true,
                    close: false,
                });
                // copy the value into the table by the given offset
                ctx.out.init.push(var_block(VarAction::SetListValue, chest_args(vec![
                    var_item(0, CName::Table(table).into(), VarScope::Global),
                    num_item(1, Name::AddIOffset(offset)),
                    num_item(2, Name::IndexI(CName::ConstTmp)),
                ])));
                ctx.out.init.push(CodeBlock::Bracket {
                    repeat: true,
                    close: true,
                });
            }
        },
        Section::Code => {
            let func_count = reader.read::<u32>()? as usize;
            if func_count != ctx.functions.len() {
                bail!("Function count between section does not match; Expected {}, found {func_count}", ctx.functions.len())
            }
            for func_id in 0..func_count {
                reader.scope_in(|reader| {
                    let type_id = ctx.functions[func_id] as usize;
                    let mut locals = ctx.function_types[type_id].parameters.clone().to_vec();
                    let local_count = reader.read::<u32>()?;
                    for _ in 0..local_count {
                        let count: u32 = reader.read()?;
                        let kind: ValueType = reader.read()?;
                        for _ in 0..count {
                            locals.push(kind);
                        }
                    }

                    let mut state = FnState::new(std::mem::take(&mut ctx.out.functions[func_id]), locals);
                    let result = ctx.function_types[type_id].results
                        .iter().copied().enumerate()
                        .map(|(i, x)| (i as u32, x)).collect::<Vec<_>>();
                    parse_function(reader, &mut state, &result)?;
                    ctx.out.functions[func_id] = state.consume();

                    Ok(())
                }).with_context(|| format!("Failed to parse function with id {func_id}"))?;
            }
        }
        _ => todo!("finish all sections")
    }
    Ok(())
}

pub fn parse_function(reader: &mut BinaryReader, state: &mut FnState, result: &[(u32, ValueType)]) -> Result<()> {
    macro_rules! pop_push {
        ($val:ident, $var:ident) => {{
            let v = Box::new(pop_stack(ValueType::$val, state)?);
            state.stack.push(StackEntry {
                value: Variable::$var(v),
                kind: ValueType::$val,
            });
        }};
    }
    loop {
        let kind = reader.read::<InstructionKind>()?;
        match kind {
            InstructionKind::LocalGet => {
                let local: u32 = reader.read()?;
                let kind = *state.locals.get(local as usize).ok_or_else(
                    || anyhow!("Local index out of bounds; Requested local {local}, only {} exist", state.locals.len())
                )?;
                state.stack.push(StackEntry { value: Variable::TrueVariable(TrueVariable::Local(local)), kind });
            },
            InstructionKind::I32Add => pop_push!(I32, Addition),
            // hardcoded atm
            InstructionKind::EndInstructions => {
                if state.stack.len() != result.len() {
                    bail!("Stack length mismatched on scope exit");
                }
                let stack = std::mem::take(&mut state.stack);
                for (entry, (reg, kind)) in stack.into_iter().zip(result) {
                    if entry.kind != *kind {
                        bail!("Stack kinds mismatched");
                    }
                    state.assign(TrueVariable::Register(*reg), &entry.value);
                }
                break;
            },
            _ => panic!("instruction not implemented")
        }
    }
    Ok(())
}

fn pop_stack<const N: usize>(kind: ValueType, state: &mut FnState) -> Result<[Variable; N]> {
    if state.stack.len() >= N {
        if let Some(vec) = (0..N).into_iter().map(|_| {
            let entry = state.stack.pop().unwrap();
            if entry.kind == kind {
                Some(entry.value)
            } else {
                None
            }
        }).collect::<Option<Vec<_>>>() {
            Ok(vec.try_into().unwrap())
        } else {
            Err(anyhow!("Stack kinds mismatched"))
        }
    } else {
        Err(anyhow!("Stack items missing"))
    }
}

fn pop_stack_dyn<const N: usize>(kinds: &[ValueType; N], state: &mut FnState) -> Result<[Variable; N]> {
    if state.stack.len() >= N {
        if let Some(vec) = kinds.into_iter().map(|kind| {
            let entry = state.stack.pop().unwrap();
            if entry.kind == *kind {
                Some(entry.value)
            } else {
                None
            }
        }).collect::<Option<Vec<_>>>() {
            Ok(vec.try_into().unwrap())
        } else {
            Err(anyhow!("Stack kinds mismatched"))
        }
    } else {
        Err(anyhow!("Stack items missing"))
    }
}