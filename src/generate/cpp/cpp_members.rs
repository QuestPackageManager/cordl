use std::{collections::HashMap, rc::Rc, sync::Arc};

use crate::generate::{cs_type::CsType, members::{
    CppUsingAlias, CsCommentedString, CsConstructor, CsField, CsMember, CsMethodDecl, CsMethodSizeData, CsParam, GenericTemplate
}, writer::CppWritable};

#[derive(Clone, Debug)]
pub enum CppNonMember {
    SizeStruct(Box<CsMethodSizeData>),
    CppUsingAlias(CppUsingAlias),
    Comment(CsCommentedString),
    CppStaticAssert(CppStaticAssert),
    CppLine(CppLine),
}

#[derive(Debug, Eq, Hash, PartialEq, Clone, Default, PartialOrd, Ord)]
pub struct CppStaticAssert {
    pub condition: String,
    pub message: Option<String>,
}

#[derive(Debug, Eq, Hash, PartialEq, Clone, Default, PartialOrd, Ord)]
pub struct CppLine {
    pub line: String,
}

#[derive(Clone, Debug, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct CppFieldImpl {
    pub declaring_type: String,
    pub declaring_type_template: Option<GenericTemplate>,
    pub cpp_name: String,
    pub field_ty: String,
    pub readonly: bool,
    pub const_expr: bool,
    pub value: String,
}

impl From<CsField> for CppFieldImpl {
    fn from(value: CsField) -> Self {
        Self {
            const_expr: value.const_expr,
            readonly: value.readonly,
            cpp_name: value.name,
            field_ty: value.field_ty,
            declaring_type: "".to_string(),
            declaring_type_template: Default::default(),
            value: value.value.unwrap_or_default(),
        }
    }
}

impl From<CsMethodDecl> for CppMethodImpl {
    fn from(value: CsMethodDecl) -> Self {
        Self {
            body: value.body.unwrap_or_default(),
            brief: value.brief,
            cpp_method_name: value.name,
            declaring_cpp_full_name: "".into(),
            instance: value.instance,
            is_const: value.is_const,
            is_no_except: value.is_no_except,
            is_operator: value.is_implicit_operator,
            is_virtual: value.is_virtual,
            is_constexpr: value.is_constexpr,
            is_inline: value.is_inline,
            parameters: value.parameters,
            prefix_modifiers: value.prefix_modifiers,
            suffix_modifiers: value.suffix_modifiers,
            return_type: value.return_type,
            template: value.template,
            declaring_type_template: Default::default(),
        }
    }
}

// TODO: Generic
#[derive(Clone, Debug)]
pub struct CppMethodImpl {
    pub cpp_method_name: String,
    pub declaring_cpp_full_name: String,

    pub return_type: String,
    pub parameters: Vec<CsParam>,
    pub instance: bool,

    pub declaring_type_template: Option<GenericTemplate>,
    pub template: Option<GenericTemplate>,
    pub is_const: bool,
    pub is_virtual: bool,
    pub is_constexpr: bool,
    pub is_no_except: bool,
    pub is_operator: bool,
    pub is_inline: bool,

    // TODO: Use bitflags to indicate these attributes
    // Holds unique of:
    // const
    // override
    // noexcept
    pub suffix_modifiers: Vec<String>,
    // Holds unique of:
    // constexpr
    // static
    // inline
    // explicit(...)
    // virtual
    pub prefix_modifiers: Vec<String>,

    pub brief: Option<String>,
    pub body: Vec<Rc<dyn CppWritable>>,
}

impl PartialEq for CppMethodImpl {
    fn eq(&self, other: &Self) -> bool {
        self.cpp_method_name == other.cpp_method_name
            && self.declaring_cpp_full_name == other.declaring_cpp_full_name
            && self.return_type == other.return_type
            && self.parameters == other.parameters
            && self.instance == other.instance
            && self.declaring_type_template == other.declaring_type_template
            && self.template == other.template
            && self.is_const == other.is_const
            && self.is_virtual == other.is_virtual
            && self.is_constexpr == other.is_constexpr
            && self.is_no_except == other.is_no_except
            && self.is_operator == other.is_operator
            && self.is_inline == other.is_inline
            && self.suffix_modifiers == other.suffix_modifiers
            && self.prefix_modifiers == other.prefix_modifiers
            && self.brief == other.brief
        // can't guarantee body is the same
        // && self.body == other.body
    }
}

impl PartialOrd for CppMethodImpl {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        match self.cpp_method_name.partial_cmp(&other.cpp_method_name) {
            Some(core::cmp::Ordering::Equal) => {}
            ord => return ord,
        }
        match self
            .declaring_cpp_full_name
            .partial_cmp(&other.declaring_cpp_full_name)
        {
            Some(core::cmp::Ordering::Equal) => {}
            ord => return ord,
        }
        match self.return_type.partial_cmp(&other.return_type) {
            Some(core::cmp::Ordering::Equal) => {}
            ord => return ord,
        }
        match self.parameters.partial_cmp(&other.parameters) {
            Some(core::cmp::Ordering::Equal) => {}
            ord => return ord,
        }
        match self.instance.partial_cmp(&other.instance) {
            Some(core::cmp::Ordering::Equal) => {}
            ord => return ord,
        }
        match self
            .declaring_type_template
            .partial_cmp(&other.declaring_type_template)
        {
            Some(core::cmp::Ordering::Equal) => {}
            ord => return ord,
        }
        self.template.partial_cmp(&other.template)
    }
}

#[derive(Clone, Debug, PartialEq, PartialOrd)]
pub struct CppNestedStruct {
    pub declaring_name: String,
    pub base_type: Option<String>,
    pub declarations: Vec<Rc<CsMember>>,
    pub is_enum: bool,
    pub is_class: bool,
    pub is_private: bool,
    pub brief_comment: Option<String>,
    pub packing: Option<u8>,
}

#[derive(Clone, Debug)]
pub struct CppConstructorImpl {
    pub declaring_full_name: String,
    pub declaring_name: String,

    pub parameters: Vec<CsParam>,
    pub base_ctor: Option<(String, String)>,
    pub initialized_values: HashMap<String, String>,

    pub is_constexpr: bool,
    pub is_no_except: bool,
    pub is_default: bool,

    pub template: Option<GenericTemplate>,

    pub body: Vec<Arc<dyn CppWritable>>,
}

impl PartialEq for CppConstructorImpl {
    fn eq(&self, other: &Self) -> bool {
        self.declaring_full_name == other.declaring_full_name
            && self.declaring_name == other.declaring_name
            && self.parameters == other.parameters
            && self.base_ctor == other.base_ctor
            && self.initialized_values == other.initialized_values
            && self.is_constexpr == other.is_constexpr
            && self.is_no_except == other.is_no_except
            && self.is_default == other.is_default
            && self.template == other.template
        // can't guarantee equality
        // && self.body == other.body
    }
}

impl PartialOrd for CppConstructorImpl {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        match self
            .declaring_full_name
            .partial_cmp(&other.declaring_full_name)
        {
            Some(core::cmp::Ordering::Equal) => {}
            ord => return ord,
        }
        match self.declaring_name.partial_cmp(&other.declaring_name) {
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

impl From<String> for CppLine {
    fn from(value: String) -> Self {
        CppLine { line: value }
    }
}
impl From<&str> for CppLine {
    fn from(value: &str) -> Self {
        CppLine {
            line: value.to_string(),
        }
    }
}

impl CppLine {
    pub fn make(v: String) -> Self {
        CppLine { line: v }
    }
}



#[derive(Debug, Eq, Hash, PartialEq, Clone)]
pub struct CppForwardDeclareGroup {
    // TODO: Make this group lots into a single namespace
    pub namespace: Option<String>,
    pub items: Vec<CppForwardDeclare>,
    pub group_items: Vec<CppForwardDeclareGroup>,
}

#[derive(Debug, Eq, Hash, PartialEq, Clone)]
pub struct CppForwardDeclare {
    // TODO: Make this group lots into a single namespace
    pub is_struct: bool,
    pub cpp_namespace: Option<String>,
    pub cpp_name: String,
    pub templates: Option<GenericTemplate>, // names of template arguments, T, TArgs etc.
    pub literals: Option<Vec<String>>,
}



impl From<CsConstructor> for CppConstructorImpl {
    fn from(value: CsConstructor) -> Self {
        Self {
            body: value.body.unwrap_or_default(),
            declaring_full_name: value.cpp_name.clone(),
            declaring_name: value.cpp_name,
            is_constexpr: value.is_constexpr,
            is_default: value.is_default,
            base_ctor: value.base_ctor,
            initialized_values: value.initialized_values,
            is_no_except: value.is_no_except,
            parameters: value.parameters,
            template: value.template,
        }
    }
}

impl CppForwardDeclare {
    pub fn from_cpp_type(cpp_type: &CsType) -> Self {
        Self::from_cpp_type_long(cpp_type, false)
    }
    pub fn from_cpp_type_long(cpp_type: &CsType, force_generics: bool) -> Self {
        let ns = if !cpp_type.nested {
            Some(cpp_type.cpp_namespace().to_string())
        } else {
            None
        };

        assert!(
            cpp_type.cpp_name_components.declaring_types.is_none(),
            "Can't forward declare nested types!"
        );

        // literals should only be added for generic specializations
        let literals = if cpp_type.generic_instantiations_args_types.is_some() || force_generics {
            cpp_type.cpp_name_components.generics.clone()
        } else {
            None
        };

        Self {
            is_struct: cpp_type.is_value_type,
            cpp_namespace: ns,
            cpp_name: cpp_type.cpp_name().clone(),
            templates: cpp_type.generic_template.clone(),
            literals,
        }
    }
}
