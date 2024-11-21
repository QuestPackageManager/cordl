use bitflags::bitflags;
use brocolib::global_metadata::{MethodIndex, TypeIndex};
use bytes::Bytes;
use itertools::Itertools;

use crate::data::type_resolver::ResolvedType;

use super::{cs_type_tag::CsTypeTag, writer::CppWritable};

use std::{hash::Hash, num, rc::Rc, sync::Arc};

#[derive(Debug, Eq, Hash, PartialEq, Clone, Default, PartialOrd, Ord)]
pub struct CsGenericTemplate {
    pub names: Vec<(CsGenericTemplateType, String)>,
}

#[derive(Debug, Eq, Hash, PartialEq, Clone, Default, PartialOrd, Ord)]
pub enum CsGenericTemplateType {
    #[default]
    Any,
    Reference,
}

impl CsGenericTemplate {
    pub fn make_typenames(names: impl Iterator<Item = String>) -> Self {
        CsGenericTemplate {
            names: names
                .into_iter()
                .map(|s| (CsGenericTemplateType::Any, s))
                .collect(),
        }
    }
    pub fn make_ref_types(names: impl Iterator<Item = String>) -> Self {
        CsGenericTemplate {
            names: names
                .into_iter()
                .map(|s| (CsGenericTemplateType::Reference, s))
                .collect(),
        }
    }

    pub fn just_names(&self) -> impl Iterator<Item = &String> {
        self.names.iter().map(|(_constraint, t)| t)
    }
}

#[derive(Debug, Clone, Eq, Hash, PartialEq, PartialOrd)]
pub struct CsCommentedString {
    pub data: String,
    pub comment: Option<String>,
}

#[derive(Debug, Hash, PartialEq, Eq, PartialOrd, Ord, Clone)]
pub struct CsUsingAlias {
    pub result: String,
    pub alias: String,
    pub template: Option<CsGenericTemplate>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum CsMember {}

#[derive(Clone, Debug, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct CsMethodData {
    pub estimated_size: usize,
    pub addrs: u64,
    pub slot: Option<u16>,
}

#[derive(Clone, Debug)]
pub struct CsMethodSizeData {
    pub cpp_method_name: String,
    pub method_name: String,
    pub declaring_type_name: String,
    pub declaring_classof_call: String,
    pub ret_ty: String,
    pub instance: bool,
    pub params: Vec<CsParam>,
    pub method_data: CsMethodData,

    // this is so bad
    pub method_info_lines: Vec<String>,
    pub method_info_var: String,

    pub template: Option<CsGenericTemplate>,
    pub generic_literals: Option<Vec<String>>,

    pub interface_clazz_of: String,
    pub is_final: bool,
    pub slot: Option<u16>,
}

#[derive(Clone, Debug, PartialEq, PartialOrd)]
pub enum CsValue {
    String(String),
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

    Object(Bytes),
    ValueType(Bytes),
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
    pub getter: Option<String>,
    pub setter: Option<String>,
    /// Whether this property is one that's indexable (accessor methods take an index argument)
    pub indexable: bool,
    pub brief_comment: Option<String>,
}

bitflags! {
    #[derive(Debug, Clone, Hash, PartialEq, PartialOrd, Eq, Ord)]
    pub struct CsParamFlags: u8 {
        const A = 1;
        const B = 1 << 1;
        const C = 0b0000_0100;
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
        const EXTERN = 0b01000000;
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
    pub template: Option<CsGenericTemplate>,
    pub method_data: Option<CsMethodData>,
    pub brief: Option<String>,
    pub method_flags: CSMethodFlags,
}

// TODO: Generics
#[derive(Clone, Debug)]
pub struct CsConstructor {
    pub cpp_name: String,
    pub parameters: Vec<CsParam>,
    pub template: Option<CsGenericTemplate>,

    pub brief: Option<String>,
    pub body: Option<Vec<Arc<dyn CppWritable>>>,
}

impl PartialEq for CsConstructor {
    fn eq(&self, other: &Self) -> bool {
        self.cpp_name == other.cpp_name
            && self.parameters == other.parameters
            && self.template == other.template
            && self.brief == other.brief
            // can't guarantee equality
            && self.body.is_some() == other.body.is_some()
    }
}
