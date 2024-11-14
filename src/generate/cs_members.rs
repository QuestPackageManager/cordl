use bitflags::bitflags;
use bytes::Bytes;
use itertools::Itertools;
use pathdiff::diff_paths;

use crate::STATIC_CONFIG;

use super::{
    context::TypeContext, cs_fields::FieldInfo, cs_type::CsType, cs_type_tag::CsTypeTag,
    writer::CppWritable,
};

use std::{
    collections::HashMap,
    hash::Hash,
    path::{Path, PathBuf},
    rc::Rc,
    sync::Arc,
};

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
pub struct CppUsingAlias {
    pub result: String,
    pub alias: String,
    pub template: Option<CsGenericTemplate>,
}

#[derive(Clone, Debug, PartialEq, PartialOrd)]
pub enum CsMember {
    FieldDecl(CsField),
    MethodDecl(CsMethodDecl),
    Property(CsPropertyDecl),
    ConstructorDecl(CsConstructor),
    NestedUnion(CsNestedUnion),
    NestedStruct(CsNestedStruct),
    CppUsingAlias(CppUsingAlias),
    Comment(CsCommentedString),
    FieldLayout(CsFieldLayout),
}

#[derive(Clone, Debug, PartialEq, PartialOrd)]
pub struct CsNestedStruct {
    pub name: String,
    pub declarations: Vec<Rc<CsMember>>,
    pub is_enum: bool,
    pub is_class: bool,
    pub brief_comment: Option<String>,
    pub packing: Option<u8>,
}

#[derive(Clone, Debug, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct CsMethodData {
    pub estimated_size: usize,
    pub addrs: u64,
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

#[derive(Clone, Debug, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub enum CsValue {
    String(String),
    Num(usize),
    FloatingNum(f64),
    Object(Bytes),
    ValueType(Bytes),
    Null,
}

#[derive(Clone, Debug, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct CsField {
    pub name: String,
    pub field_ty: CsTypeTag,
    pub instance: bool,
    pub readonly: bool,
    // is C# const
    // could be assumed from value though
    pub const_expr: bool,

    pub offset: Option<u32>,
    pub value: Option<CsValue>,
    pub brief_comment: Option<String>,
}

#[derive(Clone, Debug, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct CsPropertyDecl {
    pub cpp_name: String,
    pub prop_ty: String,
    pub instance: bool,
    pub getter: Option<String>,
    pub setter: Option<String>,
    /// Whether this property is one that's indexable (accessor methods take an index argument)
    pub indexable: bool,
    pub brief_comment: Option<String>,
}

bitflags! {
    struct CsParamFlags: u8 {
        const A = 1;
        const B = 1 << 1;
        const C = 0b0000_0100;
    }
}

#[derive(Clone, Debug, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct CsParam {
    pub name: String,
    pub il2cpp_ty: CsTypeTag,
    // TODO: Use bitflags to indicate these attributes
    // May hold:
    // const
    // May hold one of:
    // *
    // &
    // &&
    pub modifiers: CsParamFlags,
    pub def_value: Option<String>,
}

bitflags! {
    pub struct MethodModifiers: u32 {
        const STATIC = 0b00000001;
        const VIRTUAL = 0b00000010;
        const OPERATOR = 0b00000100;
    }
}

// TODO: Generics
#[derive(Clone, Debug, PartialEq, PartialOrd)]
pub struct CsMethodDecl {
    pub name: String,
    pub return_type: CsTypeTag,
    pub parameters: Vec<CsParam>,
    pub instance: bool,
    pub template: Option<CsGenericTemplate>,
    pub method_data: Option<CsMethodData>,
    pub brief: Option<String>,
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
            && self.is_constexpr == other.is_constexpr
            && self.is_explicit == other.is_explicit
            && self.is_default == other.is_default
            && self.is_no_except == other.is_no_except
            && self.is_delete == other.is_delete
            && self.is_protected == other.is_protected
            && self.base_ctor == other.base_ctor
            && self.initialized_values == other.initialized_values
            && self.brief == other.brief
            // can't guarantee equality
            && self.body.is_some() == other.body.is_some()
    }
}

impl PartialOrd for CsConstructor {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        match self.cpp_name.partial_cmp(&other.cpp_name) {
            Some(core::cmp::Ordering::Equal) => {}
            ord => return ord,
        }
        match self.parameters.partial_cmp(&other.parameters) {
            Some(core::cmp::Ordering::Equal) => {}
            ord => return ord,
        }
        self.template.partial_cmp(&other.template)
    }
}

#[derive(Clone, Debug, PartialEq, PartialOrd)]
pub struct CsNestedUnion {
    pub declarations: Vec<Rc<CsMember>>,
    pub brief_comment: Option<String>,
    pub offset: u32,
}

#[derive(Clone, Debug, PartialEq, PartialOrd)]
pub struct CsFieldLayout {
    pub field: CsField,
    // make struct with size [padding, field] packed with 1
    pub padding: u32,
    // make struct with size [alignment, field_size] default packed
    pub alignment: usize,
}
