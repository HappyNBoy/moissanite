use std::fmt;
use crate::types::Value;
use std::fmt::Display;

pub const I: &str = "i";
pub const TMP: &str = "_m_tmp";
pub const BLANK: &str = "_m_blank";
pub const INIT_FN: &str = "_mf_init";

pub fn local(x: u32) -> String {
    format!("_ml_{x}")
}

pub fn global(x: u32) -> String {
    format!("_mg_{x}")
}

pub fn register(x: u32) -> String {
    format!("_mr_{x}")
}

pub fn function(x: u32) -> String {
    format!("_mf_{x}")
}

pub fn table(x: u32) -> String {
    format!("_mt_{x}")
}

pub fn memory(x: &impl Display, y: &impl Display) -> String {
    format!("_mm_{x}_{y}")
}

pub fn integer(x: u32) -> String {
    format!("{x}")
}

pub fn value(x: Value) -> String {
    format!("{x}")
}

pub fn p_var(x: &impl Display) -> impl Display {
    fmt::from_fn(move |f| write!(f, "%var({x})"))
}