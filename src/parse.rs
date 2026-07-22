use anyhow::{bail, Context as _, Result};
use crate::Context;
use crate::output::*;
use crate::reader::{BinaryReader};
use crate::types::{ModuleMagic, Section, TableType, Value, ValueType, WasmVersion};

pub fn parse_module(reader: &mut BinaryReader, ctx: &mut Context) -> Result<()> {
    reader.read::<ModuleMagic>()?;
    reader.read::<WasmVersion>()?;
    let mut last_section = Section::Custom;
    while !reader.is_empty() {
        let section = reader.read::<u32>()?.try_into()?;

        if section == Section::Code {
            return Ok(());
        }

        if section == Section::Custom {
            println!("Ignoring custom section");
            reader.skip_scope()?;
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
                ctx.out.init_list(CName::Table(i as u32).into(), VarScope::Global, l.min);
            }
        },
        Section::Memory => {
            ctx.memories = reader.read()?;
            for (i, l) in ctx.memories.iter().enumerate() {
                for j in 0..l.min {
                    ctx.out.init_list(CName::Memory(i as u32, j).into(), VarScope::Global, PAGE_SIZE);
                }
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
        }
        _ => todo!("finish all sections")
    }
    Ok(())
}