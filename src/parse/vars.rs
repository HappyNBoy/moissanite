use crate::output::{chest_args, names, var_block, ChestSlot, ItemData, OutputLine, VarAction, VarScope};
use crate::types::ValueType;
use std::fmt::Write;

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
                name: names::register(x),
                scope: VarScope::Line
            },
            TrueVariable::Local(x) => ItemData::Variable {
                name: names::local(x),
                scope: VarScope::Line
            },
            TrueVariable::Global(x) => ItemData::Variable {
                name: names::global(x),
                scope: VarScope::Global
            },
        }
    }
}

impl TrueVariable {
    fn to_code(&self, f: &mut String) {
        // this allocates a String unnecessarily, but I don't really care atm
        // if perf is an issue then change to manual format calls
        write!(f, "%var({})", match *self {
            TrueVariable::Register(x) => names::register(x),
            TrueVariable::Local(x) => names::local(x),
            TrueVariable::Global(x) => names::global(x),
        }).expect("String shouldn't error on write");
    }
}

pub struct FnState {
    pub line: OutputLine,
    pub regs: RegisterManager,
    pub stack: Vec<StackEntry>,
    pub locals: Vec<ValueType>
}

#[macro_use]
mod codegen {
    use super::{FnState, Variable};

    pub(super) enum Argument {
        String(&'static str),
        Variable(usize),
    }

    pub(super) const fn arg_len(template: &str) -> usize {
        let bytes = template.as_bytes();
        let mut i: usize = 0;
        let mut start: usize = 0;
        let mut out: usize = 1;
        while i < bytes.len() {
            if bytes[i] == b'$' {
                if start != i {
                    out += 1;
                }
                out += 1;
                start = i + 1;
            }
            i += 1;
        }
        out
    }

    const fn const_slice(s: &str, start: usize, end: usize) -> &str {
        assert!(start <= end && end <= s.len());
        unsafe {
            let ptr = s.as_ptr().add(start);
            let len = end - start;
            let bytes = std::slice::from_raw_parts(ptr, len);
            match std::str::from_utf8(bytes) {
                Ok(x) => x,
                Err(_) => panic!("expected valid utf-8")
            }
        }
    }

    pub(super) const fn make_args(template: &'static str, args: &mut [Argument]) {
        let bytes = template.as_bytes();
        let mut args_at: usize = 0;
        let mut i: usize = 0;
        let mut start: usize = 0;
        while i < bytes.len() {
            if bytes[i] == b'$' {
                if start < i {
                    args[args_at] = Argument::String(const_slice(template, start, i));
                    args_at += 1;
                }
                i += 1;
                assert!(i < bytes.len());
                let var = bytes[i];
                assert!(b'0' <= var && var <= b'9');
                args[args_at] = Argument::Variable((var - b'0') as usize);
                args_at += 1;
                start = i + 1;
            }
            i += 1;
        }
        if start < i {
            args[args_at] = Argument::String(const_slice(template, start, i));
        }
    }

    #[inline(always)]
    pub(super) fn fmt_inner<const N: usize>(f: &mut String, state: &mut FnState, args: [Argument; N], i: usize, vars: &[Variable]) {
        if i < N {
            match args[i] {
                Argument::String(s) => f.push_str(s),
                Argument::Variable(var) => state.var_code(f, &vars[var])
            }
        }
    }

    macro_rules! unroll_args {
        ($args:expr, $f:expr, $state:expr, $vars:expr, [$($i:literal),*]) => {
            $(
                codegen::fmt_inner($f, $state, $args, $i, $vars);
            )*
        }
    }

    macro_rules! fmt_var {
        ($f:expr, $state:expr, $template:literal, $vars:expr) => {
            const ARG_LEN: usize = codegen::arg_len($template);
            const { assert!(ARG_LEN <= 32); }
            const ARGS: [codegen::Argument; ARG_LEN] = {
                let mut arr = [const { codegen::Argument::Variable(0) }; ARG_LEN];
                codegen::make_args($template, &mut arr);
                arr
            };
            unroll_args!(ARGS, $f, $state, $vars,
            [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16,
             17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31]);
        };
    }
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
    fn var_code(&mut self, f: &mut String, var: &Variable) {
        macro_rules! p {
            ($template:literal, $vars:expr) => {{
                let vars = $vars;
                fmt_var!(f, self, $template, vars);
            }};
        }
        match var {
            &Variable::TrueVariable(x) => x.to_code(f),
            Variable::Addition(v) => p!("%math($0+$1)", &**v),
            #[allow(unreachable_patterns)] // future instruction implementations might use it
            other => self.cache(other).to_code(f)
        }
    }

    /// Creates a DF item that evaluates to the variable.
    pub fn make_item(&mut self, var: &Variable) -> ItemData {
        if let &Variable::TrueVariable(x) = var {
            x.into()
        } else {
            let mut value = String::new();
            self.var_code(&mut value, var);
            ItemData::Number { value }
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
                let args = chest_args(vec![
                    ChestSlot { slot: 0, item: dest.into() },
                    ChestSlot { slot: 1, item: self.make_item(&v[0]) },
                    ChestSlot { slot: 2, item: self.make_item(&v[1]) },
                ]);
                self.line.push(var_block(VarAction::Sum, args));
            },
        }
    }
}