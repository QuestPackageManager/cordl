use std::collections::HashSet;

use color_eyre::eyre::{Context, ContextCompat, Result};
use itertools::Itertools;
use proc_macro2::TokenStream;
use quote::{format_ident, quote, ToTokens};
use syn::parse_quote;

use crate::{
    data::{
        name_components::NameComponents,
        type_resolver::{ResolvedType, TypeUsage},
    },
    generate::{
        cs_members::{CsConstructor, CsField, CsMethod, CsParam},
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

#[derive(Clone, Debug, Default)]
pub struct RustTypeRequirements {
    required_modules: HashSet<String>,
    required_types: HashSet<CsTypeTag>,
}

impl RustTypeRequirements {
    pub fn add_module(&mut self, module: &str) {
        self.required_modules.insert(module.to_string());
    }
    pub fn add_dependency(&mut self, cs_type_tag: CsTypeTag) {
        self.required_types.insert(cs_type_tag);
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
    pub fn get_dependencies(&self) -> &HashSet<CsTypeTag> {
        &self.required_types
    }
}

#[derive(Clone)]
pub struct RustType {
    // TODO: union
    pub fields: Vec<RustField>,
    pub constants: Vec<ConstRustField>,
    pub methods: Vec<RustFunction>,
    pub traits: Vec<RustTraitImpl>,
    pub nested_types: Vec<syn::ItemType>,

    pub is_value_type: bool,
    pub is_enum_type: bool,
    pub is_reference_type: bool,
    pub is_interface: bool,

    pub self_tag: CsTypeTag,
    pub self_feature: Option<RustFeature>,

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

        let generics = cs_type.generic_template.as_ref().map(|g| {
            g.names
                .iter()
                .map(|(ty, s)| RustGeneric {
                    name: s.to_string(),
                    bounds: vec!["quest_hook::libil2cpp::Type".to_string()],
                })
                .collect_vec()
        });

        let rs_name_components = RustNameComponents {
            generics: generics,
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

        let feature_name =
            config.feature_name(&cs_name_components.clone().remove_generics().combine_all());

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
            self_feature: Some(RustFeature { name: feature_name }),
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
            self.methods.push(RustFunction {
                name: format_ident!("from_object_mut"),
                body: Some(parse_quote! {
                    unsafe{ (object_param as *mut Self) }
                }),
                generics: Default::default(),
                is_mut: false,
                is_ref: false,
                is_self: false,
                where_clause: None,
                params: vec![RustParam {
                    name: format_ident!("object_param"),
                    param_type: parse_quote!(*mut quest_hook::libil2cpp::Il2CppObject),
                }],
                return_type: Some(parse_quote!(*mut Self)),
                visibility: Visibility::Public,
            });
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
            .resolve_name(self, parent, TypeUsage::TypeName, true)
            .with_no_prefix();
        let parent_field = RustField {
            name: format_ident!("{}", PARENT_FIELD),
            field_type: parent.to_type_token(),
            visibility: Visibility::Private,
            offset: 0,
        };

        self.fields.insert(0, parent_field);
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

        self.fields.insert(0, parent_field);
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

                let feature = rust_type.self_feature.as_ref().map(|f| {
                    let name = &f.name;
                    quote! {
                        #[cfg(feature = #name)]
                    }
                });

                parse_quote! {
                    #feature
                    #visibility type #name_ident #target_generics = #target;
                }
            });

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

            self.fields.push(RustField {
                name: format_ident!("__cordl_phantom_{name}"),
                field_type: parse_quote!(std::marker::PhantomData<#name>),
                visibility: Visibility::Private,
                offset: 0,
            });
        }
    }

    fn make_interfaces(
        &mut self,
        interfaces: &[ResolvedType],
        name_resolver: &RustNameResolver,
        config: &RustGenerationConfig,
    ) {
        // TODO: Implement AsMut
        for i in interfaces {
            let self_ident = self.rs_name_components.to_type_path_token();

            let generics = self.get_generics(0);

            let interface = name_resolver.resolve_name(self, i, TypeUsage::TypeName, true);
            let interface_ident = interface.to_type_path_token();

            let impl_data: Vec<syn::Stmt> = match self.is_reference_type {
                true => parse_quote! {
                    unsafe { std::mem::transmute(self) }
                },
                false => parse_quote! {
                    // TODO: implement for value types
                    todo!()
                },
            };
            let as_ref = RustTraitImpl {
                name: interface.combine_all(),
                impl_data: parse_quote! {
                    impl #generics AsRef<#interface_ident> for #self_ident {
                        fn as_ref(&self) -> & #interface_ident {
                            #(#impl_data)*
                        }
                    }
                },
            };
            let as_mut = RustTraitImpl {
                name: interface.combine_all(),
                impl_data: parse_quote! {
                    impl #generics AsMut<#interface_ident> for #self_ident {
                        fn as_mut(&mut self) -> &mut #interface_ident {
                            #(#impl_data)*
                        }
                    }
                },
            };
            self.traits.push(as_ref);
            self.traits.push(as_mut);
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

            let param_names = params.iter().map(|p| &p.name);

            let body: Vec<syn::Stmt> = parse_quote! {
                let __cordl_object: &mut Self = <Self as quest_hook::libil2cpp::Type>::class().instantiate();

                quest_hook::libil2cpp::ObjectType::as_object_mut(__cordl_object).invoke_void(".ctor", (#(#param_names),*))?;

                Ok(__cordl_object.into())
            };
            let generics = c
                .template
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

            let combined_generics = self
                .rs_name_components
                .generics
                .clone()
                .unwrap_or_default()
                .into_iter()
                .chain(generics.clone().into_iter())
                .map(|mut g| {
                    // TODO: Add these bounds on demand
                    let bounds = vec![
                        "quest_hook::libil2cpp::Type".to_string(),
                        "quest_hook::libil2cpp::Argument".to_owned(),
                        "quest_hook::libil2cpp::Returned".to_owned(),
                    ];

                    g.bounds.extend(bounds);
                    g
                })
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

                return_type: Some(parse_quote!(
                    quest_hook::libil2cpp::Result<quest_hook::libil2cpp::Gc<Self>>
                )),
                visibility: (Visibility::Public),
            };
            self.methods.push(rust_func);
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
                            .resolve_name(self, &p.il2cpp_ty, TypeUsage::Parameter, true)
                            .name
                    })
                    .map(|s| config.name_rs(&s))
                    .collect::<Vec<_>>()
            })
            .collect();

        let current_param_types = m_params
            .iter()
            .map(|p| {
                name_resolver
                    .resolve_name(self, &p.il2cpp_ty, TypeUsage::Parameter, true)
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
                    .resolve_name(self, &m.return_type, TypeUsage::ReturnType, true)
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

                let body = self.make_method_body(m, m_name, param_names, m_ret_ty_ident);

                let generics = m
                    .template
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

                let combined_generics = self
                    .rs_name_components
                    .generics
                    .clone()
                    .unwrap_or_default()
                    .into_iter()
                    .chain(generics.clone().into_iter())
                    .map(|mut g| {
                        // TODO: Add these bounds on demand
                        let bounds = vec![
                            "quest_hook::libil2cpp::Type".to_string(),
                            "quest_hook::libil2cpp::Argument".to_owned(),
                            "quest_hook::libil2cpp::Returned".to_owned(),
                        ];

                        g.bounds.extend(bounds);
                        g
                    })
                    .map(|g| -> syn::GenericParam { g.to_token_stream() })
                    .collect_vec();

                let where_clause: syn::WhereClause = parse_quote! {
                    where #(#combined_generics),*
                };

                let rust_func = RustFunction {
                    name: format_ident!("{m_name_rs}"),
                    body: Some(body),
                    generics,
                    is_mut: m.instance,
                    is_ref: m.instance,
                    is_self: m.instance,
                    params,
                    where_clause: Some(where_clause),

                    return_type: Some(m_result_ty),
                    visibility: (Visibility::Public),
                };
                self.methods.push(rust_func);
            }
        }
    }

    fn make_method_body<'a>(
        &self,
        m: &CsMethod,
        m_name: &String,
        param_names: impl Iterator<Item = &'a syn::Ident>,
        m_ret_ty: syn::Type,
    ) -> Vec<syn::Stmt> {
        let is_value_type = self.is_value_type || self.is_enum_type;

        let invoke_call: Vec<syn::Stmt> = match (m.instance, is_value_type) {
            // instance, value type
            (true, true) => parse_quote! {

                let __cordl_ret: #m_ret_ty = quest_hook::libil2cpp::ValueTypeExt::invoke(self, #m_name, ( #(#param_names),* ))?;

                Ok(__cordl_ret.into())
            },
            // instance, ref type
            (true, false) => parse_quote! {
                let __cordl_object: &mut quest_hook::libil2cpp::Il2CppObject = quest_hook::libil2cpp::ObjectType::as_object_mut(self);

                let __cordl_ret: #m_ret_ty = __cordl_object.invoke(#m_name, ( #(#param_names),* ))?;

                Ok(__cordl_ret.into())
            },
            // static
            (false, _) => parse_quote! {
                let __cordl_ret: #m_ret_ty = <Self as quest_hook::libil2cpp::Type>::class().invoke(#m_name, ( #(#param_names),* ) )?;

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
            .resolve_name(self, &p.il2cpp_ty, TypeUsage::Parameter, true)
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

        let feature = self.self_feature.as_ref().map(|f| {
            let name = &f.name;
            quote! {
                #[cfg(feature = #name)]
            }
        });

        let mut tokens = quote! {
            #feature
            #[repr(C)]
            #[derive(Debug)]
            pub struct #name_ident {
                #(#fields),*
            }

            #impl_ref

        };

        if let Some(parent) = &self.parent {
            let parent_name = parent.clone().to_type_path_token();
            let parent_field_ident = format_ident!(r#"{}"#, PARENT_FIELD);

            tokens.extend(quote! {
            #feature
            impl #generics std::ops::Deref for #path_ident {
                type Target = #parent_name;

                fn deref(&self) -> &Self::Target {
                    unsafe {&self.#parent_field_ident}
                }
            }

            #feature
            impl #generics std::ops::DerefMut for #path_ident {
                fn deref_mut(&mut self) -> &mut Self::Target {
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

                // add default for enum
                if i == 0 {
                    return parse_quote! {
                        #[default]
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

        let feature = self.self_feature.as_ref().map(|f| {
            let name = &f.name;
            quote! {
                #[cfg(feature = #name)]
            }
        });

        let tokens = quote! {
            #feature
            #[repr(#backing_type)]
            #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
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

        let feature = self.self_feature.as_ref().map(|f| {
            let name = &f.name;
            quote! {
                #[cfg(feature = #name)]
            }
        });

        let tokens = quote! {
            #feature
            #[repr(C)]
            #[derive(Debug, Clone, Default, PartialEq)]
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
            .map(|mut f| {
                f.body = f.body.or(Some(parse_quote! {
                    todo!()
                }));
                f
            })
            .map(|f| f.to_token_stream())
            .map(|f| -> syn::ImplItemFn { parse_quote!(#f) });

        let feature = self.self_feature.as_ref().map(|f| {
            let name = &f.name;
            quote! {
                #[cfg(feature = #name)]
            }
        });

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
                    #feature
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

        let impl_object_tokens: Option<syn::ItemImpl> = self.parent.as_ref().map(|_| -> syn::ItemImpl {
            let parent_field_ident = format_ident!(r#"{}"#, PARENT_FIELD);

            parse_quote! {
                #feature
                impl #generics quest_hook::libil2cpp::ObjectType for #path_ident {
                    fn as_object(&self) -> &quest_hook::libil2cpp::Il2CppObject {
                        quest_hook::libil2cpp::ObjectType::as_object(&self.#parent_field_ident)
                    }

                    fn as_object_mut(&mut self) -> &mut quest_hook::libil2cpp::Il2CppObject {
                        quest_hook::libil2cpp::ObjectType::as_object_mut(&mut self.#parent_field_ident)
                    }
                }
            }
        });

        let tokens = quote! {
            #feature
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

        let feature = self.self_feature.as_ref().map(|f| {
            let name = &f.name;
            quote! {
                #[cfg(feature = #name)]
            }
        });

        let mut tokens = quote! {
            #feature
            #[repr(C)]
            #[derive(Debug)]
            pub struct #name_ident {
                #(#fields),*
            }

            #impl_ref

        };

        if let Some(parent) = &self.parent {
            let parent_name = parent.clone().to_type_path_token();
            let parent_field_ident = format_ident!(r#"{}"#, PARENT_FIELD);

            tokens.extend(quote! {
            #feature
            impl #generics std::ops::Deref for #path_ident {
                type Target = #parent_name;

                fn deref(&self) -> &Self::Target {
                    unsafe {&self.#parent_field_ident}
                }
            }

            #feature
            impl #generics std::ops::DerefMut for #path_ident {
                fn deref_mut(&mut self) -> &mut Self::Target {
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
        let class_name = &self.cs_name_components.name;

        let self_item = self.rs_name_components.to_type_path_token();

        let generics = self.get_generics(0);
        let generic_names = self.get_generics_names_args(0);

        let class_fn_override: Option<syn::ImplItemFn> = generic_names.map(|names| {
            parse_quote! {
                fn class() ->  &'static quest_hook::libil2cpp::Il2CppClass {
                    static CLASS: ::std::sync::OnceLock< &'static quest_hook::libil2cpp::Il2CppClass>  =  ::std::sync::OnceLock::new();
                    CLASS.get_or_init(||{
                        quest_hook::libil2cpp::Il2CppClass::find(#namespace, #class_name).unwrap().make_generic:: <(#(#names),*)> ().unwrap().unwrap()
                    })
                }
        }});

        let feature = self.self_feature.as_ref().map(|f| {
            let name = &f.name;
            quote! {
                #[cfg(feature = #name)]
            }
        });

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
        let class_name = &self.cs_name_components.name;

        let generics = self.get_generics(0);
        let generic_names = self.get_generics_names_args(0);

        let self_item = self.rs_name_components.to_type_path_token();

        let class_fn_override: Option<syn::ImplItemFn> = generic_names.clone().map(|names| {
            parse_quote! {
                fn class() ->  &'static quest_hook::libil2cpp::Il2CppClass {
                    static CLASS: ::std::sync::OnceLock< &'static quest_hook::libil2cpp::Il2CppClass>  =  ::std::sync::OnceLock::new();
                    CLASS.get_or_init(||{
                        quest_hook::libil2cpp::Il2CppClass::find(#namespace, #class_name).unwrap().make_generic:: <(#(#names),*)> ().unwrap().unwrap()
                    })
                }
        }});

        let feature = self.self_feature.as_ref().map(|f| {
            let name = &f.name;
            quote! {
                #[cfg(feature = #name)]
            }
        });

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
                fn from_actual(actual:Self::Actual) -> Self {
                    actual
                }
                fn into_actual(self) -> Self::Actual {
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
                fn into_actual(self) -> Self::Actual {
                    self
                }
                fn from_actual(actual:Self::Actual) -> Self {
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
