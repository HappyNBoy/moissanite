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
            ctx.tables.iter().enumerate().for_each(|(i, _)| {
                ctx.out.init_global_list(names::table(i));
            });
        },
        Section::Memory => {
            ctx.memories = reader.read()?;
            ctx.memories.iter().enumerate().for_each(|(i, _)| {
                ctx.out.init_global_list(names::memory(i));
            });
        },
        Section::Global => {
            ctx.globals = reader.read()?;
            ctx.globals.iter().enumerate()
                .for_each(|(i, g)| {
                    ctx.out.init.push(var_block(VarAction::Set, chest_args(vec![
                        var_item(0, names::global(i), VarScope::Global),
                        num_item(1, Name::from(g.value.to_string())),
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
                ctx.out.init.create_list(names::TMP.with(Name::clone), VarScope::Line,
                                      values.map(|x| x.map(|x|
                                          ItemData::Number { value: Name::from(Value::I32(x as i32).to_string()) })
                                      )
                )?;
                // copy over the numbers one by one
                ctx.out.init.push(CodeBlock::Block(CodeBlockInner::Repeat {
                    action: RepeatAction::Multiple,
                    args: chest_args(vec![
                        var_item(0, names::I.with(Name::clone), VarScope::Line),
                        num_item(1, Name::from(count.to_string())),
                    ]),
                }));
                ctx.out.init.push(CodeBlock::Bracket {
                    repeat: true,
                    close: false,
                });
                // copy the value into the table by the given offset
                ctx.out.init.push(var_block(VarAction::SetListValue, chest_args(vec![
                    var_item(0, names::table(table as usize), VarScope::Global),
                    num_item(1, Name::from(format!("%math(%var(i)+{offset})"))),
                    num_item(2, Name::from(names::TMP.with(|tmp| format!("%index({tmp},%var(i))")))),
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