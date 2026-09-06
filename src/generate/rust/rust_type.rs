use std::{collections::HashSet, ops::Deref, sync::Arc};

use color_eyre::eyre::{Context, ContextCompat, Result};
use itertools::Itertools;
use proc_macro2::TokenStream;
use quote::{ToTokens, format_ident, quote};
use syn::parse_quote;

use crate::{
    data::{
        name_components::NameComponents,
        type_resolver::{ResolvedType, TypeUsage},
    },
    generate::{
        cs_members::{CSMethodFlags, CsConstructor, CsField, CsMethod, CsParam},
        cs_type::CsType,
        cs_type_tag::{self, CsTypeTag},
        metadata::CordlMetadata,
        offsets::SizeInfo,
        type_extensions::{TypeDefinitionExtensions, TypeDefinitionIndexExtensions},
        writer::Writer,
    },
};

use super::{
    config::RustGenerationConfig,
    rust_fields,
    rust_members::{
        ConstRustField, RustFeature, RustField, RustFunction, RustGeneric, RustParam,
        RustTraitImpl, Visibility,
    },
    rust_name_components::RustNameComponents,
    rust_name_resolver::RustNameResolver,
};

use std::io::Write;

const PARENT_FIELD: &str = "__cordl_parent";

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]

pub enum RustTypeRequirement {
    Implementation(CsTypeTag),
    Definition(CsTypeTag),
}

impl Deref for RustTypeRequirement {
    type Target = CsTypeTag;

    fn deref(&self) -> &Self::Target {
        match self {
            RustTypeRequirement::Implementation(tag) => tag,
            RustTypeRequirement::Definition(tag) => tag,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct RustTypeRequirements {
    required_modules: HashSet<String>,
    required_def_types: HashSet<RustTypeRequirement>,
    required_impl_types: HashSet<RustTypeRequirement>,
}

impl RustTypeRequirements {
    pub fn add_module(&mut self, module: &str) {
        self.required_modules.insert(module.to_string());
    }
    pub fn add_def_dependency(&mut self, cs_type_tag: RustTypeRequirement) {
        self.required_def_types.insert(cs_type_tag);
    }
    pub fn add_impl_dependency(&mut self, cs_type_tag: RustTypeRequirement) {
        self.required_impl_types.insert(cs_type_tag);
    }

    pub(crate) fn needs_object_include(&mut self) {
        self.add_module("quest_hook::libil2cpp::Il2CppObject");
    }

    pub(crate) fn needs_array_include(&mut self) {
        self.add_module("quest_hook::libil2cpp::Il2CppArray");
    }

    pub(crate) fn needs_string_include(&mut self) {
        self.add_module("quest_hook::libil2cpp::Il2CppString");
    }

    pub(crate) fn needs_byref_include(&mut self) {
        // todo!()
    }

    pub(crate) fn needs_byref_const_include(&mut self) {
        // todo!()
    }

    pub(crate) fn get_modules(&self) -> &HashSet<String> {
        &self.required_modules
    }
    pub fn get_def_dependencies(&self) -> &HashSet<RustTypeRequirement> {
        &self.required_def_types
    }
    pub fn get_impl_dependencies(&self) -> &HashSet<RustTypeRequirement> {
        &self.required_impl_types
    }
}

#[derive(Clone)]
pub struct RustType {
    // TODO: union
    pub fields: Vec<CustomArc<RustField>>,
    pub constants: Vec<CustomArc<ConstRustField>>,
    pub methods: Vec<CustomArc<RustFunction>>,
    pub traits: Vec<CustomArc<RustTraitImpl>>,
    pub nested_types: Vec<CustomArc<syn::ItemType>>,

    pub is_value_type: bool,
    pub is_enum_type: bool,
    pub is_reference_type: bool,
    pub is_interface: bool,

    pub self_tag: CsTypeTag,

    /// Unlocks the entire class, its dependencies and its methods.
    pub self_impl_feature: Option<CustomArc<RustFeature>>,

    /// Unlocks only the class definition and traits
    pub self_def_feature: Option<CustomArc<RustFeature>>,

    pub parent: Option<RustNameComponents>,
    pub backing_type_enum: Option<RustNameComponents>,

    pub cs_name_components: NameComponents,
    pub rs_name_components: RustNameComponents,
    pub(crate) prefix_comments: Vec<String>,

    pub requirements: RustTypeRequirements,
    pub packing: Option<u32>,
    pub size_info: Option<SizeInfo>,
    pub is_compiler_generated: bool,
}
impl RustType {
    pub(crate) fn make_rust_type(
        tag: CsTypeTag,
        cs_type: &CsType,
        config: &RustGenerationConfig,
    ) -> Self {
        let cs_name_components = &cs_type.cs_name_components;

        let generics = cs_type.generic_container.as_ref().map(|g| {
            g.args
                .iter()
                .map(|arg| RustGeneric {
                    name: arg.name.to_string(),
                    bounds: vec!["quest_hook::libil2cpp::Type".to_string()],
                })
                .collect_vec()
        });

        let rs_name_components = RustNameComponents {
            generics,
            name: config.name_rs(&cs_name_components.name),
            namespace: Some(
                config.namespace_rs(&cs_name_components.namespace.clone().unwrap_or_default()),
            ),
            is_ref: false,
            is_dyn: false,
            is_ptr: cs_type.is_reference_type,
            is_mut: cs_type.is_reference_type, // value types don't need to be mutable
            ..Default::default()
        };

        let feature_impl_class =
            config.feature_impl_class(&cs_name_components.clone().remove_generics().combine_all());

        let feature_def_class = config.feature_def_class(&feature_impl_class);

        RustType {
            fields: Default::default(),
            methods: Default::default(),
            traits: Default::default(),
            constants: Default::default(),
            nested_types: Default::default(),

            is_value_type: cs_type.is_value_type,
            is_enum_type: cs_type.is_enum_type,
            is_reference_type: cs_type.is_reference_type,
            is_interface: cs_type.is_interface,
            parent: Default::default(),
            backing_type_enum: Default::default(),

            requirements: RustTypeRequirements::default(),
            self_impl_feature: Some(
                RustFeature {
                    name: feature_impl_class,
                }
                .into(),
            ),
            self_def_feature: Some(
                RustFeature {
                    name: feature_def_class,
                }
                .into(),
            ),

            self_tag: tag,

            rs_name_components,
            cs_name_components: cs_type.cs_name_components.clone(),
            prefix_comments: vec![],
            packing: cs_type.packing.map(|p| p as u32),
            size_info: cs_type.size_info.clone(),
            is_compiler_generated: cs_type.is_compiler_generated,
        }
    }

    pub fn fill(
        &mut self,
        cs_type: CsType,
        name_resolver: &RustNameResolver,
        config: &RustGenerationConfig,
    ) {
        if cs_type.is_interface || cs_type.namespace() == "System" && cs_type.name() == "Object" {
            self.make_object_parent();
        } else {
            self.make_parent(cs_type.parent.as_ref(), name_resolver);
        }

        self.make_nested_types(&cs_type.nested_types, name_resolver);
        self.make_interfaces(&cs_type.interfaces, name_resolver, config);

        self.make_fields(&cs_type.fields, name_resolver, config);

        self.make_methods(&cs_type.methods, name_resolver, config);

        // add phantom markers
        self.make_generics();

        if self.is_reference_type {
            self.make_ref_constructors(&cs_type.constructors, name_resolver, config);
        }

        if self.is_interface {
            // Runtime-checked casts from a dynamically-typed object to this interface, backed
            // by `is_assignable_from` rather than an unconditional transmute - unlike
            // `make_interfaces`'s `AsRef`/`AsMut` (sound by construction, since the concrete
            // type's declared interfaces are known at codegen time), the object passed here
            // could be any instance, so soundness has to be checked at runtime instead.
            self.methods.push(
                RustFunction {
                    name: format_ident!("try_cast"),
                    body: Some(parse_quote! {
                        match <Self as quest_hook::libil2cpp::Type>::class().is_assignable_from(object.class()) {
                            true => Some(unsafe { &*(object as *const quest_hook::libil2cpp::Il2CppObject as *const Self) }),
                            false => None,
                        }
                    }),
                    generics: Default::default(),
                    is_mut: false,
                    is_ref: false,
                    is_self: false,
                    where_clause: None,
                    feature: None,
                    params: vec![RustParam {
                        name: format_ident!("object"),
                        param_type: parse_quote!(&quest_hook::libil2cpp::Il2CppObject),
                    }],
                    return_type: Some(parse_quote!(Option<&Self>)),
                    visibility: Visibility::Public,
                }
                .into(),
            );
            self.methods.push(
                RustFunction {
                    name: format_ident!("try_cast_mut"),
                    body: Some(parse_quote! {
                        let __cordl_matches = <Self as quest_hook::libil2cpp::Type>::class().is_assignable_from(object.class());
                        match __cordl_matches {
                            true => Some(unsafe { &mut *(object as *mut quest_hook::libil2cpp::Il2CppObject as *mut Self) }),
                            false => None,
                        }
                    }),
                    generics: Default::default(),
                    is_mut: false,
                    is_ref: false,
                    is_self: false,
                    where_clause: None,
                    feature: None,
                    params: vec![RustParam {
                        name: format_ident!("object"),
                        param_type: parse_quote!(&mut quest_hook::libil2cpp::Il2CppObject),
                    }],
                    return_type: Some(parse_quote!(Option<&mut Self>)),
                    visibility: Visibility::Public,
                }
                .into(),
            );
        }

        // check if any method name matches more than once
        let duplicated_methods = self
            .methods
            .iter()
            .filter(|m| {
                self.methods
                    .iter()
                    .filter(|m2| m2.is_self == m.is_self && m2.name == m.name)
                    .count()
                    > 1
            })
            .collect_vec();
        if !duplicated_methods.is_empty() {
            panic!(
                "Duplicate method names found! {} {}",
                self.rs_name_components.combine_all(),
                duplicated_methods.iter().map(|m| &m.name).join(", ")
            );
        }

        if let Some(backing_type) = cs_type.enum_backing_type {
            let backing_ty = RustNameResolver::primitive_to_rust_ty(&backing_type);
            let resolved_ty = RustNameComponents {
                name: backing_ty.to_owned(),
                namespace: None,
                generics: None,
                is_ref: false,
                is_ptr: false,
                is_mut: false,
                ..Default::default()
            };

            self.backing_type_enum = Some(resolved_ty);
        }
    }

    fn make_parent(
        &mut self,
        parent: Option<&ResolvedType>,
        name_resolver: &RustNameResolver<'_, '_>,
    ) {
        if self.is_value_type || self.is_enum_type {
            return;
        }

        let Some(parent) = parent else { return };
        let parent = name_resolver
            .resolve_name(self, parent, TypeUsage::TypeName, false, true)
            .with_no_prefix();
        let parent_field = RustField {
            name: format_ident!("{}", PARENT_FIELD),
            field_type: parent.to_type_token(),
            visibility: Visibility::Private,
            offset: 0,
        };

        self.fields.insert(0, parent_field.into());
        self.parent = Some(parent);
    }
    fn make_object_parent(&mut self) {
        if self.is_value_type || self.is_enum_type {
            return;
        }

        let parent = RustNameComponents {
            name: "Il2CppObject".to_string(),
            namespace: Some("quest_hook::libil2cpp".to_string()),
            generics: None,
            is_mut: false,
            is_ptr: false,

            is_ref: false,
            is_dyn: false,
            is_static_ref: false,
        };
        let parent_field = RustField {
            name: format_ident!("{}", PARENT_FIELD),
            field_type: parent.to_type_token(),
            visibility: Visibility::Private,
            offset: 0,
        };

        self.fields.insert(0, parent_field.into());
        self.parent = Some(parent);
    }

    fn make_nested_types(
        &mut self,
        nested_types: &HashSet<CsTypeTag>,
        name_resolver: &RustNameResolver<'_, '_>,
    ) {
        let nested_types = nested_types
            .iter()
            .filter_map(|tag| name_resolver.collection.get_rust_type(*tag))
            .filter(|t| !t.is_compiler_generated)
            .sorted_by(|a, b| a.rs_name_components.name.cmp(&b.rs_name_components.name))
            .map(|rust_type| -> syn::ItemType {
                let mut name = name_resolver
                    .config
                    .name_rs(&rust_type.cs_name_components.name);

                if name == "Target" {
                    // avoid conflict with Deref
                    name = "TargetType".to_string();
                }

                let name_ident = format_ident!("{name}",);

                let target = rust_type.rs_name_components.to_type_path_token();

                let declaring_generic_count = self
                    .rs_name_components
                    .generics
                    .as_ref()
                    .map(|g| g.len())
                    .unwrap_or_default();
                let target_generics = rust_type.get_generics(declaring_generic_count);

                let visibility = match rust_type.is_interface {
                    false => Visibility::Public,
                    true => Visibility::Private,
                }
                .to_token_stream();

                let feature = rust_type
                    .self_impl_feature
                    .as_ref()
                    .map(|f| f.to_token_stream());

                parse_quote! {
                    #feature
                    #visibility type #name_ident #target_generics = #target;
                }
            })
            .map(CustomArc::from);

        self.nested_types = nested_types.collect();
    }

    fn make_fields(
        &mut self,
        fields: &[CsField],
        name_resolver: &RustNameResolver,
        config: &RustGenerationConfig,
    ) {
        let instance_fields = fields
            .iter()
            .filter(|f| f.instance && !f.is_const)
            .cloned()
            .collect_vec();

        if self.is_value_type && !self.is_enum_type {
            rust_fields::handle_valuetype_fields(self, &instance_fields, name_resolver, config);
        } else {
            rust_fields::handle_referencetype_fields(self, &instance_fields, name_resolver, config);
        }

        rust_fields::handle_static_fields(self, fields, name_resolver, config);
        rust_fields::handle_const_fields(self, fields, name_resolver, config);

        // for f in fields {
        //     if !f.instance || f.is_const {
        //         continue;
        //     }
        //     let field_type = name_resolver.resolve_name(self, &f.field_ty, TypeUsage::Field, true);

        //     let rust_field = RustField {
        //         name: config.name_rs(&f.name),
        //         field_type: RustItem::NamedType(field_type.combine_all()),
        //         visibility: Visibility::Public,
        //         offset: f.offset.unwrap_or_default(),
        //     };
        //     self.fields.push(rust_field);
        // }
    }

    fn make_generics(&mut self) {
        let Some(generic) = &self.rs_name_components.generics else {
            return;
        };

        for g in generic.iter() {
            let name = format_ident!("{}", g.name);

            self.fields.push(
                RustField {
                    name: format_ident!("__cordl_phantom_{name}"),
                    field_type: parse_quote!(std::marker::PhantomData<#name>),
                    visibility: Visibility::Private,
                    offset: 0,
                }
                .into(),
            );
        }
    }

    fn make_interfaces(
        &mut self,
        interfaces: &[ResolvedType],
        name_resolver: &RustNameResolver,
        config: &RustGenerationConfig,
    ) {
        for i in interfaces {
            let self_ident = self.rs_name_components.to_type_path_token();

            let generics = self.get_generics(0);

            let interface = name_resolver.resolve_name(self, i, TypeUsage::TypeName, true, false);
            let interface_ident = interface.to_type_path_token();

            // Sound by construction: C# metadata already guarantees the concrete type declares
            // this interface, so a plain reinterpret is safe here (contrast with the interface's
            // own `try_cast`, which checks at runtime because it starts from an arbitrary
            // object).

            // Value types have no interface-typed representation of their own - C#
            // only exposes one after boxing - so the impl target is `BoxedValue<Self>` instead
            // of `Self` for them.
            let target: syn::Type = match self.is_reference_type {
                true => parse_quote!(#self_ident),
                false => parse_quote!(quest_hook::libil2cpp::BoxedValue<#self_ident>),
            };
            let impl_data: Vec<syn::Stmt> = parse_quote! {
                unsafe { std::mem::transmute(self) }
            };
            let as_ref = RustTraitImpl {
                name: interface.combine_all(),
                impl_data: parse_quote! {
                    impl #generics AsRef<#interface_ident> for #target {
                        fn as_ref(&self) -> & #interface_ident {
                            #(#impl_data)*
                        }
                    }
                },
            };
            let as_mut = RustTraitImpl {
                name: interface.combine_all(),
                impl_data: parse_quote! {
                    impl #generics AsMut<#interface_ident> for #target {
                        fn as_mut(&mut self) -> &mut #interface_ident {
                            #(#impl_data)*
                        }
                    }
                },
            };
            self.traits.push(as_ref.into());
            self.traits.push(as_mut.into());
        }
    }

    fn make_ref_constructors(
        &mut self,
        constructors: &[CsConstructor],
        name_resolver: &RustNameResolver<'_, '_>,
        config: &RustGenerationConfig,
    ) {
        let overloaded_method_data = constructors
            .iter()
            .map(|m| (m.name.clone(), m.parameters.as_slice()))
            .collect_vec();

        for (i, c) in constructors.iter().enumerate() {
            let m_name_rs = self.make_overloaded_name(
                &overloaded_method_data,
                name_resolver,
                ("New".to_string(), c.parameters.as_slice()),
                i,
            );

            let params = c
                .parameters
                .iter()
                .map(|p| self.make_parameter(p, name_resolver, config))
                .collect_vec();

            let param_names = params.iter().map(|p| &p.name).collect_vec();
            let param_types = params.iter().map(|p| &p.param_type).collect_vec();
            let n = params.len();

            let generics = c
                .generic_container
                .as_ref()
                .map(|t| {
                    t.just_names()
                        .map(|g| RustGeneric {
                            name: g.clone(),
                            bounds: vec!["quest_hook::libil2cpp::Type".to_string()],
                        })
                        .collect_vec()
                })
                .unwrap_or_default();
            let g_ty = Self::generics_tuple_type(&generics);

            // Cache the `.ctor` lookup ourselves rather than going through
            // `Il2CppObject::invoke_void`, which repeats an uncached `find_method` by name on
            // every call - same reasoning as the OnceLock caching in `make_method_body`.
            let body: Vec<syn::Stmt> = parse_quote! {
                let __cordl_object: &mut Self = <Self as quest_hook::libil2cpp::Type>::class().instantiate();

                static METHOD: std::sync::OnceLock<&'static quest_hook::libil2cpp::MethodInfo> = std::sync::OnceLock::new();
                let __cordl_ctor: &'static quest_hook::libil2cpp::MethodInfo = *METHOD.get_or_init(|| {
                    <Self as quest_hook::libil2cpp::Type>::class()
                        .find_method::<(#(#param_types),*), #g_ty, (), #n>(".ctor")
                        .unwrap_or_else(|e| {
                            panic!(
                                "no matching constructor found for {}(...) Cause: {e:?}",
                                <Self as quest_hook::libil2cpp::Type>::class()
                            )
                        })
                });

                let _cordl_ctor_ret: () = unsafe { __cordl_ctor.invoke_unchecked(&mut *__cordl_object, (#(#param_names),*))? };

                Ok(__cordl_object.into())
            };

            let combined_generics = self
                .rs_name_components
                .generics
                .clone()
                .unwrap_or_default()
                .into_iter()
                .chain(generics.clone().into_iter())
                .map(Self::with_arg_bounds)
                .map(|g| -> syn::GenericParam { g.to_token_stream() })
                .collect_vec();

            let where_clause: syn::WhereClause = parse_quote! {
                where #(#combined_generics),*
            };

            let rust_func = RustFunction {
                name: format_ident!("{}", m_name_rs),
                body: Some(body),
                generics,

                is_mut: true,
                is_ref: true,
                is_self: false,
                params,
                where_clause: Some(where_clause),

                feature: None,

                return_type: Some(parse_quote!(
                    quest_hook::libil2cpp::Result<quest_hook::libil2cpp::Gc<Self>>
                )),
                visibility: (Visibility::Public),
            };
            self.methods.push(rust_func.into());
        }
    }

    fn make_overloaded_name<'a>(
        &mut self,
        overload_methods: &Vec<(String, &'a [CsParam])>,
        name_resolver: &RustNameResolver<'_, '_>,
        (m_name, m_params): (String, &'a [CsParam]),
        index: usize,
    ) -> String {
        let config = name_resolver.config;

        let mut m_name_rs = config.name_rs(&m_name);
        if overload_methods.len() == 1 {
            return m_name_rs;
        }

        let param_types: Vec<_> = overload_methods
            .iter()
            .map(|(m_name, m_params)| {
                m_params
                    .iter()
                    .map(|p| {
                        name_resolver
                            .resolve_name(self, &p.il2cpp_ty, TypeUsage::Parameter, true, false)
                            .name
                    })
                    .map(|s| config.name_rs(&s))
                    .collect::<Vec<_>>()
            })
            .collect();

        // current name
        let current_param_types = m_params
            .iter()
            .map(|p| {
                name_resolver
                    .resolve_name(self, &p.il2cpp_ty, TypeUsage::Parameter, true, true)
                    .name
            })
            .map(|s| config.name_rs(&s))
            .collect::<Vec<_>>();

        let differing_params: Vec<_> = current_param_types
            .iter()
            .enumerate()
            .filter(|(i, ty)| param_types.iter().any(|types| types.get(*i) != Some(ty)))
            .map(|(_, ty)| ty.clone())
            .collect();

        if !differing_params.is_empty() {
            m_name_rs = format!("{m_name_rs}_{}", differing_params.join("_"));
        } else {
            // fallback
            m_name_rs = format!("{m_name_rs}_{}", current_param_types.join("_"));
        }

        if m_name_rs.chars().last().is_some_and(|s| s.is_numeric()) {
            m_name_rs = format!("{m_name_rs}_{index}");
        } else {
            m_name_rs = format!("{m_name_rs}{index}");
        }
        m_name_rs
    }

    fn make_methods(
        &mut self,
        methods: &[CsMethod],
        name_resolver: &RustNameResolver,
        config: &RustGenerationConfig,
    ) {
        for (_, overload_methods) in methods
            .iter()
            // .filter(|m| m.instance)
            .into_group_map_by(|m| &m.name)
        {
            let overloaded_method_data = overload_methods
                .iter()
                .map(|m| (m.name.clone(), m.parameters.as_slice()))
                .collect_vec();

            for (i, m) in overload_methods.iter().enumerate() {
                let m_name = &m.name;

                let m_name_rs = self.make_overloaded_name(
                    &overloaded_method_data,
                    name_resolver,
                    (m_name.clone(), m.parameters.as_slice()),
                    i,
                );

                let m_ret_ty = name_resolver
                    .resolve_name(self, &m.return_type, TypeUsage::ReturnType, true, true)
                    .wrap_by_gc();
                let m_ret_ty_ident = m_ret_ty.to_type_token();
                let m_result_ty: syn::Type =
                    parse_quote!(quest_hook::libil2cpp::Result<#m_ret_ty_ident>);

                let params = m
                    .parameters
                    .iter()
                    .map(|p| self.make_parameter(p, name_resolver, config))
                    .collect_vec();

                let param_names = params.iter().map(|p| &p.name);
                let param_types = params.iter().map(|p| &p.param_type);

                let method_generics = m
                    .generic_container
                    .as_ref()
                    .map(|t| {
                        t.just_names()
                            .map(|g| -> RustGeneric {
                                RustGeneric {
                                    name: g.clone(),
                                    bounds: vec![],
                                }
                            })
                            .collect_vec()
                    })
                    .unwrap_or_default();

                let body = self.make_method_body(
                    m,
                    m_name,
                    param_types,
                    param_names,
                    m_ret_ty_ident,
                    &method_generics,
                );

                let combined_generics = self
                    .rs_name_components
                    .generics
                    .clone()
                    .unwrap_or_default()
                    .into_iter()
                    .chain(method_generics.clone().into_iter())
                    .map(Self::with_arg_bounds)
                    .map(|g| -> syn::GenericParam { g.to_token_stream() })
                    .collect_vec();

                let where_clause: syn::WhereClause = parse_quote! {
                    where #(#combined_generics),*
                };

                let rust_func = RustFunction {
                    name: format_ident!("{m_name_rs}"),
                    body: Some(body),
                    generics: method_generics,
                    is_mut: m.instance,
                    is_ref: m.instance,
                    is_self: m.instance,
                    params,
                    where_clause: Some(where_clause),

                    feature: None,

                    return_type: Some(m_result_ty),
                    visibility: (Visibility::Public),
                };
                self.methods.push(rust_func.into());
            }
        }
    }

    /// Builds the `G` (method-generics) type argument for `find_method`/`find_static_method`/
    /// `MethodInfo::make_generic`: `()` for no generics, a bare type for one (matches
    /// quest_hook's blanket `impl<T: Type> Generics for T`), or a real tuple for N (matches its
    /// macro-generated tuple impls) - mirrors [`Self::get_generics_names_args`]'s tuple shape.
    fn generics_tuple_type(generics: &[RustGeneric]) -> syn::Type {
        let idents = generics.iter().map(|g| format_ident!("{}", g.name));
        parse_quote!( (#(#idents),*) )
    }

    /// `Type::class()`'s default implementation (quest_hook's `typecheck/ty.rs`) does an
    /// uncached `Il2CppClass::find(namespace, name)` on every call - a string-keyed lookup
    /// that's on the hot path of every generated method/constructor call (each looks up
    /// `Self::class()` at least once). Overriding it here with a `OnceLock` cache turns every
    /// call after the first into a plain atomic load.
    fn class_fn_override(
        namespace: &str,
        class_name: &str,
        generic_names: Option<Vec<syn::GenericArgument>>,
    ) -> syn::ImplItemFn {
        let lookup: syn::Expr = match generic_names {
            Some(names) => parse_quote! {
                quest_hook::libil2cpp::Il2CppClass::find(#namespace, #class_name)
                    .unwrap_or_else(|| panic!("Class {}.{} not found", #namespace, #class_name))
                    .make_generic::<(#(#names),*)>()
                    .unwrap_or_else(|e| panic!("failed to instantiate generic class {}.{}: {e:?}", #namespace, #class_name))
                    .unwrap_or_else(|| panic!("generic class instantiation returned no class for {}.{}", #namespace, #class_name))
            },
            None => parse_quote! {
                quest_hook::libil2cpp::Il2CppClass::find(#namespace, #class_name)
                    .unwrap_or_else(|| panic!("Class {}.{} not found", #namespace, #class_name))
            },
        };

        parse_quote! {
            fn class() -> &'static quest_hook::libil2cpp::Il2CppClass {
                static CLASS: ::std::sync::OnceLock<&'static quest_hook::libil2cpp::Il2CppClass> = ::std::sync::OnceLock::new();
                CLASS.get_or_init(|| #lookup)
            }
        }
    }

    /// Bounds required on a generic type argument passed by value across the quest_hook FFI
    /// boundary (as a method/constructor argument or return value).
    fn with_arg_bounds(mut g: RustGeneric) -> RustGeneric {
        g.bounds.extend([
            "quest_hook::libil2cpp::Type".to_string(),
            "quest_hook::libil2cpp::Argument".to_string(),
            "quest_hook::libil2cpp::Returned".to_string(),
        ]);
        g
    }

    fn make_method_body<'a>(
        &self,
        m: &CsMethod,
        m_name: &String,
        param_types: impl Iterator<Item = &'a syn::Type>,
        param_names: impl Iterator<Item = &'a syn::Ident>,
        m_ret_ty: syn::Type,
        method_generics: &[RustGeneric],
    ) -> Vec<syn::Stmt> {
        let param_types = param_types.collect_vec();
        let n = param_types.len();

        let method_name = format_ident!("cordl_method_info");
        let g_ty = Self::generics_tuple_type(method_generics);
        let is_generic = !method_generics.is_empty();

        let instantiate_generic: Option<syn::Expr> = is_generic.then(|| {
            parse_quote! {
                __cordl_base_method
                    .make_generic::<#g_ty>()
                    .unwrap_or_else(|e| panic!("failed to instantiate generic method {}: {e:?}", #m_name))
                    .unwrap_or_else(|| panic!("generic method instantiation returned no method for {}", #m_name))
            }
        });

        // instance methods declared on an interface, or virtual/abstract (and not final),
        // must be resolved against the vtable slot of the *actual* runtime class of `self`
        // every call - the concrete override can differ per subclass, so (unlike the
        // name+signature lookup below) this can never be cached in a `static`. Mirrors the
        // C++ backend's `should_resolve_slot` (src/generate/cpp/cpp_type.rs).
        let is_virtual = m.method_flags.contains(CSMethodFlags::VIRTUAL);
        let is_abstract = m.method_flags.contains(CSMethodFlags::ABSTRACT);
        let is_final = m.method_flags.contains(CSMethodFlags::FINAL);
        let should_resolve_slot =
            m.instance && (self.is_interface || ((is_virtual || is_abstract) && !is_final));

        let get_method: Vec<syn::Stmt> = if should_resolve_slot {
            let slot = m.method_data.slot.unwrap_or_else(|| {
                panic!(
                    "virtual/interface method {} on {} has no vtable slot",
                    m_name,
                    self.rs_name()
                )
            });

            let resolved: syn::Expr = match &instantiate_generic {
                Some(instantiate_generic) => instantiate_generic.clone(),
                None => parse_quote!(__cordl_base_method),
            };

            parse_quote! {
                let __cordl_self_class = quest_hook::libil2cpp::RefType::as_object(self).class();
                let __cordl_declaring_class = <Self as quest_hook::libil2cpp::Type>::class();
                let __cordl_base_method: &'static quest_hook::libil2cpp::MethodInfo = __cordl_self_class
                    .find_method_by_vtable(__cordl_declaring_class, #slot)
                    .unwrap_or_else(|| panic!("no vtable method found for slot {} of {}", #slot, #m_name));
                let #method_name: &'static quest_hook::libil2cpp::MethodInfo = #resolved;
            }
        } else {
            let find_call: syn::Expr = match m.instance {
                true => parse_quote! {
                    <Self as quest_hook::libil2cpp::Type>::class()
                        .find_method::<(#(#param_types),*), #g_ty, #m_ret_ty, #n>(#m_name)
                        .unwrap_or_else(|e| {
                            panic!(
                                "no matching methods found for non-void {}.{}({}) Cause: {e:?}",
                                <Self as quest_hook::libil2cpp::Type>::class(),
                                #m_name,
                                #n
                            )
                        })
                },
                false => parse_quote! {
                    <Self as quest_hook::libil2cpp::Type>::class()
                        .find_static_method::<(#(#param_types),*), #g_ty, #m_ret_ty, #n>(#m_name)
                        .unwrap_or_else(|e| {
                            panic!(
                                "no matching methods found for non-void {}.{}({}) Cause: {e:?}",
                                <Self as quest_hook::libil2cpp::Type>::class(),
                                #m_name,
                                #n
                            )
                        })
                },
            };

            let resolved: syn::Expr = match &instantiate_generic {
                Some(instantiate_generic) => instantiate_generic.clone(),
                None => parse_quote!(__cordl_base_method),
            };

            parse_quote! {
                static METHOD: std::sync::OnceLock<&'static quest_hook::libil2cpp::MethodInfo> = std::sync::OnceLock::new();
                let #method_name: &'static quest_hook::libil2cpp::MethodInfo = *METHOD.get_or_init(|| {
                    let __cordl_base_method: &'static quest_hook::libil2cpp::MethodInfo = #find_call;
                    #resolved
                });
            }
        };

        let invoke_call: Vec<syn::Stmt> = match m.instance {
            true => parse_quote! {
                #(#get_method)*

                let __cordl_ret: #m_ret_ty = unsafe { #method_name.invoke_unchecked(self, ( #(#param_names),* ))? };

                Ok(__cordl_ret.into())
            },
            false => parse_quote! {
                #(#get_method)*

                let __cordl_ret: #m_ret_ty = unsafe { #method_name.invoke_unchecked((), ( #(#param_names),* ))? };

                Ok(__cordl_ret.into())
            },
        };

        parse_quote! {
            #(#invoke_call)*
        }
    }

    fn make_parameter(
        &mut self,
        p: &CsParam,
        name_resolver: &RustNameResolver<'_, '_>,
        config: &RustGenerationConfig,
    ) -> RustParam {
        let p_ty = name_resolver
            .resolve_name(self, &p.il2cpp_ty, TypeUsage::Parameter, true, false)
            .wrap_by_gc();
        // let p_il2cpp_ty = p.il2cpp_ty.get_type(name_resolver.cordl_metadata);

        let name_rs = config.name_rs(&p.name);
        RustParam {
            name: format_ident!("{name_rs}"),
            param_type: p_ty.to_type_token(),
            // is_ref: p_il2cpp_ty.is_byref(),
            // is_ptr: !p_il2cpp_ty.valuetype,
            // is_mut: true,
        }
    }

    pub fn name(&self) -> &String {
        &self.cs_name_components.name
    }

    pub fn namespace(&self) -> Option<&str> {
        self.cs_name_components.namespace.as_deref()
    }

    pub fn rs_name(&self) -> &String {
        &self.rs_name_components.name
    }
    pub fn rs_namespace(&self) -> &Option<String> {
        &self.rs_name_components.namespace
    }

    pub(crate) fn write(&self, writer: &mut Writer, config: &RustGenerationConfig) -> Result<()> {
        if self.is_value_type {
            if self.is_enum_type {
                self.write_enum_type(writer, config)?;
            } else {
                self.write_value_type(writer, config)?;
            }
        }

        if self.is_interface {
            self.write_interface(writer, config)?;
        } else if self.is_reference_type {
            self.write_reference_type(writer, config)?;
        }

        Ok(())
    }

    pub fn nested_fixup(
        &mut self,
        context_tag: &CsTypeTag,
        cs_type: &CsType,
        metadata: &CordlMetadata,
        config: &RustGenerationConfig,
    ) {
        // Nested type unnesting fix
        let Some(declaring_tag) = cs_type.declaring_ty.as_ref() else {
            return;
        };

        let mut declaring_td = declaring_tag
            .get_tdi()
            .get_type_definition(metadata.metadata);
        let mut declaring_name = declaring_td.get_name_components(metadata.metadata).name;

        while declaring_td.declaring_type_index != u32::MAX {
            let declaring_ty =
                &metadata.metadata_registration.types[declaring_td.declaring_type_index as usize];

            let declaring_tag =
                cs_type_tag::CsTypeTag::from_type_data(declaring_ty.data, metadata.metadata);

            declaring_td = declaring_tag
                .get_tdi()
                .get_type_definition(metadata.metadata);

            let name = declaring_td.get_name_components(metadata.metadata).name;
            declaring_name = format!("{declaring_name}_{name}",);
        }

        let context_td = context_tag.get_tdi().get_type_definition(metadata.metadata);
        let declaring_namespace = context_td.namespace(metadata.metadata);

        let combined_name = format!("{}_{}", declaring_name, self.name());

        self.rs_name_components.namespace = Some(config.namespace_rs(declaring_namespace));
        self.rs_name_components.name = config.name_rs(&combined_name);
    }
    pub fn enum_fixup(&mut self, cs_type: &CsType) {
        if !cs_type.is_enum_type {
            return;
        }
        self.rs_name_components.generics = None;
    }

    /// easy switch to enable optional derive feature
    pub const OPTIONAL_DERIVE_FEATURE: bool = true;

    fn derive_feature(s: &str) -> String {
        format!("derive_{}", s)
    }

    /// Generate optional derive attribute based on impl feature
    /// TODO: Fields require this bound too so this doesn't help at the moment
    fn optional_derive(&self, derives: &[&str]) -> TokenStream {
        let derives = derives.iter().map(|d| format_ident!("{}", d));

        match &self.self_impl_feature {
            // single feature for all derives
            // Some(feature) if Self::OPTIONAL_DERIVE_FEATURE => {
            //     let name = &feature.name;
            //     quote! {
            //         #[cfg_attr(feature = #name, derive( #(#derives),* ))]
            //     }
            // }

            // format derives individually
            Some(feature) if Self::OPTIONAL_DERIVE_FEATURE => {
                let cfg_derives = derives.map(|d| {
                    let feature_name = Self::derive_feature(&d.to_string());
                    quote! {
                        #[cfg_attr(feature = #feature_name, derive( #d ))]
                    }
                });

                quote! {
                    #(#cfg_derives)*
                }
            }
            _ => {
                quote! {
                    #[derive(#(#derives),*)]
                }
            }
        }
    }

    fn write_reference_type(
        &self,
        writer: &mut Writer,
        config: &RustGenerationConfig,
    ) -> Result<()> {
        let name_ident = self.rs_name_components.clone().to_name_ident();
        let path_ident = self.rs_name_components.to_type_path_token();

        let generics = self.get_generics(0);
        let generics_names = self.get_generics_unbound(0);

        let fields = self.fields.iter().map(|f| {
            let f_name = format_ident!(r#"{}"#, f.name);
            let f_ty = &f.field_type;
            let f_visibility = match f.visibility {
                Visibility::Public => quote! { pub },
                Visibility::PublicCrate => quote! { pub(crate) },
                Visibility::Private => quote! {},
            };

            quote! {
                #f_visibility #f_name: #f_ty
            }
        });

        let cs_namespace = self
            .cs_name_components
            .namespace
            .clone()
            .unwrap_or_default();
        let cs_name_str = self
            .cs_name_components
            .clone()
            .remove_namespace()
            .remove_generics()
            .combine_all();

        let impl_ref = self.implement_reference_type();

        let def_feature = self.self_def_feature.as_ref().map(|f| f.to_token_stream());
        let impl_feature = self.self_impl_feature.as_ref().map(|f| f.to_token_stream());

        let derives = self.optional_derive(&["Debug"]);

        let mut tokens = quote! {
            #def_feature
            #[repr(C)]
            #derives
            pub struct #name_ident {
                #(#fields),*
            }

            #impl_ref

        };

        // add Deref for parent
        if let Some(parent) = &self.parent {
            let parent_name = parent.clone().to_type_path_token();
            let parent_field_ident = format_ident!(r#"{}"#, PARENT_FIELD);

            tokens.extend(quote! {
            #impl_feature
            impl #generics std::ops::Deref for #path_ident {
                type Target = #parent_name;

                fn deref(&self) -> &<Self as std::ops::Deref>::Target {
                    unsafe {&self.#parent_field_ident}
                }
            }

            #impl_feature
            impl #generics std::ops::DerefMut for #path_ident {
                fn deref_mut(&mut self) -> &mut <Self as std::ops::Deref>::Target {
                    unsafe{ &mut self.#parent_field_ident }
                }
            }

            });
        }

        writer.write_pretty_tokens(tokens)?;

        self.write_impl(writer, config)?;
        Ok(())
    }

    fn write_enum_type(&self, writer: &mut Writer, config: &RustGenerationConfig) -> Result<()> {
        let fields = self
            .constants
            .iter()
            .enumerate()
            .map(|(i, f)| -> syn::Variant {
                let name = &f.name;
                let val = &f.value;

                let default_feature_name = Self::derive_feature("Default");

                let default_variant =
                    if self.self_impl_feature.is_some() && Self::OPTIONAL_DERIVE_FEATURE {
                        quote! {
                            #[cfg_attr(feature = #default_feature_name, default)]
                        }
                    } else {
                        quote! {
                            #[default]
                        }
                    };

                // add default for enum
                if i == 0 {
                    return parse_quote! {
                        #default_variant
                        #name = #val
                    };
                }

                parse_quote! {
                    #name = #val
                }
            });
        let backing_type = self
            .backing_type_enum
            .as_ref()
            .wrap_err("No enum backing type found!")?
            .to_type_token();

        let name_ident = self.rs_name_components.to_name_ident();
        let path_ident = self.rs_name_components.to_type_path_token();

        let cs_namespace = self
            .cs_name_components
            .namespace
            .clone()
            .unwrap_or_default();
        let cs_name_str = self
            .cs_name_components
            .clone()
            .remove_namespace()
            .combine_all();

        let quest_hook_path: syn::Path = parse_quote!(quest_hook::libil2cpp);
        let impl_value = self.implement_value_type();

        let feature = self.self_def_feature.as_ref().map(|f| f.to_token_stream());
        let mapped_feature_derive =
            self.optional_derive(&["Debug", "Clone", "Copy", "PartialEq", "Eq", "Default"]);

        let tokens = quote! {
            #feature
            #mapped_feature_derive
            #[repr(#backing_type)]
            pub enum #name_ident {
                #(#fields),*
            }


            #impl_value
        };

        writer.write_pretty_tokens(tokens)?;

        // self.write_impl(writer, config)?;

        Ok(())
    }

    fn write_value_type(&self, writer: &mut Writer, config: &RustGenerationConfig) -> Result<()> {
        let generics = self.get_generics(0);
        let generic_names = self.get_generics_unbound(0);

        let name_ident = self.rs_name_components.clone().to_name_ident();
        let path_ident = self.rs_name_components.to_type_path_token();

        let fields = self.fields.iter().map(|f| {
            let f_name = format_ident!(r#"{}"#, f.name);
            let f_ty = &f.field_type;
            let f_visibility = match f.visibility {
                Visibility::Public => quote! { pub },
                Visibility::PublicCrate => quote! { pub(crate) },
                Visibility::Private => quote! {},
            };

            quote! {
                #f_visibility #f_name: #f_ty
            }
        });

        let cs_namespace = self
            .cs_name_components
            .namespace
            .clone()
            .unwrap_or_default();
        let cs_name_str = self
            .cs_name_components
            .clone()
            .remove_namespace()
            .combine_all();

        let quest_hook_path: syn::Path = parse_quote!(quest_hook::libil2cpp);
        let impl_value = self.implement_value_type();

        let feature = self.self_def_feature.as_ref().map(|f| f.to_token_stream());
        let mapped_feature_derive =
            self.optional_derive(&["Debug", "Clone", "Default", "PartialEq"]);

        let tokens = quote! {
            #feature
            #mapped_feature_derive
            #[repr(C)]
            pub struct #name_ident {
                #(#fields),*
            }


            #impl_value

            // implement ThisArgument for value types
            #feature
            unsafe impl #generics #quest_hook_path::ThisArgument for #path_ident {
                type Type = Self;

                fn matches(method: &#quest_hook_path::MethodInfo) -> bool {
                    <Self as #quest_hook_path::Type>::matches_this_argument(method)
                }

                fn invokable(&mut self) -> *mut std::ffi::c_void {
                    unsafe { #quest_hook_path::value_box(self) as *mut std::ffi::c_void }
                }
            }
        };

        writer.write_pretty_tokens(tokens)?;

        self.write_impl(writer, config)?;

        Ok(())
    }

    fn get_generics(&self, skip_amount: usize) -> Option<syn::Generics> {
        self.rs_name_components
            .generics
            .as_ref()
            .map(|g| {
                g.iter()
                    .skip(skip_amount)
                    .map(|g| -> syn::GenericArgument {
                        let s = g.to_string();
                        syn::parse_str(&s).unwrap()
                    })
                    .collect_vec()
            })
            .map(|g| -> syn::Generics {
                parse_quote! { <#(#g),*> }
            })
    }
    fn get_generics_unbound(&self, skip_amount: usize) -> Option<syn::Generics> {
        self.rs_name_components
            .generics
            .as_ref()
            .map(|g| {
                g.iter()
                    .skip(skip_amount)
                    .map(|g: &RustGeneric| -> syn::GenericArgument {
                        syn::parse_str(&g.name).unwrap()
                    })
                    .collect_vec()
            })
            .map(|g| -> syn::Generics {
                parse_quote! { <#(#g),*> }
            })
    }
    fn get_generics_names_args(&self, skip_amount: usize) -> Option<Vec<syn::GenericArgument>> {
        self.rs_name_components.generics.as_ref().map(|g| {
            g.iter()
                .skip(skip_amount)
                .map(|g| -> syn::GenericArgument { syn::parse_str(&g.name).unwrap() })
                .collect_vec()
        })
    }

    fn write_impl(&self, writer: &mut Writer, _config: &RustGenerationConfig) -> Result<()> {
        let name_ident = self.rs_name_components.clone().to_name_ident();
        let path_ident = self.rs_name_components.clone().to_type_path_token();

        let generics = self.get_generics(0);

        let const_fields = self
            .constants
            .iter()
            .sorted_by(|a, b| a.name.cmp(&b.name))
            .map(|f| -> syn::ImplItemConst {
                let name = &f.name;
                let val = &f.value;
                let f_ty = &f.field_type;

                parse_quote! {
                    pub const #name: #f_ty = #val;
                }
            });

        let methods = self
            .methods
            .iter()
            .sorted_by(|a, b| a.name.cmp(&b.name))
            .cloned()
            .map(|f| {
                if f.body.is_some() {
                    return f;
                }

                let mut f = Arc::unwrap_or_clone(f.0);

                f.body = f.body.clone().or(Some(parse_quote! {
                    todo!()
                }));
                f.into()
            })
            .map(|f| f.to_token_stream())
            .map(|f| -> syn::ImplItemFn { parse_quote!(#f) });

        let def_feature = self.self_def_feature.as_ref().map(|f| f.to_token_stream());
        let impl_feature = self.self_impl_feature.as_ref().map(|f| f.to_token_stream());

        let nested_types = &self
            .nested_types
            .iter()
            .sorted_by(|a, b| a.ident.cmp(&b.ident))
            .collect_vec();

        let other_impls = self
            .traits
            .iter()
            .sorted_by(|a, b| a.name.cmp(&b.name))
            .map(|t| -> syn::ItemImpl {
                let impl_data = &t.impl_data;

                parse_quote! {
                    #impl_feature
                    #impl_data
                }
            })
            .collect_vec();

        let impl_tokens: syn::ItemImpl = parse_quote! {
            impl #generics #path_ident {
                #(#const_fields)*
                #(#nested_types)*
                #(#methods)*
            }
        };

        // `RefType` is blanket-implemented by quest_hook for anything implementing
        // `AsRef<Il2CppObject> + AsMut<Il2CppObject>` (no longer Deref-based) - every reference
        // type except Il2CppObject itself (which gets the identity impl from quest_hook) needs
        // to provide those explicitly, delegating one hop through its parent field. The parent
        // field's own `AsRef`/`AsMut<Il2CppObject>` impl (this same mechanism, recursively, or
        // the identity impl if the parent field type *is* Il2CppObject) continues the chain, so
        // this only ever needs to delegate one level up, regardless of hierarchy depth.
        let impl_object_tokens: Option<TokenStream> = self.parent.as_ref().map(|_| {
            let parent_field_ident = format_ident!(r#"{}"#, PARENT_FIELD);

            // TODO: Figure out if we use def/impl feature here
            quote! {
                #def_feature
                impl #generics AsRef<quest_hook::libil2cpp::Il2CppObject> for #path_ident {
                    fn as_ref(&self) -> &quest_hook::libil2cpp::Il2CppObject {
                        AsRef::<quest_hook::libil2cpp::Il2CppObject>::as_ref(&self.#parent_field_ident)
                    }
                }

                #def_feature
                impl #generics AsMut<quest_hook::libil2cpp::Il2CppObject> for #path_ident {
                    fn as_mut(&mut self) -> &mut quest_hook::libil2cpp::Il2CppObject {
                        AsMut::<quest_hook::libil2cpp::Il2CppObject>::as_mut(&mut self.#parent_field_ident)
                    }
                }
            }
        });

        let tokens = quote! {
            #impl_feature
            #impl_tokens


            #impl_object_tokens

            #(#other_impls)*
        };

        writer.write_pretty_tokens(tokens.to_token_stream())?;
        Ok(())
    }

    fn write_interface(&self, writer: &mut Writer, config: &RustGenerationConfig) -> Result<()> {
        let name_ident = self.rs_name_components.clone().to_name_ident();
        let path_ident = self.rs_name_components.to_type_path_token();

        let generics = self.get_generics(0);
        let generics_names = self.get_generics_unbound(0);

        let fields = self.fields.iter().map(|f| {
            let f_name = format_ident!(r#"{}"#, f.name);
            let f_ty = &f.field_type;
            let f_visibility = match f.visibility {
                Visibility::Public => quote! { pub },
                Visibility::PublicCrate => quote! { pub(crate) },
                Visibility::Private => quote! {},
            };

            quote! {
                #f_visibility #f_name: #f_ty
            }
        });

        let cs_namespace = self
            .cs_name_components
            .namespace
            .clone()
            .unwrap_or_default();
        let cs_name_str = self
            .cs_name_components
            .clone()
            .remove_namespace()
            .remove_generics()
            .combine_all();

        let quest_hook_path: syn::Path = parse_quote!(quest_hook::libil2cpp);
        let impl_ref = self.implement_reference_type();

        let def_feature = self.self_def_feature.as_ref().map(|f| f.to_token_stream());
        let impl_feature = self.self_impl_feature.as_ref().map(|f| f.to_token_stream());

        let mapped_feature_derive = self.optional_derive(&["Debug"]);
        let mut tokens = quote! {
            #def_feature
            #mapped_feature_derive
            #[repr(C)]
            pub struct #name_ident {
                #(#fields),*
            }

            #impl_ref

        };

        if let Some(parent) = &self.parent {
            let parent_name = parent.clone().to_type_path_token();
            let parent_field_ident = format_ident!(r#"{}"#, PARENT_FIELD);

            tokens.extend(quote! {
            #impl_feature
            impl #generics std::ops::Deref for #path_ident {
                type Target = #parent_name;

                fn deref(&self) -> &<Self as std::ops::Deref>::Target {
                    unsafe {&self.#parent_field_ident}
                }
            }

            #impl_feature
            impl #generics std::ops::DerefMut for #path_ident {
                fn deref_mut(&mut self) -> &mut <Self as std::ops::Deref>::Target {
                    unsafe{ &mut self.#parent_field_ident }
                }
            }

            });
        }

        writer.write_pretty_tokens(tokens)?;

        self.write_impl(writer, config)?;

        Ok(())
    }

    pub(crate) fn classof_name(&self) -> String {
        format!(
            "<{} as quest_hook::libil2cpp::Type>::class()",
            self.rs_name()
        )
    }

    fn implement_reference_type(&self) -> syn::ItemImpl {
        let namespace = self.cs_name_components.namespace.as_deref().unwrap_or("");
        let class_name = self
            .cs_name_components
            .clone()
            .remove_namespace()
            .remove_generics()
            .combine_all();

        let self_item = self.rs_name_components.to_type_path_token();

        let generics = self.get_generics(0);
        let generic_names = self.get_generics_names_args(0);

        let class_fn_override = Self::class_fn_override(namespace, &class_name, generic_names);

        let feature = self.self_def_feature.as_ref().map(|f| f.to_token_stream());

        parse_quote! {
            // let quest_hook_path: syn::Path = parse_quote!(quest_hook::libil2cpp);
            // #quest_hook_path::unsafe_impl_reference_type!(in #quest_hook_path for #path_ident => #cs_namespace.#cs_name_str #generics_names);
            #feature
            unsafe impl #generics quest_hook::libil2cpp::Type for #self_item {
                type Held<'a> = ::std::option::Option<&'a mut Self>;
                type HeldRaw = *mut Self;
                const NAMESPACE: &'static str = #namespace;
                const CLASS_NAME: &'static str = #class_name;

                #class_fn_override

                fn matches_reference_argument(ty: &quest_hook::libil2cpp::Il2CppType) -> bool {
                    ty.class()
                        .is_assignable_from(<Self as quest_hook::libil2cpp::Type>::class())
                }
                fn matches_value_argument(_: &quest_hook::libil2cpp::Il2CppType) -> bool {
                    false
                }
                fn matches_reference_parameter(ty: &quest_hook::libil2cpp::Il2CppType) -> bool {
                    <Self as quest_hook::libil2cpp::Type>::class().is_assignable_from(ty.class())
                }
                fn matches_value_parameter(_: &quest_hook::libil2cpp::Il2CppType) -> bool {
                    false
                }
            }
        }
    }

    fn implement_value_type(&self) -> TokenStream {
        let namespace = self.cs_name_components.namespace.as_deref().unwrap_or("");
        let class_name = self
            .cs_name_components
            .clone()
            .remove_namespace()
            .remove_generics()
            .combine_all();

        let generics = self.get_generics(0);
        let generic_names = self.get_generics_names_args(0);

        let self_item = self.rs_name_components.to_type_path_token();

        let class_fn_override =
            Self::class_fn_override(namespace, &class_name, generic_names.clone());

        let feature = self.self_def_feature.as_ref().map(|f| f.to_token_stream());

        parse_quote! {

            // #quest_hook_path::unsafe_impl_value_type!(in #quest_hook_path for #path_ident => #cs_namespace.#cs_name_str #generic_names);
            #feature
            unsafe impl #generics quest_hook::libil2cpp::Type for #self_item {
                type Held<'a>  = Self;
                type HeldRaw = Self;
                const NAMESPACE: &'static str = #namespace;
                const CLASS_NAME: &'static str = #class_name;

                #class_fn_override

                fn matches_value_argument(ty: &quest_hook::libil2cpp::Il2CppType) -> bool {
                    !ty.is_ref()&&ty.class().is_assignable_from(<Self as quest_hook::libil2cpp::Type> ::class())
                }
                fn matches_reference_argument(ty: &quest_hook::libil2cpp::Il2CppType) -> bool {
                    ty.is_ref()&&ty.class().is_assignable_from(<Self as quest_hook::libil2cpp::Type> ::class())
                }
                fn matches_value_parameter(ty: &quest_hook::libil2cpp::Il2CppType) -> bool {
                    !ty.is_ref()&& <Self as quest_hook::libil2cpp::Type> ::class().is_assignable_from(ty.class())
                }
                fn matches_reference_parameter(ty: &quest_hook::libil2cpp::Il2CppType) -> bool {
                    ty.is_ref()&& <Self as quest_hook::libil2cpp::Type> ::class().is_assignable_from(ty.class())
                }

            }
            #feature
            unsafe impl #generics quest_hook::libil2cpp::Argument for #self_item{
                type Type = Self;
                fn matches(ty: &quest_hook::libil2cpp::Il2CppType) -> bool {
                    <Self as quest_hook::libil2cpp::Type> ::matches_value_argument(ty)
                }
                fn class() -> &'static quest_hook::libil2cpp::Il2CppClass {
                    <Self as quest_hook::libil2cpp::Type>::class()
                }
                fn invokable(&mut self) ->  *mut ::std::ffi::c_void {
                    self as *mut Self as *mut ::std::ffi::c_void
                }

            }
            #feature
            unsafe impl #generics quest_hook::libil2cpp::Parameter for #self_item{
                type Actual = Self;
                fn matches(ty: &quest_hook::libil2cpp::Il2CppType) -> bool {
                    <Self as quest_hook::libil2cpp::Type> ::matches_value_parameter(ty)
                }
                fn class() -> &'static quest_hook::libil2cpp::Il2CppClass {
                    <Self as quest_hook::libil2cpp::Type>::class()
                }
                fn from_actual(actual: <Self as quest_hook::libil2cpp::Parameter>::Actual) -> Self {
                    actual
                }
                fn into_actual(self) -> <Self as quest_hook::libil2cpp::Parameter>::Actual {
                    self
                }

            }
            #feature
            unsafe impl #generics quest_hook::libil2cpp::Returned for #self_item{
                type Type = Self;
                fn matches(ty: &quest_hook::libil2cpp::Il2CppType) -> bool {
                    <Self as quest_hook::libil2cpp::Type> ::matches_returned(ty)
                }
                fn from_object(object:Option< &mut quest_hook::libil2cpp::Il2CppObject>) -> Self {
                    unsafe {
                        quest_hook::libil2cpp::raw::unbox(quest_hook::libil2cpp::WrapRaw::raw(object.unwrap()))
                    }
                }

            }
            #feature
            unsafe impl #generics quest_hook::libil2cpp::Return for #self_item {
                type Actual = Self;
                fn matches(ty: &quest_hook::libil2cpp::Il2CppType) -> bool {
                    <Self as quest_hook::libil2cpp::Type> ::matches_return(ty)
                }
                fn into_actual(self) -> <Self as quest_hook::libil2cpp::Return>::Actual {
                    self
                }
                fn from_actual(actual: <Self as quest_hook::libil2cpp::Return>::Actual) -> Self {
                    actual
                }

            }

        }
    }
}

impl Writer {
    pub(crate) fn write_pretty_tokens(&mut self, tokens: TokenStream) -> Result<()> {
        let syntax_tree = syn::parse2(tokens.clone()).with_context(|| format!("{tokens}"))?;
        let formatted = prettyplease::unparse(&syntax_tree);

        self.stream.write_all(formatted.as_bytes())?;
        Ok(())
    }
}

#[derive(Debug, PartialEq, PartialOrd, Hash)]
pub struct CustomArc<T>(pub Arc<T>);
impl<T> Clone for CustomArc<T> {
    fn clone(&self) -> Self {
        CustomArc(Arc::clone(&self.0))
    }
}

impl<T> From<T> for CustomArc<T> {
    fn from(t: T) -> Self {
        CustomArc(Arc::new(t))
    }
}

impl<T> Deref for CustomArc<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        self.0.as_ref()
    }
}

impl<T: ToTokens> ToTokens for CustomArc<T> {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        self.0.as_ref().to_tokens(tokens)
    }
}
