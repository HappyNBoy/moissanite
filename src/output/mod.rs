pub mod structs;
pub mod names;

pub use structs::*;
use anyhow::Result;
use crate::types::FunctionType;

pub const MAX_SIZE: u32 = 10000;
pub const PAGE_SIZE_BYTES: u32 = 65536;
pub const PAGE_SIZE_LEN: u32 = PAGE_SIZE_BYTES / 8;

fn create_line(name: String, args: ChestArgs) -> OutputLine {
    OutputLine::from(vec![CodeBlock::Block(
        CodeBlockInner::Function {
            data: name,
            args,
        }
    )])
}

pub fn var_block(action: VarAction, args: ChestArgs) -> CodeBlock {
    CodeBlock::Block(
        CodeBlockInner::SetVariable {
            action,
            args,
        }
    )
}

pub fn chest_args(items: Vec<ChestSlot>) -> ChestArgs {
    ChestArgs { items }
}

pub fn chest_item(slot: u32, item: ItemData) -> ChestSlot {
    ChestSlot { slot, item }
}

pub fn function_result(i: u32) -> ChestSlot {
    chest_item(i, ItemData::Parameter {
        name: names::register(i), // the result variables are automatically the bottom of the stack
        optional: false,
        plural: false,
        param_type: ParameterType::Variable,
    })
}

pub fn function_parameter(i: u32, results: u32) -> ChestSlot {
    chest_item(results + i, ItemData::Parameter {
        name: names::local(i),
        optional: false,
        plural: false,
        param_type: ParameterType::Number,
    })
}

pub fn var_item(i: u32, name: String, scope: VarScope) -> ChestSlot {
    chest_item(i, ItemData::Variable { name, scope })
}

pub fn num_item(i: u32, value: String) -> ChestSlot {
    chest_item(i, ItemData::Number { value })
}

pub fn list_with_len(name: String, scope: VarScope, len: u32) -> CodeBlock {
    var_block(VarAction::TrimList, chest_args(vec![
        var_item(0, name, scope),
        var_item(1, names::BLANK.into(), VarScope::Global),
        num_item(2, names::integer(len)),
    ]))
}

impl OutputLine {
    pub const fn new() -> OutputLine {
        OutputLine { blocks: Vec::new() }
    }

    pub fn push(&mut self, block: CodeBlock) {
        self.blocks.push(block);
    }

    pub fn create_list(&mut self, name: String, scope: VarScope, values: impl Iterator<Item = Result<ItemData>>) -> Result<()> {
        let mut values = values.peekable();
        let var = var_item(0, name, scope);
        let mut action = VarAction::CreateList;
        while action == VarAction::CreateList || values.peek().is_some() {
            let mut args = ChestArgs { items: Vec::with_capacity(27) };
            args.items.push(var.clone());
            for i in 1..27 {
                let Some(item) = values.next() else { break };
                args.items.push(chest_item(i, item?));
            }
            self.push(var_block(action, args));
            action = VarAction::AppendValue;
        }
        Ok(())
    }
}

impl Output {
    pub fn new() -> Self {
        Output {
            functions: Box::new([]),
            init: OutputLine::new(),
        }
    }

    pub fn init(functions: &[u32], function_types: &[FunctionType]) -> Self {
        let mut init = create_line(names::INIT_FN.into(), ChestArgs::empty());
        // fill a blank page for the easier creation of new pages
        let blank_item = var_item(0, names::BLANK.into(), VarScope::Global);
        init.push(var_block(VarAction::CreateList, chest_args(vec![
            blank_item.clone()
        ])));
        init.push(CodeBlock::Block(CodeBlockInner::Repeat {
            action: RepeatAction::Multiple,
            args: chest_args(vec![
                num_item(0, names::integer(MAX_SIZE))
            ]),
        }));
        init.push(CodeBlock::Bracket {
            repeat: true,
            close: false,
        });
        init.push(var_block(VarAction::AppendValue, chest_args(vec![
            blank_item,
            num_item(1, names::integer(0)),
        ])));
        init.push(CodeBlock::Bracket {
            repeat: true,
            close: true,
        });
        Output {
            functions: (0..functions.len()).map(|id| {
                let function_type = &function_types[functions[id] as usize];
                let results_len = function_type.results.len();
                let parameters_len = function_type.parameters.len();
                let mut items = Vec::with_capacity(results_len + parameters_len);
                for i in 0..results_len {
                    items.push(function_result(i as u32));
                }
                for i in 0..parameters_len {
                    items.push(function_parameter(i as u32, results_len as u32));
                }
                create_line(names::function(id as u32), chest_args(items))
            }).collect(),
            init,
        }
    }
}