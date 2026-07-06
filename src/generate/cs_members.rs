use bitflags::bitflags;
use brocolib::global_metadata::MethodIndex;

use crate::data::type_resolver::ResolvedType;

use std::hash::Hash;

use super::cs_type_tag::CsTypeTag;

#[derive(Debug, Eq, Hash, PartialEq, Clone, Default)]
pub struct CsGenericContainer {
    pub args: Vec<CsGenericArg>,
}

#[derive(Debug, Eq, Hash, PartialEq, Clone, Default)]
pub struct CsGenericArg {
    pub constraints: Vec<CsGenericConstraint>,
    pub name: String,
    pub index: u16,
}

pub type CsGenericConstraint = ResolvedType;

impl CsGenericContainer {
    pub fn just_names(&self) -> impl Iterator<Item = &String> {
        self.args.iter().map(|t| &t.name)
    }
}

#[derive(Clone, Debug, Default, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct CsMethodData {
    pub estimated_size: Option<usize>,
    pub addrs: Option<u64>,
    pub slot: Option<u16>,
}

#[derive(Clone, Debug, PartialEq, PartialOrd)]
pub enum CsValue {
    String(String),
    Char(String),
    Bool(bool),

    U8(u8),
    U16(u16),
    U32(u32),
    U64(u64),

    I8(i8),
    I16(i16),
    I32(i32),
    I64(i64),

    F32(f32),
    F64(f64),

    Null,
}

/// Explicit layout
/// il2cpp basically turns each field into 2 structs within a union:
/// 1 which is packed with size 1, and padded with offset to fit to the end
/// the other which has the same padding and layout, except this one is for alignment so it's just packed as the parent struct demands
/// union {
///      [[pack(1)]]
///      struct {
///          byte __field_padding = size(offset)
///          T field
///      }
///      [[pack(default)]]
///      struct {
///          byte __field_padding_forAlignment = size(offset)
///          T __field_forAlignment
///      }... per field
/// }
///
#[derive(Clone, Debug, PartialEq)]
pub struct CsField {
    pub name: String,
    pub field_ty: ResolvedType,
    pub instance: bool,
    pub readonly: bool,
    // is C# const (constant evaluated)
    // could be assumed from value though
    pub is_const: bool,

    pub offset: Option<u32>,
    pub size: usize,

    pub value: Option<CsValue>,
    pub brief_comment: Option<String>,
}

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub struct CsProperty {
    pub name: String,
    pub prop_ty: ResolvedType,
    pub instance: bool,
    pub getter: Option<(MethodIndex, String)>,
    pub setter: Option<(MethodIndex, String)>,
    /// Whether this property is one that's indexable (accessor methods take an index argument)
    pub indexable: bool,
    pub brief_comment: Option<String>,
}

bitflags! {
    #[derive(Debug, Clone, Hash, PartialEq, PartialOrd, Eq, Ord)]
    pub struct CsParamFlags: u8 {
        const REF = 1;
        const IN = 1 << 1;
        const OUT = 1 << 2;
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct CsParam {
    pub name: String,
    pub il2cpp_ty: ResolvedType,
    // TODO: Use bitflags to indicate these attributes
    // May hold:
    // const
    // May hold one of:
    // *
    // &
    // &&
    pub modifiers: CsParamFlags,
    pub def_value: Option<CsValue>,
}

bitflags! {
    #[derive(Clone, Debug, PartialEq)]
    pub struct CSMethodFlags: u32 {
        const STATIC = 0b00000001;
        const VIRTUAL = 0b00000010;
        const OPERATOR = 0b00000100;
        const ABSTRACT = 0b00001000;
        const OVERRIDE = 0b00010000;
        const FINAL = 0b00100000;
        const SPECIAL_NAME = 0b01000000;
        const UNSAFE = 0b10000000;
    }
}

// TODO: Generics
#[derive(Clone, Debug, PartialEq)]
pub struct CsMethod {
    pub name: String,
    pub method_index: MethodIndex,
    pub return_type: ResolvedType,
    pub parameters: Vec<CsParam>,
    pub instance: bool,
    pub template: Option<CsGenericContainer>,
    pub brief: Option<String>,

    pub declaring_type: CsTypeTag,

    pub method_data: CsMethodData,
    pub method_flags: CSMethodFlags,
    /// if this method is a generic instantiation, the types used to instantiate it
    /// are stored here
    pub generic_instatiation: Option<Vec<ResolvedType>>,
}

// TODO: Generics
#[derive(Clone, Debug)]
pub struct CsConstructor {
    pub name: String,
    pub parameters: Vec<CsParam>,
    pub template: Option<CsGenericContainer>,
}

impl PartialEq for CsConstructor {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name
            && self.parameters == other.parameters
            && self.template == other.template
    }
}
