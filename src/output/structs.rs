use serde::{Serialize, Serializer};

pub struct Output {
    pub functions: Box<[OutputLine]>,
    pub init: OutputLine,
}

#[derive(Serialize, Default)]
pub struct OutputLine {
    pub blocks: Vec<CodeBlock>,
}

impl From<Vec<CodeBlock>> for OutputLine {
    fn from(vec: Vec<CodeBlock>) -> OutputLine {
        OutputLine { blocks: vec }
    }
}

#[derive(Serialize)]
#[serde(tag = "block")]
pub enum CodeBlockInner {
    #[serde(rename = "func")]
    Function {
        data: String,
        args: ChestArgs,
    },
    #[serde(rename = "set_var")]
    SetVariable {
        action: VarAction,
        args: ChestArgs
    },
    #[serde(rename = "repeat")]
    Repeat {
        action: RepeatAction,
        args: ChestArgs,
    },
}

#[derive(Serialize, Copy, Clone, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub enum VarAction {
    #[serde(rename = "=")]
    Set,
    CreateList,
    TrimList,
    AppendValue,
    SetListValue,
    GetListValue,
    #[serde(rename = "+")]
    Sum,
}

#[derive(Serialize, Copy, Clone, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub enum RepeatAction {
    Multiple,
}

fn serialize_bracket_type<S: Serializer>(value: &bool, serializer: S) -> anyhow::Result<S::Ok, S::Error> {
    if *value {
        serializer.serialize_str("repeat")
    } else {
        serializer.serialize_str("norm")
    }
}

fn serialize_bracket_direction<S: Serializer>(value: &bool, serializer: S) -> anyhow::Result<S::Ok, S::Error> {
    if *value {
        serializer.serialize_str("close")
    } else {
        serializer.serialize_str("open")
    }
}

#[derive(Serialize)]
#[serde(tag = "id", rename_all = "lowercase")]
pub enum CodeBlock {
    Bracket {
        #[serde(rename = "type", serialize_with = "serialize_bracket_type")]
        repeat: bool,
        #[serde(rename = "direct", serialize_with = "serialize_bracket_direction")]
        close: bool,
    },
    Block (CodeBlockInner)
}

#[derive(Serialize)]
pub struct ChestArgs {
    pub items: Vec<ChestSlot>
}

impl ChestArgs {
    pub(crate) const fn empty() -> ChestArgs {
        ChestArgs { items: Vec::new() }
    }
}

#[derive(Serialize, Clone)]
pub struct ChestSlot {
    pub slot: u32,
    pub item: ItemData,
}

#[derive(Serialize, Clone)]
#[serde(tag = "id", content = "data")]
pub enum ItemData {
    #[serde(rename = "num")]
    Number {
        #[serde(rename = "name")]
        value: String,
    },
    #[serde(rename = "var")]
    Variable {
        name: String,
        scope: VarScope,
    },
    #[serde(rename = "pn_el")]
    Parameter {
        name: String,
        optional: bool,
        plural: bool,
        #[serde(rename = "type")]
        param_type: ParameterType,
    },
}

#[derive(Serialize, Copy, Clone)]
pub enum ParameterType {
    #[serde(rename = "var")]
    Variable,
    #[serde(rename = "num")]
    Number,
}

#[derive(Serialize, Copy, Clone)]
#[serde(rename_all = "lowercase")]
pub enum VarScope {
    Saved,
    #[serde(rename = "unsaved")]
    Global,
    Local,
    Line,
}