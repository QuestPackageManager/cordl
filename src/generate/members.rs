use bitflags::bitflags;
use brocolib::runtime_metadata::Il2CppType;
use bytes::Bytes;
use itertools::Itertools;
use pathdiff::diff_paths;

use crate::STATIC_CONFIG;

use super::{context::TypeContext, cs_type::CsType, writer::CppWritable};

use std::{
    collections::HashMap,
    hash::Hash,
    path::{Path, PathBuf},
    rc::Rc,
    sync::Arc,
};

#[derive(Debug, Eq, Hash, PartialEq, Clone, Default, PartialOrd, Ord)]
pub struct GenericTemplate {
    pub names: Vec<(GenericTemplateType, String)>,
}

#[derive(Debug, Eq, Hash, PartialEq, Clone, Default, PartialOrd, Ord)]
pub enum GenericTemplateType {
    #[default]
    Any,
    Reference,
}

impl GenericTemplate {
    pub fn make_typenames(names: impl Iterator<Item = String>) -> Self {
        GenericTemplate {
            names: names
                .into_iter()
                .map(|s| (GenericTemplateType::Any, s))
                .collect(),
        }
    }
    pub fn make_ref_types(names: impl Iterator<Item = String>) -> Self {
        GenericTemplate {
            names: names
                .into_iter()
                .map(|s| (GenericTemplateType::Reference, s))
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
pub struct CppInclude {
    pub include: PathBuf,
    pub system: bool,
}

#[derive(Debug, Hash, PartialEq, Eq, PartialOrd, Ord, Clone)]
pub struct CppUsingAlias {
    pub result: String,
    pub alias: String,
    pub template: Option<GenericTemplate>,
}

#[derive(Clone, Debug, PartialEq, PartialOrd)]
pub enum CsMember {
    FieldDecl(CsField),
    MethodDecl(CsMethodDecl),
    Property(CsPropertyDecl),
    ConstructorDecl(CsConstructor),
    NestedUnion(CsNestedUnion),
    CppUsingAlias(CppUsingAlias),
    Comment(CsCommentedString),
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

    pub template: Option<GenericTemplate>,
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
    pub field_ty: Il2CppType,
    pub instance: bool,
    pub readonly: bool,
    pub const_expr: bool,

    pub offset: Option<u32>,
    pub value: Option<CsValue>,
    pub brief_comment: Option<String>,
    pub is_private: bool,
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
    pub il2cpp_ty: Il2CppType,
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
    pub return_type: Il2CppType,
    pub parameters: Vec<CsParam>,
    pub instance: bool,
    pub template: Option<GenericTemplate>,
    pub method_data: Option<CsMethodData>,
    pub brief: Option<String>,
}

// TODO: Generics
#[derive(Clone, Debug)]
pub struct CsConstructor {
    pub cpp_name: String,
    pub parameters: Vec<CsParam>,
    pub template: Option<GenericTemplate>,

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
    pub is_private: bool,
}

impl CsParam {
    pub fn params_as_args(params: &[CsParam]) -> impl Iterator<Item = String> + '_ {
        params.iter().map(|p| match &p.def_value {
            Some(val) => format!("{}{} {} = {val}", p.ty, p.modifiers, p.name),
            None => format!("{} {} {}", p.ty, p.modifiers, p.name),
        })
    }
    pub fn params_as_args_no_default(params: &[CsParam]) -> impl Iterator<Item = String> + '_ {
        params
            .iter()
            .map(|p| format!("{} {} {}", p.ty, p.modifiers, p.name))
    }
    pub fn params_names(params: &[CsParam]) -> impl Iterator<Item = &String> {
        params.iter().map(|p| &p.name)
    }
    pub fn params_types(params: &[CsParam]) -> impl Iterator<Item = &String> {
        params.iter().map(|p| &p.ty)
    }

    pub fn params_il2cpp_types(params: &[CsParam]) -> impl Iterator<Item = String> + '_ {
        params
            .iter()
            .map(|p| format!("::il2cpp_utils::ExtractType({})", p.name))
    }
}

impl CppInclude {
    // smelly use of config but whatever
    pub fn new_context_typedef(context: &TypeContext) -> Self {
        Self {
            include: diff_paths(&context.typedef_path, &STATIC_CONFIG.header_path).unwrap(),
            system: false,
        }
    }
    pub fn new_context_typeimpl(context: &TypeContext) -> Self {
        Self {
            include: diff_paths(&context.type_impl_path, &STATIC_CONFIG.header_path).unwrap(),
            system: false,
        }
    }
    pub fn new_context_fundamental(context: &TypeContext) -> Self {
        Self {
            include: diff_paths(&context.fundamental_path, &STATIC_CONFIG.header_path).unwrap(),
            system: false,
        }
    }

    pub fn new_system<P: AsRef<Path>>(str: P) -> Self {
        Self {
            include: str.as_ref().to_path_buf(),
            system: true,
        }
    }

    pub fn new_exact<P: AsRef<Path>>(str: P) -> Self {
        Self {
            include: str.as_ref().to_path_buf(),
            system: false,
        }
    }
}

impl CppUsingAlias {
    // TODO: Rewrite
    pub fn from_cpp_type(
        alias: String,
        cpp_type: &CsType,
        forwarded_generic_args_opt: Option<Vec<String>>,
        fixup_generic_args: bool,
    ) -> Self {
        let forwarded_generic_args = forwarded_generic_args_opt.unwrap_or_default();

        // splits literals and template
        let (literal_args, template) = match &cpp_type.generic_template {
            Some(other_template) => {
                // Skip the first args as those aren't necessary
                let extra_template_args = other_template
                    .names
                    .iter()
                    .skip(forwarded_generic_args.len())
                    .cloned()
                    .collect_vec();

                let remaining_cpp_template = match !extra_template_args.is_empty() {
                    true => Some(GenericTemplate {
                        names: extra_template_args,
                    }),
                    false => None,
                };

                // Essentially, all nested types inherit their declaring type's generic params.
                // Append the rest of the template params as generic parameters
                match remaining_cpp_template {
                    Some(remaining_cpp_template) => (
                        forwarded_generic_args
                            .iter()
                            .chain(remaining_cpp_template.just_names())
                            .cloned()
                            .collect_vec(),
                        Some(remaining_cpp_template),
                    ),
                    None => (forwarded_generic_args, None),
                }
            }
            None => (forwarded_generic_args, None),
        };

        let do_fixup = fixup_generic_args && !literal_args.is_empty();

        let mut name_components = cpp_type.cpp_name_components.clone();
        if do_fixup {
            name_components = name_components.remove_generics();
        }

        let mut result = name_components.remove_pointer().combine_all();

        // easy way to tell it's a generic instantiation
        if do_fixup {
            result = format!("{result}<{}>", literal_args.join(", "))
        }

        Self {
            alias,
            result,
            template,
        }
    }
}
