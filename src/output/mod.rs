pub mod structs;
pub mod names;

pub use structs::*;
pub use names::Name;
use anyhow::Result;
use crate::types::{FunctionType, Value};

fn create_line(name: Name, args: ChestArgs) -> OutputLine {
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

pub fn chest_item(slot: usize, item: ItemData) -> ChestSlot {
    ChestSlot { slot, item }
}

pub fn function_result(i: usize) -> ChestSlot {
    chest_item(i, ItemData::Parameter {
        name: names::stack(i), // the result variables are automatically the bottom of the stack
        optional: false,
        plural: false,
        param_type: ParameterType::Variable,
    })
}

pub fn function_parameter(i: usize, results: usize) -> ChestSlot {
    chest_item(results + i, ItemData::Parameter {
        name: names::local(i),
        optional: false,
        plural: false,
        param_type: ParameterType::Number,
    })
}

pub fn var_item(i: usize, name: Name, scope: VarScope) -> ChestSlot {
    chest_item(i, ItemData::Variable { name, scope })
}

pub fn num_item(i: usize, value: Name) -> ChestSlot {
    chest_item(i, ItemData::Number { value })
}

impl OutputLine {
    pub const fn new() -> OutputLine {
        OutputLine { blocks: Vec::new() }
    }

    pub fn push(&mut self, block: CodeBlock) {
        self.blocks.push(block);
    }

    pub fn create_list(&mut self, name: Name, scope: VarScope, values: impl Iterator<Item = Result<ItemData>>) -> Result<()> {
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
        let mut init = create_line(names::INIT_FN.with(Name::clone), ChestArgs::empty());
        // fill a blank page for the easier creation of new pages
        let blank_item = var_item(0, names::BLANK_PAGE.with(Name::clone), VarScope::Global);
        init.push(var_block(VarAction::CreateList, chest_args(vec![
            blank_item.clone()
        ])));
        init.push(CodeBlock::Block(CodeBlockInner::Repeat {
            action: RepeatAction::Multiple,
            args: chest_args(vec![
                num_item(0, names::PAGE_SIZE.with(Name::clone))
            ]),
        }));
        init.push(CodeBlock::Bracket {
            repeat: true,
            close: false,
        });
        init.push(var_block(VarAction::AppendValue, chest_args(vec![
            blank_item,
            num_item(1, names::ZERO.with(Name::clone)),
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
                    items.push(function_result(i));
                }
                for i in 0..parameters_len {
                    items.push(function_parameter(i, results_len));
                }
                create_line(names::function(id), chest_args(items))
            }).collect(),
            init,
        }
    }

    pub fn init_global_list(&mut self, name: Name) {
        self.init.push(var_block(VarAction::CreateList, chest_args(vec![
            var_item(0, name, VarScope::Global),
        ])))
    }
}