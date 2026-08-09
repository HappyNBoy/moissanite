use crate::{fmt_eager, fmt_name};
use crate::output::{chest_args, var_block, ChestSlot, ItemData, Name, OutputLine, VarAction, VarScope};
use crate::types::ValueType;

pub struct RegisterManager {
    free_regs: Vec<u32>,
    reg_count: u32,
}

impl RegisterManager {
    pub fn alloc_reg(&mut self) -> u32 {
        match self.free_regs.pop() {
            Some(reg) => reg,
            None => {
                let out = self.reg_count;
                self.reg_count += 1;
                out
            }
        }
    }

    pub fn free_reg(&mut self, reg: u32) {
        debug_assert!(!self.free_regs.contains(&reg));
        self.free_regs.push(reg);
    }
}

pub struct StackEntry {
    pub value: Variable,
    pub kind: ValueType,
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum TrueVariable {
    Register(u32),
    Local(u32),
    Global(u32),
}

#[derive(Debug, Clone)]
pub enum Variable {
    TrueVariable(TrueVariable),
    Addition(Box<[Variable; 2]>),
}

impl From<TrueVariable> for ItemData {
    fn from(value: TrueVariable) -> Self {
        match value {
            TrueVariable::Register(x) => ItemData::Variable {
                name: Name::register(x),
                scope: VarScope::Line
            },
            TrueVariable::Local(x) => ItemData::Variable {
                name: Name::local(x),
                scope: VarScope::Line
            },
            TrueVariable::Global(x) => ItemData::Variable {
                name: Name::global(x),
                scope: VarScope::Global
            },
        }
    }
}

impl From<TrueVariable> for Name {
    fn from(value: TrueVariable) -> Self {
        fmt_eager!("%var({a})", match value {
            TrueVariable::Register(x) => Name::register(x),
            TrueVariable::Local(x) => Name::local(x),
            TrueVariable::Global(x) => Name::global(x),
        })
    }
}

pub struct FnState {
    pub line: OutputLine,
    pub regs: RegisterManager,
    pub stack: Vec<StackEntry>,
    pub locals: Vec<ValueType>
}

impl FnState {
    pub const fn new(line: OutputLine, locals: Vec<ValueType>) -> Self {
        Self {
            line,
            locals,
            regs: RegisterManager {
                free_regs: Vec::new(),
                reg_count: 0,
            },
            stack: Vec::new(),
        }
    }

    pub fn consume(self) -> OutputLine {
        self.line
    }

    /// Creates a Name that evaluates to the variable by using DF percent codes.
    fn var_code(&self, var: &Variable) -> Name {
        match var {
            &Variable::TrueVariable(x) => x.into(),
            Variable::Addition(v) => fmt_eager!(
                "%math({a},{b})",
                self.var_code(&v[0]), self.var_code(&v[1])
            ),
        }
    }

    /// Creates a DF item that evaluates to the variable.
    pub fn make_item(&self, var: &Variable) -> ItemData {
        if let &Variable::TrueVariable(x) = var {
            x.into()
        } else {
            ItemData::Number { value: self.var_code(var) }
        }
    }

    /// Converts a variable to any TrueVariable form by assigning it
    /// to a free register if it's not evaluated yet. Used in cases of
    /// reusing a variable multiple times to avoid duplicate calculations.
    pub fn cache(&mut self, var: &Variable) -> TrueVariable {
        if let Variable::TrueVariable(x) = var {
            *x
        } else {
            let reg = TrueVariable::Register(self.regs.alloc_reg());
            self.assign(reg, var);
            reg
        }
    }

    /// Assigns the value of a variable to a true variable.
    pub fn assign(&mut self, dest: TrueVariable, src: &Variable) {
        match src {
            &Variable::TrueVariable(src) => {
                if dest != src {
                    self.line.push(var_block(VarAction::Set, chest_args(vec![
                        ChestSlot { slot: 0, item: dest.into() },
                        ChestSlot { slot: 1, item: src.into() },
                    ])));
                }
            },
            Variable::Addition(v) => {
                self.line.push(var_block(VarAction::Sum, chest_args(vec![
                    ChestSlot { slot: 0, item: dest.into() },
                    ChestSlot { slot: 1, item: self.make_item(&v[0]) },
                    ChestSlot { slot: 2, item: self.make_item(&v[1]) },
                ])));
            },
        }
    }
}