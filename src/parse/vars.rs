use crate::output::{chest_args, names, var_block, ChestSlot, ItemData, OutputLine, VarAction, VarScope};
use crate::output::structs::Tag;
use crate::types::ValueType;
use std::fmt::Write;
use moissanite_macros::define_vars;

pub struct RegisterManager {
    free_regs: Vec<u32>,
    pending_frees: Vec<u32>,
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

    pub fn mark_frees(&self) -> usize {
        self.pending_frees.len()
    }

    pub fn schedule_free(&mut self, reg: u32) {
        debug_assert!(!self.free_regs.contains(&reg));
        debug_assert!(!self.pending_frees.contains(&reg));
        self.pending_frees.push(reg);
    }

    pub fn flush_frees(&mut self, mark: usize) {
        while self.pending_frees.len() > mark {
            let reg = self.pending_frees.pop().unwrap();
            self.free_reg(reg);
        }
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
                pending_frees: Vec::new(),
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
            self.regs.schedule_free(x);
        }
        var.to_code(f);
    }

    fn true_item(&mut self, var: TrueVariable) -> ItemData {
        if let TrueVariable::Register(x) = var {
            self.regs.schedule_free(x);
        }
        var.to_item()
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
}

define_vars! {
    Addition(2) => {
        Sum($0, $1);
        "%math($0+$1)"
    },
    I32ShrU(2) => Bitwise($0, $1) [BitwiseTrue, BitwiseShrU],

    #[cfg(test)]
    TestSub(2) => "%math($0-$1)",

    #[cfg(test)]
    TestRotl32(2) => {
        #0 = Bitwise($0, "%math($1%32)") [BitwiseTrue, BitwiseShrU];
        #1 = Bitwise($0, "%math(32-($1%32))") [BitwiseTrue, BitwiseShrU];
        Sum(#0, #1)
    },

    #[cfg(test)]
    TestShiftThenAdd(2) => {
        #0 = Bitwise($0, $1) [BitwiseTrue, BitwiseShrU];
        "%math(#0+$0)"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::output::structs::CodeBlock;
    use crate::output::structs::CodeBlockInner;

    #[test]
    fn test_dual_mode_add_assign_vs_var_code() {
        // Direct assign of Addition should emit a single `VarAction::Sum` block
        let mut state = FnState::new(OutputLine::new(), vec![ValueType::I32, ValueType::I32]);
        let expr = Variable::Addition(Box::new([
            Variable::TrueVariable(TrueVariable::Local(0)),
            Variable::TrueVariable(TrueVariable::Local(1)),
        ]));
        state.assign(TrueVariable::Register(0), &expr);
        assert_eq!(state.line.blocks.len(), 1);
        let CodeBlock::Block(CodeBlockInner::SetVariable { action, args }) = &state.line.blocks[0] else {
            panic!("expected SetVariable block");
        };
        assert!(*action == VarAction::Sum);
        assert_eq!(args.items.len(), 3);

        // Nested Addition inside another Addition should format using `%math(...)`
        let mut state2 = FnState::new(OutputLine::new(), vec![ValueType::I32, ValueType::I32]);
        let nested = Variable::Addition(Box::new([
            expr,
            Variable::TrueVariable(TrueVariable::Local(0)),
        ]));
        let item = state2.make_item(&nested);
        assert!(state2.line.blocks.is_empty());
        let ItemData::Number { value } = item else {
            panic!("expected Number item");
        };
        assert_eq!(value, "%math(%math(%var(_ml_0)+%var(_ml_1))+%var(_ml_0))");
    }

    #[test]
    fn test_sibling_operands_do_not_clobber_registers() {
        // Addition(I32ShrU(l0, l1), I32ShrU(l0, l1))
        // The first I32ShrU allocates _mr_0; the second I32ShrU must allocate _mr_1 (not clobber _mr_0!),
        // and then when assigned to a destination, both _mr_0 and _mr_1 are freed before dest is allocated.
        let mut state = FnState::new(OutputLine::new(), vec![ValueType::I32, ValueType::I32]);
        let shr1 = Variable::I32ShrU(Box::new([
            Variable::TrueVariable(TrueVariable::Local(0)),
            Variable::TrueVariable(TrueVariable::Local(1)),
        ]));
        let shr2 = Variable::I32ShrU(Box::new([
            Variable::TrueVariable(TrueVariable::Local(0)),
            Variable::TrueVariable(TrueVariable::Local(1)),
        ]));
        let sum = Variable::Addition(Box::new([shr1, shr2]));
        state.assign(TrueVariable::Local(0), &sum);

        // Should have 3 blocks: _mr_0 = l0 >>> l1, _mr_1 = l0 >>> l1, _ml_0 = _mr_0 + _mr_1
        assert_eq!(state.line.blocks.len(), 3);
        let CodeBlock::Block(CodeBlockInner::SetVariable { action, args }) = &state.line.blocks[2] else {
            panic!("expected SetVariable");
        };
        assert!(*action == VarAction::Sum);
        let ItemData::Number { value: v1 } = &args.items[0].item else { panic!() };
        let ItemData::Number { value: v2 } = &args.items[1].item else { panic!() };
        assert_eq!(v1, "%var(_mr_0)");
        assert_eq!(v2, "%var(_mr_1)");
        // Both _mr_0 and _mr_1 must now be free!
        assert_eq!(state.regs.reg_count, 2);
        assert_eq!(state.regs.free_regs.len(), 2);
        assert!(state.regs.pending_frees.is_empty());
    }

    #[test]
    fn test_multi_block_caching_and_temp_registers() {
        let mut state = FnState::new(OutputLine::new(), vec![ValueType::I32, ValueType::I32]);

        // Pass a compound expression as $0 to TestRotl32 so it must be cached once into a register
        let compound_arg0 = Variable::TestSub(Box::new([
            Variable::TrueVariable(TrueVariable::Local(0)),
            Variable::TrueVariable(TrueVariable::Local(1)),
        ]));
        let rotl = Variable::TestRotl32(Box::new([
            compound_arg0,
            Variable::TrueVariable(TrueVariable::Local(1)),
        ]));

        state.assign(TrueVariable::Local(0), &rotl);

        // Expected blocks:
        // 1. Cache $0: _mr_0 = %math(_ml_0 - _ml_1) (VarAction::Set)
        // 2. #0: _mr_1 = Bitwise(_mr_0, "%math(%var(_ml_1)%32)")
        // 3. #1: _mr_0 = Bitwise(_mr_0, "%math(32-(%var(_ml_1)%32))") (reuses _mr_0 since $0 is freed after #1's inputs!)
        // 4. Final: _ml_0 = Sum(#0 (_mr_1), #1 (_mr_0))
        assert_eq!(state.line.blocks.len(), 4);
        // Only 2 physical registers needed in total, and all registers freed at the end!
        assert_eq!(state.regs.reg_count, 2);
        assert_eq!(state.regs.free_regs.len(), 2);
        assert!(state.regs.pending_frees.is_empty());
    }

    #[test]
    fn test_setup_blocks_with_var_code() {
        let mut state = FnState::new(OutputLine::new(), vec![ValueType::I32, ValueType::I32]);

        // TestShiftThenAdd nested inside TestSub
        let inner = Variable::TestShiftThenAdd(Box::new([
            Variable::TrueVariable(TrueVariable::Local(0)),
            Variable::TrueVariable(TrueVariable::Local(1)),
        ]));
        let outer = Variable::TestSub(Box::new([
            inner,
            Variable::TrueVariable(TrueVariable::Local(1)),
        ]));

        state.assign(TrueVariable::Local(0), &outer);

        // Expected blocks:
        // 1. Setup block from TestShiftThenAdd: _mr_0 = Bitwise(_ml_0, _ml_1)
        // 2. Final Set block from TestSub: _ml_0 = %math(%math(%var(_mr_0)+%var(_ml_0))-%var(_ml_1))
        assert_eq!(state.line.blocks.len(), 2);
        let CodeBlock::Block(CodeBlockInner::SetVariable { action, args }) = &state.line.blocks[1] else {
            panic!("expected SetVariable");
        };
        assert!(*action == VarAction::Set);
        let ItemData::Number { value } = &args.items[1].item else {
            panic!("expected Number item in slot 1");
        };
        assert_eq!(value, "%math(%math(%var(_mr_0)+%var(_ml_0))-%var(_ml_1))");
        assert_eq!(state.regs.reg_count, 1);
        assert_eq!(state.regs.free_regs.len(), 1);
        assert!(state.regs.pending_frees.is_empty());
    }
}
