use std::collections::HashSet;

use brocolib::global_metadata::Token;
use clap::builder::Str;
use color_eyre::eyre::{Context, Result};
use itertools::Itertools;
use prettyplease::unparse;
use proc_macro2::{Span, TokenStream};
use quote::{format_ident, quote, quote_spanned, ToTokens};
use syn::{parse::Parse, parse_file, parse_quote, punctuated::Punctuated, token, Ident, Token};

use crate::{
    data::{
        name_components::NameComponents,
        type_resolver::{ResolvedType, TypeUsage},
    },
    generate::{
        cs_members::{CsField, CsMethod, CsParam},
        cs_type::CsType,
        cs_type_tag::CsTypeTag,
        metadata::CordlMetadata,
        offsets::SizeInfo,
        type_extensions::{TypeDefinitionExtensions, TypeDefinitionIndexExtensions},
        writer::{Writable, Writer},
    },
};

use super::{
    config::RustGenerationConfig,
    rust_fields,
    rust_members::{RustField, RustFunction, RustItem, RustParam, RustTrait, Visibility},
    rust_name_components::RustNameComponents,
    rust_name_resolver::RustNameResolver,
};

use std::io::Write;

const PARENT_FIELD: &str = "__cordl_parent";

#[derive(Clone, Debug, Default)]
pub struct RustTypeRequirements {
    required_modules: HashSet<String>,
}

impl RustTypeRequirements {
    pub fn add_module(&mut self, module: &str) {
        self.required_modules.insert(module.to_string());
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
}

#[derive(Clone, Debug)]
pub struct RustType {
    pub fields: Vec<RustField>,
    pub methods: Vec<RustFunction>,
    pub traits: Vec<RustTrait>,

    pub is_value_type: bool,
    pub is_enum_type: bool,
    pub is_reference_type: bool,
    pub is_interface: bool,

    pub self_tag: CsTypeTag,

    pub parent: Option<RustNameComponents>,

    pub generics: Option<Vec<String>>,
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

        let rs_name_components = RustNameComponents {
            generics: cs_name_components.generics.clone(),
            name: config.name_rs(&cs_name_components.name),
            namespace: cs_name_components
                .namespace
                .as_ref()
                .map(|s| config.namespace_rs(s)),
            is_ref: false,
            is_ptr: cs_type.is_reference_type,
            is_mut: cs_type.is_reference_type, // value types don't need to be mutable
        };

        RustType {
            fields: Default::default(),
            methods: Default::default(),
            traits: Default::default(),

            is_value_type: cs_type.is_value_type,
            is_enum_type: cs_type.is_enum_type,
            is_reference_type: cs_type.is_reference_type,
            is_interface: cs_type.is_interface,
            parent: Default::default(),

            requirements: RustTypeRequirements::default(),
            self_tag: tag,
            generics: cs_type
                .generic_template
                .as_ref()
                .map(|g| g.just_names().cloned().collect_vec()),

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
        self.make_parent(cs_type.parent.as_ref(), name_resolver);

        self.make_fields(&cs_type.fields, name_resolver, config);
        self.make_methods(&cs_type.methods, name_resolver, config);
    }

    fn make_parent(
        &mut self,
        parent: Option<&ResolvedType>,
        name_resolver: &RustNameResolver<'_, '_>,
    ) {
        let Some(parent) = parent else { return };
        let parent = name_resolver.resolve_name(self, parent, TypeUsage::TypeName, true);
        let parent_field = RustField {
            name: PARENT_FIELD.to_string(),
            field_type: parent.to_combined_ident(),
            visibility: Visibility::Private,
            offset: 0,
        };

        self.fields.insert(0, parent_field);
        self.parent = Some(parent);
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

        if self.is_value_type || self.is_enum_type {
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
    fn make_methods(
        &mut self,
        methods: &[CsMethod],
        name_resolver: &RustNameResolver,
        config: &RustGenerationConfig,
    ) {
        for m in methods {
            let m_ret_ty = name_resolver.resolve_name(self, &m.return_type, TypeUsage::Field, true);

            let params = m
                .parameters
                .iter()
                .map(|p| self.make_parameter(p, name_resolver, config))
                .collect_vec();

            let rust_func = RustFunction {
                name: config.name_rs(&m.name),
                body: None,

                is_mut: true,
                is_ref: true,
                is_self: m.instance,
                params,

                return_type: Some(m_ret_ty.combine_all()),
                visibility: Some(Visibility::Public),
            };
            self.methods.push(rust_func);
        }
    }

    fn make_parameter(
        &mut self,
        p: &CsParam,
        name_resolver: &RustNameResolver<'_, '_>,
        config: &RustGenerationConfig,
    ) -> RustParam {
        let p_ty = name_resolver.resolve_name(self, &p.il2cpp_ty, TypeUsage::Field, true);
        // let p_il2cpp_ty = p.il2cpp_ty.get_type(name_resolver.cordl_metadata);

        let name_rs = config.name_rs(&p.name);
        RustParam {
            name: name_rs,
            param_type: p_ty.combine_all(),
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
        cs_type: &CsType,
        metadata: &CordlMetadata,
        config: &RustGenerationConfig,
    ) {
        // Nested type unnesting fix
        let Some(declaring_tag) = cs_type.declaring_ty.as_ref() else {
            return;
        };

        let declaring_td = declaring_tag
            .get_tdi()
            .get_type_definition(metadata.metadata);

        let declaring_name = declaring_td.get_name_components(metadata.metadata).name;
        let declaring_namespace = declaring_td.namespace(metadata.metadata);

        let combined_name = format!("{}_{}", declaring_name, self.name());

        self.rs_name_components.namespace = Some(config.namespace_rs(declaring_namespace));
        self.rs_name_components.name = config.sanitize_to_rs_name(&combined_name);
    }

    fn write_reference_type(
        &self,
        writer: &mut Writer,
        config: &RustGenerationConfig,
    ) -> Result<()> {
        let name_ident = self
            .rs_name_components
            .clone()
            .with_no_prefix()
            .remove_namespace()
            .to_combined_ident();

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

        let cs_name_ident = self.cs_name_components.to_combined_ident();

        // let mut tokens = if let Some(generics) = &self.generics {
        //     let generics_ident = generics
        //         .iter()
        //         .map(|g| format_ident!("{}", g))
        //         .collect_vec();

        //     quote! {
        //         #[repr(c)]
        //         #[derive(Debug)]
        //         pub struct #name_ident<#(#generics_ident),*> {
        //             #(#fields),*
        //         }
        //         quest_hook::libil2cpp::unsafe_impl_reference_type!(in quest_hook::libil2cpp for #name_ident<#(#generics_ident),*> => #cs_name_ident);
        //     }
        // } else {
        //     quote! {
        //         #[repr(c)]
        //         #[derive(Debug)]
        //         pub struct #name_ident {
        //             #(#fields),*
        //         }
        //         quest_hook::libil2cpp::unsafe_impl_reference_type!(in quest_hook::libil2cpp for #name_ident => #cs_name_ident);
        //     }
        // };
        let mut tokens = quote! {
            #[repr(c)]
            #[derive(Debug)]
            pub struct #name_ident {
                #(#fields),*
            }
            quest_hook::libil2cpp::unsafe_impl_reference_type!(in quest_hook::libil2cpp for #name_ident => #cs_name_ident);
        };

        // example of using the il2cpp_subtype macro
        // il2cpp_subtype!(List, Il2CppObject, object);
        // macro_rules! il2cpp_subtype {
        //     ($type:ident, $target:ty, $field:ident) => {
        //         impl<T: Type> std::ops::Deref for $type<T> {
        //             type Target = $target;

        //             fn deref(&self) -> &Self::Target {
        //                 &self.$field
        //             }
        //         }

        //         impl<T: Type> std::ops::DerefMut for $type<T> {
        //             fn deref_mut(&mut self) -> &mut Self::Target {
        //                 &mut self.$field
        //             }
        //         }
        //     };
        // }
        // il2cpp_subtype!(List<T>, Il2CppObject, object);
        if let Some(parent) = &self.parent {
            let parent_name = parent.to_combined_ident();
            let parent_field_ident = format_ident!(r#"{}"#, PARENT_FIELD);

            tokens.extend(quote! {
                    quest_hook::libil2cpp::il2cpp_subtype!(#name_ident => #parent_name, {#parent_field_ident});
                });

            // if let Some(generics) = &self.generics {
            //     let generics_ident = generics
            //         .iter()
            //         .map(|g| format_ident!("{}", g))
            //         .collect_vec();

            //     tokens.extend(quote! {
            //     quest_hook::libil2cpp::il2cpp_subtype!(#name_ident<#(#generics_ident),*> => #parent_name_ident, {#parent_field_ident});
            // });
            // } else {
            //     tokens.extend(quote! {
            //         quest_hook::libil2cpp::il2cpp_subtype!(#name_ident => #parent_name_ident, {#parent_field_ident});
            //     });
            // }
        }

        // Parse the string into a `syn::File` AST
        // let syntax_tree = parse_file(&tokens.to_string()).context("Failed to parse code")?;

        // // Pretty-print the syntax tree
        // let formatted_code = unparse(&syntax_tree);

        // TODO: prettyplease
        writeln!(writer, "{tokens}")?;

        self.write_impl(writer, config)?;
        Ok(())
    }

    fn write_enum_type(&self, writer: &mut Writer, config: &RustGenerationConfig) -> Result<()> {
        writeln!(writer, "#[repr(c)]")?;
        writeln!(writer, "#[derive(Debug, Clone)]")?;
        writeln!(writer, "pub struct {name} {{", name = self.rs_name())?;
        for f in &self.fields {
            f.write(writer)?;
        }
        writeln!(writer, "}}")?;

        writeln!(writer, "quest_hook::libil2cpp::unsafe_impl_value_type!(in quest_hook::libil2cpp for {} => {});",
         self.rs_name(),
         self.cs_name_components.combine_all()
        )?;

        self.write_impl(writer, config)?;

        Ok(())
    }

    fn write_value_type(&self, writer: &mut Writer, config: &RustGenerationConfig) -> Result<()> {
        let name = match &self.generics {
            Some(generics) => format!("{}<{}>", self.rs_name(), generics.join(", ")),
            None => self.rs_name().clone(),
        };

        writeln!(writer, "#[repr(c)]")?;
        writeln!(writer, "#[derive(Debug, Clone)]")?;
        writeln!(writer, "pub struct {name} {{")?;
        for f in &self.fields {
            f.write(writer)?;
        }
        writeln!(writer, "}}")?;

        writeln!(writer, "quest_hook::libil2cpp::unsafe_impl_value_type!(in quest_hook::libil2cpp for {} => {});",
         self.rs_name(),
         self.cs_name_components.combine_all()
        )?;

        self.write_impl(writer, config)?;

        Ok(())
    }

    fn write_impl(&self, writer: &mut Writer, _config: &RustGenerationConfig) -> Result<()> {
        let name = self.rs_name();

        match &self.generics {
            Some(generics) => {
                let generics_combined = generics.join(", ");
                writeln!(
                    writer,
                    "impl<{generics_combined}> {name}<{generics_combined}> {{"
                )?;
            }
            None => {
                writeln!(writer, "impl {name} {{")?;
            }
        };

        for m in &self.methods {
            m.write(writer)?;
        }

        writeln!(writer, "}}")?;

        Ok(())
    }

    fn write_interface(&self, writer: &mut Writer, _config: &RustGenerationConfig) -> Result<()> {
        let name = match &self.generics {
            Some(generics) => format!("{}<{}>", self.rs_name(), generics.join(", ")),
            None => self.rs_name().clone(),
        };

        writeln!(writer, "pub trait {name} {{")?;
        for mut m in self.methods.clone() {
            // traits don't like visibility modifiers
            m.visibility = None;
            m.write(writer)?;
        }
        writeln!(writer, "}}")?;
        writeln!(writer, "quest_hook::libil2cpp::unsafe_impl_reference_type!(in quest_hook::libil2cpp for {} => {});",
         self.rs_name(),
         self.cs_name_components.combine_all()
        )?;

        Ok(())
    }

    pub(crate) fn classof_name(&self) -> String {
        format!("{}::class()", self.rs_name())
    }
}

impl NameComponents {
    pub fn to_combined_ident(&self) -> TokenStream {
        let mut completed = match self.name.split_once('`') {
            Some((a, b)) => {
                let ident_a = format_ident!(r#"{}"#, a);
                let ident_b: syn::Lit = syn::parse_str(b).expect("Failed to parse number");

                quote! {
                    #ident_a ^ #ident_b
                }
            }
            None => format_ident!(r#"{}"#, self.name).to_token_stream(),
        };

        // add declaring types
        if let Some(declaring_types) = self.declaring_types.as_ref() {
            let declaring_types = declaring_types.iter().map(|g| format_ident!(r#"{}"#, g));

            completed = quote! {
                #(#declaring_types)/ * / #completed
            }
        }

        // add namespace
        if let Some(namespace_str) = self.namespace.as_ref() {
            let namespace: syn::punctuated::Punctuated<Ident, Token![.]> =
                syn::parse_str(&namespace_str).expect("Failed to parse namespace");
                
            completed = quote! {
                #namespace.#completed
            }
        }

        // add generics
        if let Some(generics_strings) = &self.generics {
            let generics: Vec<syn::GenericArgument> = generics_strings
                .iter()
                .map(|g| syn::parse_str(g).expect("Failed to parse generic"))
                .collect();

            completed = quote! {
                #completed <#(#generics),*>
            }
        }

        completed
    }
}

impl RustNameComponents {
    pub fn to_name_ident(&self) -> TokenStream {
        match self.generics {
            Some(ref generics) => {
                let generics = generics.iter().map(|g| format_ident!(r#"{}"#, g));

                let name = format_ident!(r#"{}"#, self.name);

                quote! {
                    #name <#(#generics),*>
                }
                .to_token_stream()
            }
            None => format_ident!(r#"{}"#, self.combine_all()).to_token_stream(),
        }
    }

    pub fn to_combined_ident(&self) -> TokenStream {
        let mut completed = format_ident!(r#"{}"#, self.name).to_token_stream();

        // add namespace
        if let Some(namespace) = self.namespace.as_ref() {
            let namespace_ident: syn::Path =
                syn::parse_str(namespace).expect("Failed to parse namespace");
            completed = quote! {
                #namespace_ident::#completed
            }
        }

        // add generics
        if let Some(generics) = &self.generics {
            let generics: Vec<syn::GenericArgument> = generics
                .iter()
                .map(|g| syn::parse_str(g).expect("Unable to parse generic argument"))
                .collect();

            completed = quote! {
            #completed<#(#generics),*>
            }
        }

        // add & or * or mut

        let mut prefix = if self.is_ref {
            quote! { & }
        } else if self.is_ptr {
            quote! { * }
        } else {
            quote! {}
        };

        if self.is_mut {
            prefix = quote! { #prefix mut  };
        }

        quote! {
            #prefix #completed
        }
    }
}
