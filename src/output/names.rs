use std::fmt::{Display, Formatter};
use std::rc::Rc;
use serde::{Serialize, Serializer};
use crate::types::Value;

#[derive(Clone)]
pub struct Name(pub Rc<dyn Display>);

impl Display for Name {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

#[macro_export] macro_rules! fmt_name {
    ($($arg:tt)*) => {
        Name(::std::rc::Rc::new(::std::fmt::from_fn(move |f| write!(f, $($arg)*))))
    };
}

#[macro_export] macro_rules! fmt_eager {
    ($fmt:expr, $a:expr) => {{
        let a = $a;
        fmt_name!($fmt, a=a)
    }};
    ($fmt:expr, $a:expr, $b:expr) => {{
        let a = $a;
        let b = $b;
        fmt_name!($fmt, a=a, b=b)
    }};
 }

impl Serialize for Name {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(&self.0)
    }
}

thread_local! {
    static CONST_I: Rc<&'static str> = Rc::new("i");
    static CONST_TMP: Rc<&'static str> = Rc::new("_m_tmp");
    static CONST_BLANK: Rc<&'static str> = Rc::new("_m_blank");
    static CONST_INIT_FN: Rc<&'static str> = Rc::new("_mf_init");
}

impl Name {
    pub fn local(x: u32) -> Name {
        fmt_name!("_ml_{x}")
    }

    pub fn global(x: u32) -> Name {
        fmt_name!("_mg_{x}")
    }

    pub fn register(x: u32) -> Name {
        fmt_name!("_mr_{x}")
    }

    pub fn function(x: u32) -> Name {
        fmt_name!("_mf_{x}")
    }

    pub fn table(x: u32) -> Name {
        fmt_name!("_mt_{x}")
    }

    pub fn memory(x: u32, y: u32) -> Name {
        fmt_name!("_mm_{x}_{y}")
    }

    pub fn integer(x: u32) -> Name {
        fmt_name!("{x}")
    }

    pub fn value(x: Value) -> Name {
        Name(Rc::new(x))
    }

    pub fn const_i() -> Name {
        Name(CONST_I.with(Rc::clone))
    }

    pub fn const_tmp() -> Name {
        Name(CONST_TMP.with(Rc::clone))
    }

    pub fn const_blank() -> Name {
        Name(CONST_BLANK.with(Rc::clone))
    }

    pub fn const_init_fn() -> Name {
        Name(CONST_INIT_FN.with(Rc::clone))
    }
}









// use std::fmt::{Display, Formatter, Write};
// use serde::{Serialize, Serializer};
// use crate::types::Value;
//
// // Names which do not reference other names and do not involve runtime calculations
// #[derive(Clone)]
// pub enum CName {
//     // Constants
//     ConstI,
//     ConstTmp,
//     ConstZero,
//     ConstBlank,
//     ConstInitFn,
//
//     // Value
//     Value(Value),
//     CountValue(u32),
//
//     // Variables
//     Local(u32),
//     Global(u32),
//     Register(u32),
//     Table(u32),
//     Memory(u32, u32),
//     Function(u32),
// }
//
// // General names
// #[derive(Clone)]
// pub enum Name {
//     // CName
//     CName(CName),
//
//     // Constant Codes
//     VarCode(CName),
//     AddIOffset(u32),
//     MemoryI(u32),
//     IndexI(CName),
//
//     // Dynamic Codes
//     MathAdd(Box<Name>, Box<Name>),
// }
//
// impl From<CName> for Name {
//     fn from(value: CName) -> Self {
//         Name::CName(value)
//     }
// }
//
// impl Display for CName {
//     fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
//         match self {
//             CName::ConstI => f.write_str("_m_i"),
//             CName::ConstTmp => f.write_str("_m_tmp"),
//             CName::ConstZero => f.write_char('0'),
//             CName::ConstBlank => f.write_str("_m_blank"),
//             CName::ConstInitFn => f.write_str("_mf_init"),
//
//             CName::Value(v) => v.fmt(f),
//             CName::CountValue(x) => x.fmt(f),
//
//             CName::Local(x) => write!(f, "_ml_{x}"),
//             CName::Global(x) => write!(f, "_mg_{x}"),
//             CName::Register(x) => write!(f, "_mr_{x}"),
//             CName::Table(x) => write!(f, "_mt_{x}"),
//             CName::Memory(x, y) => write!(f, "_mm_{x}_{y}"),
//             CName::Function(x) => write!(f, "_mf_{x}"),
//         }
//     }
// }
//
// impl Display for Name {
//     fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
//         match self {
//             Name::CName(x) => x.fmt(f),
//
//             Name::VarCode(val) => write!(f, "%var({val})"),
//             Name::AddIOffset(x) => write!(f, "%math(%var({})+{x})", CName::ConstI),
//             Name::MemoryI(i) => write!(f, "_mm_{i}_%var({})", CName::ConstI),
//             Name::IndexI(x) => write!(f, "%index({x},{})", CName::ConstI),
//
//             Name::MathAdd(a, b) => write!(f, "%math({a}+{b})"),
//         }
//     }
// }
//
// impl Serialize for Name {
//     fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
//         serializer.collect_str(self)
//     }
// }