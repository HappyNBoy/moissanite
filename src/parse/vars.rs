use crate::output::{chest_args, names, var_block, ChestSlot, ItemData, OutputLine, VarAction, VarScope};
use crate::output::structs::Tag;
use crate::types::ValueType;
use std::fmt::Write;

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

    macro_rules! fmt_var {
        ($f:expr, $state:expr, $template:literal, $vars:expr) => {
            const ARG_LEN: usize = codegen::arg_len($template);
            const { assert!(ARG_LEN <= 32); }
            const ARGS: [codegen::Argument; ARG_LEN] = {
                let mut arr = [const { codegen::Argument::Variable(0) }; ARG_LEN];
                codegen::make_args($template, &mut arr);
                arr
            };
            seq_macro::seq!(I in 0..32 {
                codegen::fmt_inner($f, $state, ARGS, I, $vars);
            });
        };
    }
}

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
    I32ShrU(Box<[Variable; 2]>),
}

impl TrueVariable {
    /// If you want to free the register, use `FnState.true_code` instead.
    fn to_code(self, f: &mut String) {
        // this allocates a String unnecessarily, but I don't really care atm
        // if perf is an issue then change to manual format calls
        write!(f, "%var({})", match self {
            TrueVariable::Register(x) => names::register(x),
            TrueVariable::Local(x) => names::local(x),
            TrueVariable::Global(x) => names::global(x),
        }).expect("String shouldn't error on write");
    }

    /// If you want to free the register, use `FnState.true_item` instead.
    fn to_item(self) -> ItemData {
        match self {
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

    fn true_code(&mut self, f: &mut String, var: TrueVariable) {
        if let TrueVariable::Register(x) = var {
            self.regs.free_reg(x);
        }
        var.to_code(f);
    }

    fn true_item(&mut self, var: TrueVariable) -> ItemData {
        if let TrueVariable::Register(x) = var {
            self.regs.free_reg(x);
        }
        var.to_item()
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
            &Variable::TrueVariable(x) => self.true_code(f, x),
            Variable::Addition(v) => p!("%math($0+$1)", &**v),
            #[allow(unreachable_patterns)] // future instruction implementations might use it
            other => {
                let cached = self.cache(other);
                self.true_code(f, cached);
            }
        }
    }

    /// Creates a DF item that evaluates to the variable.
    pub fn make_item(&mut self, var: &Variable) -> ItemData {
        if let &Variable::TrueVariable(x) = var {
            self.true_item(x)
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
            let mut reg = 0; // placeholder
            self.assign_inner(|this| {
                reg = this.regs.alloc_reg();
                TrueVariable::Register(reg).to_item()
            }, var);
            TrueVariable::Register(reg)
        }
    }

    /// Assigns the value of a variable to a true variable.
    pub fn assign(&mut self, dest: TrueVariable, src: &Variable) {
        match src {
            &Variable::TrueVariable(x) if x == dest => {},
            src => self.assign_inner(|_| dest.to_item(), src)
        }
    }

    fn assign_inner(&mut self, mut dest: impl FnMut(&mut Self) -> ItemData, src: &Variable) {
        macro_rules! args {
            ($vars:expr $(,$tag:ident)*) => {{
                const TAGS_LEN: usize = <[&str]>::len(&[$(stringify!($tag)),*]);
                let mut vec = Vec::with_capacity($vars.len() + 1 + TAGS_LEN);
                // this should unroll, if it doesn't and causes issues
                // then replace with `seq!` and pass an integer literal
                for i in 0..$vars.len() {
                    vec.push(ChestSlot { slot: (i + 1) as u32, item: self.make_item(&$vars[i]) });
                }
                vec.push(ChestSlot { slot: 0, item: dest(self) });
                #[allow(unused)]
                let mut i = 26 - (TAGS_LEN as u32);
                $(
                    vec.push(ChestSlot { slot: { i += 1; i }, item: ItemData::Tag(Tag::$tag) });
                )*
                chest_args(vec)
            }};
        }
        macro_rules! vb {
            ($vars:expr, $action:ident $(,$tag:ident)*) => {{
                let args = args!($vars $(,$tag)*);
                self.line.push(var_block(VarAction::$action, args));
            }};
        }
        match src {
            &Variable::TrueVariable(src) => {
                let args = chest_args(vec![
                    ChestSlot { slot: 0, item: dest(self) },
                    ChestSlot { slot: 1, item: self.true_item(src) },
                ]);
                self.line.push(var_block(VarAction::Set, args));
            },
            Variable::Addition(v) => vb!(v, Sum),
            Variable::I32ShrU(v) => vb!(v, Bitwise, BitwiseTrue, BitwiseShrU),
        }
    }
}