use itertools::Itertools;
use pathdiff::diff_paths;

use crate::{
    data::type_resolver::{ResolvedType, ResolvedTypeData, TypeUsage},
    generate::{
        cs_members::{CsGenericConstraint, CsGenericContainer},
        cs_type_tag::CsTypeTag,
        type_extensions::TypeDefinitionIndexExtensions,
        writer::Writable,
    },
};

use std::{
    collections::HashMap,
    fmt::Debug,
    hash::Hash,
    path::{Path, PathBuf},
    rc::Rc,
    sync::Arc,
};

use super::{
    config::STATIC_CONFIG,
    cpp_context::CppContext,
    cpp_name_resolver::CppNameResolver,
    cpp_type::{
        CORDL_DEFAULT_CTOR_CONSTRAINT, CORDL_REFERENCE_TYPE_CONSTRAINT, CORDL_TYPE_CONSTRAINT,
        CORDL_VALUE_TYPE_CONSTRAINT, CppType,
    },
};

/// A single template parameter, e.g. `typename T`, plus the C# constraints declared on it
#[derive(Debug, Eq, Hash, PartialEq, Clone, Default, PartialOrd, Ord)]
pub struct CppTemplateParam {
    /// The parameter kind, practically always `typename`
    pub kind: String,
    pub name: String,
    /// Boolean expressions that must hold for [`Self::name`], written out as a `requires` clause.
    /// Empty for an unconstrained parameter.
    pub constraints: Vec<String>,
}

impl CppTemplateParam {
    pub fn typename(name: String) -> Self {
        CppTemplateParam {
            kind: "typename".to_string(),
            name,
            constraints: vec![],
        }
    }
}

#[derive(Debug, Eq, Hash, PartialEq, Clone, Default, PartialOrd, Ord)]
pub struct CppTemplate {
    pub names: Vec<CppTemplateParam>,
}

impl CppTemplate {
    pub fn just_names(&self) -> impl Iterator<Item = &String> {
        self.names.iter().map(|p| &p.name)
    }

    /// Drops the first `skip` parameters, as well as any constraint left behind that
    /// mentioned one of them. Whatever the caller substitutes those parameters with does
    /// not necessarily name a type the remaining `requires` clause may still refer to.
    pub fn skip_params(&self, skip: usize) -> CppTemplate {
        let dropped = self
            .names
            .iter()
            .take(skip)
            .map(|p| p.name.as_str())
            .collect_vec();

        CppTemplate {
            names: self
                .names
                .iter()
                .skip(skip)
                .map(|p| CppTemplateParam {
                    constraints: p
                        .constraints
                        .iter()
                        .filter(|c| !dropped.iter().any(|d| mentions_identifier(c, d)))
                        .cloned()
                        .collect(),
                    ..p.clone()
                })
                .collect(),
        }
    }

    /// The `requires` clause covering every constrained parameter, if there is one.
    /// Constraints of parameters that aren't part of this template are not included,
    /// so slicing [`Self::names`] keeps the clause well formed.
    pub fn requires_clause(&self) -> Option<String> {
        let constraints = self
            .names
            .iter()
            .flat_map(|p| p.constraints.iter())
            .join(" && ");

        (!constraints.is_empty()).then_some(constraints)
    }
}

impl CppTemplate {
    /// Makes a template whose parameters carry the C# `where` clauses of `container`.
    ///
    /// Constraining types are hard included, since a `requires` clause is checked at
    /// instantiation time and needs them complete by then.
    pub fn make_constrained(
        container: &CsGenericContainer,
        declaring_cpp_type: &mut CppType,
        name_resolver: &CppNameResolver,
    ) -> Self {
        let names = container
            .args
            .iter()
            .map(|arg| {
                let name = &arg.name;

                let constraints = arg
                    .constraints
                    .iter()
                    .filter_map(|constraint| match constraint {
                        CsGenericConstraint::Class => {
                            Some(format!("{CORDL_REFERENCE_TYPE_CONSTRAINT}<{name}>"))
                        }
                        CsGenericConstraint::Struct => {
                            Some(format!("{CORDL_VALUE_TYPE_CONSTRAINT}<{name}>"))
                        }
                        CsGenericConstraint::DefaultConstructor => {
                            Some(format!("{CORDL_DEFAULT_CTOR_CONSTRAINT}<{name}>"))
                        }
                        // variance says how the type may be substituted, it constrains nothing
                        CsGenericConstraint::Covariant | CsGenericConstraint::Contravariant => None,
                        CsGenericConstraint::Resolved(resolved) => {
                            let ty = resolve_constraint_name(
                                resolved,
                                declaring_cpp_type,
                                name_resolver,
                            )?;

                            Some(format!("{CORDL_TYPE_CONSTRAINT}<{name}, {ty}>"))
                        }
                    })
                    .collect();

                CppTemplateParam {
                    kind: "typename".to_string(),
                    name: name.clone(),
                    constraints,
                }
            })
            .collect();

        CppTemplate { names }
    }
}

/// Resolves the C++ name a `where T : X` constraint should check against,
/// or [`None`] if the constraint cannot be expressed in C++.
fn resolve_constraint_name(
    constraint: &ResolvedType,
    declaring_cpp_type: &mut CppType,
    name_resolver: &CppNameResolver,
) -> Option<String> {
    let metadata = name_resolver.cordl_metadata;

    // `where T : class`/`struct`/`Enum` also carry a constraint on the base type they imply.
    // The other variants of CsGenericConstraint already cover those, and those base types
    // aren't C++ base classes anyway.
    if constraint_tags(constraint).any(|tag| {
        let td = tag.get_tdi().get_type_definition(metadata.metadata);

        td.namespace(metadata.metadata) == "System"
            && matches!(td.name(metadata.metadata), "Object" | "ValueType" | "Enum")
    }) {
        return None;
    }

    // A constraint naming the type it is declared on, e.g. `class Foo<T> where T : Foo<T>`,
    // would need Foo<T> complete to check whether Foo<T> may be instantiated
    if constraint_tags(constraint).any(|tag| tag == declaring_cpp_type.self_tag) {
        return None;
    }

    if matches!(constraint.data, ResolvedTypeData::Blacklisted(_)) {
        return None;
    }

    let name = name_resolver.resolve_name(
        declaring_cpp_type,
        constraint,
        TypeUsage::GenericConstraint,
        true,
    );

    Some(name.combine_all())
}

/// Whether `haystack` uses `ident` as a whole C++ identifier rather than as part of a longer one
fn mentions_identifier(haystack: &str, ident: &str) -> bool {
    let is_ident_char = |c: char| c.is_alphanumeric() || c == '_';

    haystack.match_indices(ident).any(|(start, _)| {
        let before = haystack[..start].chars().next_back();
        let after = haystack[start + ident.len()..].chars().next();

        !before.is_some_and(is_ident_char) && !after.is_some_and(is_ident_char)
    })
}

/// Every type tag a resolved type refers to, including its generic arguments
fn constraint_tags(ty: &ResolvedType) -> Box<dyn Iterator<Item = CsTypeTag> + '_> {
    match &ty.data {
        ResolvedTypeData::Array(inner)
        | ResolvedTypeData::Ptr(inner)
        | ResolvedTypeData::ByRef(inner)
        | ResolvedTypeData::ByRefConst(inner) => constraint_tags(inner),
        ResolvedTypeData::GenericInst(inner, args) => Box::new(
            constraint_tags(inner).chain(args.iter().flat_map(|(arg, _)| constraint_tags(arg))),
        ),
        ResolvedTypeData::Type(tag) | ResolvedTypeData::Blacklisted(tag) => {
            Box::new(std::iter::once(*tag))
        }
        ResolvedTypeData::Primitive(_)
        | ResolvedTypeData::GenericArg(_, _)
        | ResolvedTypeData::GenericMethodArg(_, _, _) => Box::new(std::iter::empty()),
    }
}

impl From<CsGenericContainer> for CppTemplate {
    /// Makes an unconstrained template. Constraints need the resolved C++ names of the
    /// constraining types, so they are filled in later by [`CppTemplate::make_constrained`].
    fn from(value: CsGenericContainer) -> Self {
        CppTemplate {
            names: value
                .args
                .into_iter()
                .map(|arg| CppTemplateParam::typename(arg.name))
                .collect(),
        }
    }
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

pub trait WritableDebug: Writable + Debug {}
impl<T: Writable + Debug> WritableDebug for T {}

// TODO: Make this group lots into a single namespace
// #[derive(Debug, Eq, Hash, PartialEq, Clone)]
// pub struct CppForwardDeclareGroup {
//     pub namespace: Option<String>,
//     pub items: Vec<CppForwardDeclare>,
//     pub group_items: Vec<CppForwardDeclareGroup>,
// }

#[derive(Debug, Eq, Hash, PartialEq, Clone)]
pub struct CppForwardDeclare {
    pub is_struct: bool,
    pub cpp_namespace: Option<String>,
    pub cpp_name: String,
    pub templates: Option<CppTemplate>, // names of template arguments, T, TArgs etc.
    pub literals: Option<Vec<String>>,
}

#[derive(Debug, Clone, Eq, Hash, PartialEq, PartialOrd)]
pub struct CppCommentedString {
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
    pub template: Option<CppTemplate>,
}

#[derive(Clone, Debug, PartialEq, PartialOrd)]
pub enum CppMember {
    FieldDecl(CppFieldDecl),
    FieldImpl(CppFieldImpl),
    MethodDecl(CppMethodDecl),
    MethodImpl(CppMethodImpl),
    Property(CppPropertyDecl),
    ConstructorDecl(CppConstructorDecl),
    ConstructorImpl(CppConstructorImpl),
    NestedStruct(CppNestedStruct),
    NestedUnion(CppNestedUnion),
    CppUsingAlias(CppUsingAlias),
    Comment(CppCommentedString),
    CppStaticAssert(CppStaticAssert),
    CppLine(CppLine),
}

#[derive(Clone, Debug)]
pub enum CppNonMember {
    SizeStruct(Box<CppMethodSizeStruct>),
    CppUsingAlias(CppUsingAlias),
    Comment(CppCommentedString),
    CppStaticAssert(CppStaticAssert),
    CppLine(CppLine),
}

#[derive(Clone, Debug, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct CppMethodData {
    pub estimated_size: usize,
    pub addrs: u64,
}

#[derive(Clone, Debug)]
pub struct CppMethodSizeStruct {
    pub cpp_method_name: String,
    pub declaring_type_name: String,
    pub declaring_classof_call: String,
    pub ret_ty: String,
    pub instance: bool,
    pub params: Vec<CppParam>,
    pub method_data: CppMethodData,

    // this is so bad
    pub method_info_lines: Vec<String>,
    pub method_info_var: String,

    pub declaring_template: Option<CppTemplate>,
    pub template: Option<CppTemplate>,

    pub interface_clazz_of: String,
    pub is_final: bool,
    pub slot: Option<u16>,
}
#[derive(Clone, Debug, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct CppFieldDecl {
    pub cpp_name: String,
    pub field_ty: String,
    pub offset: Option<u32>,
    pub instance: bool,
    pub readonly: bool,
    pub const_expr: bool,
    pub value: Option<String>,
    pub brief_comment: Option<String>,
    pub is_private: bool,
}
#[derive(Clone, Debug, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct CppFieldImpl {
    pub declaring_type: String,
    pub declaring_type_template: Option<CppTemplate>,
    pub cpp_name: String,
    pub field_ty: String,
    pub readonly: bool,
    pub const_expr: bool,
    pub value: String,
}

impl From<CppFieldDecl> for CppFieldImpl {
    fn from(value: CppFieldDecl) -> Self {
        Self {
            const_expr: value.const_expr,
            readonly: value.readonly,
            cpp_name: value.cpp_name,
            field_ty: value.field_ty,
            declaring_type: "".to_string(),
            declaring_type_template: Default::default(),
            value: value.value.unwrap_or_default(),
        }
    }
}

#[derive(Clone, Debug, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct CppPropertyDecl {
    pub cpp_name: String,
    pub prop_ty: String,
    pub instance: bool,
    pub getter: Option<String>,
    pub setter: Option<String>,
    /// Whether this property is one that's indexable (accessor methods take an index argument)
    pub indexable: bool,
    pub brief_comment: Option<String>,
}

#[derive(Clone, Debug, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct CppParam {
    pub name: String,
    pub ty: String,
    // TODO: Use bitflags to indicate these attributes
    // May hold:
    // const
    // May hold one of:
    // *
    // &
    // &&
    pub modifiers: String,
    pub def_value: Option<String>,
    /// e.g. `[Optional]` - rendered as a leading `/* ... */` comment since a parameter has no
    /// room for a line comment of its own within a signature.
    pub comment: Option<String>,
}

// TODO: Generics
#[derive(Clone, Debug)]
pub struct CppMethodDecl {
    pub cpp_name: String,
    pub return_type: String,
    pub parameters: Vec<CppParam>,
    pub instance: bool,
    pub template: Option<CppTemplate>,
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
    pub is_virtual: bool,
    pub is_constexpr: bool,
    pub is_const: bool,
    pub is_no_except: bool,
    pub is_implicit_operator: bool,
    pub is_explicit_operator: bool,
    pub is_inline: bool,

    pub brief: Option<String>,
    pub body: Option<Vec<Arc<dyn WritableDebug>>>,
}

impl PartialEq for CppMethodDecl {
    fn eq(&self, other: &Self) -> bool {
        self.cpp_name == other.cpp_name
            && self.return_type == other.return_type
            && self.parameters == other.parameters
            && self.instance == other.instance
            && self.template == other.template
            && self.suffix_modifiers == other.suffix_modifiers
            && self.prefix_modifiers == other.prefix_modifiers
            && self.is_virtual == other.is_virtual
            && self.is_constexpr == other.is_constexpr
            && self.is_const == other.is_const
            && self.is_no_except == other.is_no_except
            && self.is_implicit_operator == other.is_implicit_operator
            && self.is_inline == other.is_inline
            && self.brief == other.brief
            // can't gurantee body is equal
            && self.body.is_some() == other.body.is_some()
    }
}

impl PartialOrd for CppMethodDecl {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        match self.cpp_name.partial_cmp(&other.cpp_name) {
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
        self.template.partial_cmp(&other.template)
    }
}

impl From<CppMethodDecl> for CppMethodImpl {
    fn from(value: CppMethodDecl) -> Self {
        Self {
            body: value.body.unwrap_or_default(),
            brief: value.brief,
            cpp_method_name: value.cpp_name,
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
    pub parameters: Vec<CppParam>,
    pub instance: bool,

    pub declaring_type_template: Option<CppTemplate>,
    pub template: Option<CppTemplate>,
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
    pub body: Vec<Arc<dyn WritableDebug>>,
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

// TODO: Generics
#[derive(Clone, Debug)]
pub struct CppConstructorDecl {
    pub cpp_name: String,
    pub parameters: Vec<CppParam>,
    pub template: Option<CppTemplate>,

    pub is_constexpr: bool,
    pub is_explicit: bool,
    pub is_default: bool,
    pub is_no_except: bool,
    pub is_delete: bool,
    pub is_protected: bool,

    // call base ctor
    pub base_ctor: Option<(String, String)>,
    pub initialized_values: HashMap<String, String>,

    pub brief: Option<String>,
    pub body: Option<Vec<Arc<dyn WritableDebug>>>,
}

impl PartialEq for CppConstructorDecl {
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

impl PartialOrd for CppConstructorDecl {
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

#[derive(Clone, Debug)]
pub struct CppConstructorImpl {
    pub declaring_full_name: String,
    pub declaring_name: String,

    pub parameters: Vec<CppParam>,
    pub base_ctor: Option<(String, String)>,
    pub initialized_values: HashMap<String, String>,

    pub is_constexpr: bool,
    pub is_no_except: bool,
    pub is_default: bool,

    pub template: Option<CppTemplate>,

    pub body: Vec<Arc<dyn WritableDebug>>,
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

#[derive(Clone, Debug, PartialEq, PartialOrd)]
pub struct CppNestedStruct {
    pub declaring_name: String,
    pub base_type: Option<String>,
    pub declarations: Vec<Rc<CppMember>>,
    pub is_enum: bool,
    pub is_class: bool,
    pub is_private: bool,
    pub brief_comment: Option<String>,
    pub packing: Option<u8>,
}

#[derive(Clone, Debug, PartialEq, PartialOrd)]
pub struct CppNestedUnion {
    pub declarations: Vec<Rc<CppMember>>,
    pub brief_comment: Option<String>,
    pub offset: u32,
    pub is_private: bool,
}

impl From<CppConstructorDecl> for CppConstructorImpl {
    fn from(value: CppConstructorDecl) -> Self {
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
    pub fn from_cpp_type(cpp_type: &CppType) -> Self {
        Self::from_cpp_type_long(cpp_type, false)
    }
    pub fn from_cpp_type_long(cpp_type: &CppType, force_generics: bool) -> Self {
        let ns = cpp_type.cpp_namespace().to_string();

        // TODO: Proper nested check
        // assert!(
        //     cpp_type.is_nested,
        //     "Can't forward declare nested types! {:#?}",
        //     cpp_type.cpp_name_components
        // );

        // literals should only be added for generic specializations
        let literals = if force_generics {
            cpp_type.cpp_name_components.generics.clone()
        } else {
            None
        };

        Self {
            is_struct: cpp_type.is_value_type,
            cpp_namespace: Some(ns),
            cpp_name: cpp_type.cpp_name().clone(),
            templates: cpp_type.cpp_template.clone(),
            literals,
        }
    }
}

impl CppParam {
    fn attribute_comment_prefix(p: &CppParam) -> String {
        match &p.comment {
            Some(comment) => format!("/* {comment} */ "),
            None => String::new(),
        }
    }

    pub fn params_as_args(params: &[CppParam]) -> impl Iterator<Item = String> + '_ {
        params.iter().map(|p| {
            let prefix = Self::attribute_comment_prefix(p);
            match &p.def_value {
                Some(val) => format!("{prefix}{} {} {} = {val}", p.ty, p.modifiers, p.name),
                None => format!("{prefix}{} {} {}", p.ty, p.modifiers, p.name),
            }
        })
    }
    pub fn params_as_args_no_default(params: &[CppParam]) -> impl Iterator<Item = String> + '_ {
        params.iter().map(|p| {
            let prefix = Self::attribute_comment_prefix(p);
            format!("{prefix}{} {} {}", p.ty, p.modifiers, p.name)
        })
    }
    pub fn params_names(params: &[CppParam]) -> impl Iterator<Item = &String> {
        params.iter().map(|p| &p.name)
    }
    pub fn params_types(params: &[CppParam]) -> impl Iterator<Item = &String> {
        params.iter().map(|p| &p.ty)
    }
}

impl CppInclude {
    // smelly use of config but whatever
    pub fn new_context_typedef(context: &CppContext) -> Self {
        Self {
            include: diff_paths(&context.typedef_path, &STATIC_CONFIG.header_path).unwrap(),
            system: false,
        }
    }
    pub fn new_context_typeimpl(context: &CppContext) -> Self {
        Self {
            include: diff_paths(&context.type_impl_path, &STATIC_CONFIG.header_path).unwrap(),
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
        cpp_type: &CppType,
        forwarded_generic_args_opt: Option<Vec<String>>,
        fixup_generic_args: bool,
    ) -> Self {
        let forwarded_generic_args = forwarded_generic_args_opt.unwrap_or_default();

        // splits literals and template
        let (literal_args, template) = match &cpp_type.cpp_template {
            Some(other_template) => {
                // Skip the first args as those aren't necessary
                let extra_template_args = other_template.skip_params(forwarded_generic_args.len());

                let remaining_cpp_template =
                    (!extra_template_args.names.is_empty()).then_some(extra_template_args);

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
