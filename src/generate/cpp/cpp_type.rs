use std::{
    collections::{HashMap, HashSet},
    rc::Rc,
    sync::Arc,
};

use brocolib::{
    global_metadata::{
        FieldIndex, Il2CppTypeDefinition, MethodIndex, ParameterIndex, TypeDefinitionIndex,
        TypeIndex,
    },
    runtime_metadata::{Il2CppType, Il2CppTypeEnum, TypeData},
};
use clap::builder::Str;
use color_eyre::eyre::Context;
use itertools::Itertools;
use log::{info, warn};
use std::io::Write;

use crate::{
    data::{
        name_components::NameComponents,
        type_resolver::{ResolvedType, TypeUsage},
    },
    generate::{
        cpp::cpp_members::{CppMethodSizeStruct, CppStaticAssert},
        cs_members::{
            CsConstructor, CsField, CsGenericTemplateType, CsMethod, CsParam, CsParamFlags,
            CsProperty,
        },
        cs_type::CsType,
        cs_type_tag::CsTypeTag,
        metadata::CordlMetadata,
        offsets::{self, SizeInfo},
        type_extensions::{
            MethodDefintionExtensions, ParameterDefinitionExtensions, TypeDefinitionExtensions,
            TypeDefinitionIndexExtensions, TypeExtentions,
        },
        writer::{CppWritable, CppWriter, Sortable},
    },
};

use super::{
    config::CppGenerationConfig,
    cpp_context_collection::CppContextCollection,
    cpp_members::{
        CppConstructorDecl, CppConstructorImpl, CppFieldDecl, CppForwardDeclare, CppInclude,
        CppLine, CppMember, CppMethodData, CppMethodDecl, CppMethodImpl, CppNestedStruct,
        CppNonMember, CppParam, CppPropertyDecl, CppTemplate, CppUsingAlias,
    },
    cpp_name_resolver::{CppNameResolver, VALUE_WRAPPER_TYPE},
};

pub const CORDL_TYPE_MACRO: &str = "CORDL_TYPE";
pub const __CORDL_IS_VALUE_TYPE: &str = "__IL2CPP_IS_VALUE_TYPE";
pub const __CORDL_BACKING_ENUM_TYPE: &str = "__CORDL_BACKING_ENUM_TYPE";

pub const CORDL_REFERENCE_TYPE_CONSTRAINT: &str = "::il2cpp_utils::il2cpp_reference_type";
pub const CORDL_NUM_ENUM_TYPE_CONSTRAINT: &str = "::cordl_internals::is_or_is_backed_by";
pub const CORDL_METHOD_HELPER_NAMESPACE: &str = "::cordl_internals";

// negative
pub const VALUE_TYPE_SIZE_OFFSET: u32 = 0x10;

pub const VALUE_TYPE_WRAPPER_SIZE: &str = "__IL2CPP_VALUE_TYPE_SIZE";
pub const REFERENCE_TYPE_WRAPPER_SIZE: &str = "__IL2CPP_REFERENCE_TYPE_SIZE";
pub const REFERENCE_TYPE_FIELD_SIZE: &str = "__fields";
pub const REFERENCE_WRAPPER_INSTANCE_NAME: &str = "::bs_hook::Il2CppWrapperType::instance";

pub const CORDL_NO_INCLUDE_IMPL_DEFINE: &str = "CORDL_NO_IMPL_INCLUDE";
pub const CORDL_ACCESSOR_FIELD_PREFIX: &str = "___";

pub const ENUM_PTR_TYPE: &str = "::bs_hook::EnumPtr";
pub const VT_PTR_TYPE: &str = "::bs_hook::VTPtr";

const SIZEOF_IL2CPP_OBJECT: u32 = 0x10;

#[derive(Debug, Clone, Default)]
pub struct CppTypeRequirements {
    pub forward_declares: HashSet<(CppForwardDeclare, CppInclude)>,

    // Only value types or classes
    pub required_def_includes: HashSet<CppInclude>,
    pub required_impl_includes: HashSet<CppInclude>,

    // Lists both types we forward declare or include
    pub depending_types: HashSet<CsTypeTag>,
}

impl CppTypeRequirements {
    pub fn add_forward_declare(&mut self, cpp_data: (CppForwardDeclare, CppInclude)) {
        // self.depending_types.insert(cpp_type.self_tag);
        self.forward_declares.insert(cpp_data);
    }

    pub fn add_def_include(&mut self, cpp_type: Option<&CppType>, cpp_include: CppInclude) {
        if let Some(cpp_type) = cpp_type {
            self.depending_types.insert(cpp_type.self_tag);
        }
        self.required_def_includes.insert(cpp_include);
    }
    pub fn add_impl_include(&mut self, cpp_type: Option<&CppType>, cpp_include: CppInclude) {
        if let Some(cpp_type) = cpp_type {
            self.depending_types.insert(cpp_type.self_tag);
        }
        self.required_impl_includes.insert(cpp_include);
    }
    pub fn add_dependency(&mut self, cpp_type: &CppType) {
        self.depending_types.insert(cpp_type.self_tag);
    }
    pub fn add_dependency_tag(&mut self, tag: CsTypeTag) {
        self.depending_types.insert(tag);
    }

    pub fn need_wrapper(&mut self) {
        self.add_def_include(
            None,
            CppInclude::new_exact("beatsaber-hook/shared/utils/base-wrapper-type.hpp"),
        );
    }
    pub fn needs_int_include(&mut self) {
        self.add_def_include(None, CppInclude::new_system("cstdint"));
    }
    pub fn needs_byte_include(&mut self) {
        self.add_def_include(None, CppInclude::new_system("cstddef"));
    }
    pub fn needs_math_include(&mut self) {
        self.add_def_include(None, CppInclude::new_system("cmath"));
    }
    pub fn needs_stringw_include(&mut self) {
        self.add_def_include(
            None,
            CppInclude::new_exact("beatsaber-hook/shared/utils/typedefs-string.hpp"),
        );
    }
    pub fn needs_arrayw_include(&mut self) {
        self.add_def_include(
            None,
            CppInclude::new_exact("beatsaber-hook/shared/utils/typedefs-array.hpp"),
        );
    }

    pub fn needs_byref_include(&mut self) {
        self.add_def_include(
            None,
            CppInclude::new_exact("beatsaber-hook/shared/utils/byref.hpp"),
        );
    }

    pub fn needs_enum_include(&mut self) {
        self.add_def_include(
            None,
            CppInclude::new_exact("beatsaber-hook/shared/utils/enum-type.hpp"),
        );
    }

    pub fn needs_value_include(&mut self) {
        self.add_def_include(
            None,
            CppInclude::new_exact("beatsaber-hook/shared/utils/value-type.hpp"),
        );
    }
}

#[derive(Clone, Debug)]
pub struct CppType {
    pub declarations: Vec<Arc<CppMember>>,
    pub nonmember_declarations: Vec<Arc<CppNonMember>>,
    pub implementations: Vec<Arc<CppMember>>,
    pub nonmember_implementations: Vec<Arc<CppNonMember>>,

    pub parent: Option<String>,
    pub interfaces: Vec<String>,

    pub is_value_type: bool,
    pub is_enum_type: bool,
    pub is_reference_type: bool,

    pub requirements: CppTypeRequirements,
    pub self_tag: CsTypeTag,

    /// contains the array of generic Il2CppType indexes
    pub generic_instantiations_args_types: Option<Vec<usize>>, // GenericArg -> Instantiation Arg
    pub method_generic_instantiation_map: HashMap<MethodIndex, Vec<TypeIndex>>, // MethodIndex -> Generic Args

    pub cpp_template: Option<CppTemplate>,
    pub cs_name_components: NameComponents,
    pub cpp_name_components: NameComponents,
    pub tag: CsTypeTag,
    pub(crate) prefix_comments: Vec<String>,
    pub packing: Option<u32>,
    pub size_info: Option<SizeInfo>,
}

impl CppType {
    pub fn write_impl(&self, writer: &mut CppWriter) -> color_eyre::Result<()> {
        self.write_impl_internal(writer)
    }

    pub fn write_def(&self, writer: &mut CppWriter) -> color_eyre::Result<()> {
        self.write_def_internal(writer, Some(&self.cpp_namespace()))
    }

    pub fn write_impl_internal(&self, writer: &mut CppWriter) -> color_eyre::Result<()> {
        self.nonmember_implementations
            .iter()
            .try_for_each(|d| d.write(writer))?;

        // Write all declarations within the type here
        self.implementations
            .iter()
            .sorted_by(|a, b| a.sort_level().cmp(&b.sort_level()))
            .try_for_each(|d| d.write(writer))?;

        Ok(())
    }

    fn write_def_internal(
        &self,
        writer: &mut CppWriter,
        namespace: Option<&str>,
    ) -> color_eyre::Result<()> {
        self.prefix_comments
            .iter()
            .try_for_each(|pc| writeln!(writer, "// {pc}").context("Prefix comment"))?;

        let type_kind = match self.is_value_type {
            true => "struct",
            false => "class",
        };

        // Just forward declare
        if let Some(n) = &namespace {
            writeln!(writer, "namespace {n} {{")?;
            writer.indent();
        }

        // Write type definition
        if let Some(generic_args) = &self.cpp_template {
            writeln!(writer, "// cpp template")?;
            generic_args.write(writer)?;
        }
        writeln!(writer, "// Is value type: {}", self.is_value_type)?;

        let clazz_name = self.cpp_name_components.formatted_name(false);

        writeln!(
            writer,
            "// CS Name: {}",
            self.cs_name_components.combine_all()
        )?;

        if let Some(packing) = &self.packing {
            writeln!(writer, "#pragma pack(push, {packing})")?;
        }

        let inherits = self.get_inherits().collect_vec();
        match inherits.is_empty() {
            true => writeln!(writer, "{type_kind} {CORDL_TYPE_MACRO} {clazz_name} {{")?,
            false => writeln!(
                writer,
                "{type_kind} {CORDL_TYPE_MACRO} {clazz_name} : {} {{",
                inherits
                    .into_iter()
                    .map(|s| format!("public {s}"))
                    .join(", ")
            )?,
        }

        writer.indent();

        // add public access
        writeln!(writer, "public:")?;
        writeln!(writer, "// Declarations")?;
        // Write all declarations within the type here
        self.declarations
            .iter()
            .sorted_by(|a, b| a.as_ref().partial_cmp(b.as_ref()).unwrap())
            .sorted_by(|a, b| {
                // fields and unions need to be sorted by offset to work correctly

                let a_offset = match a.as_ref() {
                    CppMember::FieldDecl(f) => f.offset,
                    CppMember::NestedUnion(u) => Some(u.offset),
                    _ => None,
                };

                let b_offset = match b.as_ref() {
                    CppMember::FieldDecl(f) => f.offset,
                    CppMember::NestedUnion(u) => Some(u.offset),
                    _ => None,
                };

                a_offset.cmp(&b_offset)
            })
            // sort by sort level after fields have been ordered correctly
            .sorted_by(|a, b| a.sort_level().cmp(&b.sort_level()))
            .try_for_each(|d| -> color_eyre::Result<()> {
                d.write(writer)?;
                writeln!(writer)?;
                Ok(())
            })?;

        writeln!(
            writer,
            "static constexpr bool {__CORDL_IS_VALUE_TYPE} = {};",
            self.is_value_type
        )?;
        // Type complete
        writer.dedent();
        writeln!(writer, "}};")?;

        if self.packing.is_some() {
            writeln!(writer, "#pragma pack(pop)")?;
        }

        // NON MEMBER DECLARATIONS
        writeln!(writer, "// Non member Declarations")?;

        self.nonmember_declarations
            .iter()
            .try_for_each(|d| -> color_eyre::Result<()> {
                d.write(writer)?;
                writeln!(writer)?;
                Ok(())
            })?;

        // Namespace complete
        if let Some(n) = namespace {
            writer.dedent();
            writeln!(writer, "}} // namespace end def {n}")?;
        }

        // TODO: Write additional meta-info here, perhaps to ensure correct conversions?
        Ok(())
    }

    pub fn write_type_trait(&self, writer: &mut CppWriter) -> color_eyre::Result<()> {
        if self.cpp_template.is_some() {
            // generic
            // macros from bs hook
            let type_trait_macro = if self.is_enum_type || self.is_value_type {
                "MARK_GEN_VAL_T"
            } else {
                "MARK_GEN_REF_PTR_T"
            };

            writeln!(
                writer,
                "{type_trait_macro}({});",
                self.cpp_name_components
                    .clone()
                    .remove_generics()
                    .remove_pointer()
                    .combine_all()
            )?;
        } else {
            // non-generic
            // macros from bs hook
            let type_trait_macro = if self.is_enum_type || self.is_value_type {
                "MARK_VAL_T"
            } else {
                "MARK_REF_PTR_T"
            };

            writeln!(
                writer,
                "{type_trait_macro}({});",
                self.cpp_name_components.remove_pointer().combine_all()
            )?;
        }

        Ok(())
    }

    pub fn make_cpp_type(
        tag: CsTypeTag,
        cs_type: CsType,
        name_resolver: &CppNameResolver,
        config: &CppGenerationConfig,
    ) -> CppType {
        let mut cpp_type = CppType {
            declarations: vec![],
            nonmember_declarations: vec![],
            implementations: vec![],
            nonmember_implementations: vec![],
            parent: cs_type.parent.map(|t| t.to_string()),
            interfaces: cs_type.interfaces.iter().map(|i| i.to_string()).collect(),
            is_value_type: cs_type.is_value_type,
            is_enum_type: cs_type.is_enum_type,
            is_reference_type: cs_type.is_reference_type,
            requirements: CppTypeRequirements::default(),
            self_tag: tag.clone(),
            generic_instantiations_args_types: cs_type.generic_instantiations_args_types,
            method_generic_instantiation_map: cs_type.method_generic_instantiation_map,
            cpp_template: cs_type.generic_template.map(|t| t.into()),
            cs_name_components: cs_type.cs_name_components,
            cpp_name_components: NameComponents::default(), // TODO
            tag,
            prefix_comments: vec![],
            packing: cs_type.packing.map(|p| p as u32),
            size_info: cs_type.size_info,
        };

        // Fill type from CS data
        cpp_type.make_fields(cs_type.fields, name_resolver, config);
        cpp_type.make_methods(cs_type.methods, name_resolver, config);
        cpp_type.make_properties(cs_type.properties, name_resolver, config);
        cpp_type.make_constructors(cs_type.constructors, name_resolver, config);

        cpp_type.make_parent(cs_type.parent, name_resolver, config);
        cpp_type.make_interfaces(cs_type.interfaces, name_resolver, config);
        cpp_type.make_nested_types(cs_type.nested_types, name_resolver, config);

        let tdi: TypeDefinitionIndex = cs_type.self_tag.into();
        let metadata = name_resolver.metadata;
        let t = &metadata.metadata.global_metadata.type_definitions[tdi];

        // we depend on parents and generic args here
        // default ctor
        if t.is_value_type() || t.is_enum_type() {
            cpp_type.create_valuetype_constructor(metadata, name_resolver, config);
            cpp_type.create_valuetype_field_wrapper();
            if t.is_enum_type() {
                cpp_type.create_enum_wrapper(metadata, name_resolver, config);
                cpp_type.create_enum_backing_type_constant(metadata, name_resolver, config);
            }
            cpp_type.add_default_ctor(false);
        } else if t.is_interface() {
            // self.make_interface_constructors();

            cpp_type.delete_copy_ctor();
            // self.delete_default_ctor();
        } else {
            // ref type
            cpp_type.delete_move_ctor();
            cpp_type.delete_copy_ctor();
            cpp_type.add_default_ctor(true);
            // self.delete_default_ctor();
        }

        if !t.is_interface() {
            cpp_type.create_size_assert();
        }

        cpp_type.add_type_index_member();

        if !t.is_interface() {
            cpp_type.create_size_padding(metadata, name_resolver, config);
        }

        // if let Some(func) = metadata.custom_type_handler.get(&tdi) {
        //     func(self)
        // }

        cpp_type
    }

    fn make_fields(
        &mut self,
        fields: Vec<CsField>,
        name_resolver: &CppNameResolver,
        config: &CppGenerationConfig,
    ) {
        for field in fields {
            let field_ty = name_resolver
                .resolve_name(self, field.field_ty, TypeUsage::Field, true)
                .combine_all();
            let field_decl = CppFieldDecl {
                cpp_name: config.name_cpp(&field.name),
                field_ty,
                offset: field.offset,
                instance: field.instance,
                readonly: field.readonly,
                const_expr: field.is_const,
                value: field.value.as_ref().map(|v| v.to_string()),
                brief_comment: field.brief_comment.clone(),
                is_private: false,
            };

            self.declarations
                .push(CppMember::FieldDecl(field_decl).into());
        }
    }

    fn make_methods(
        &mut self,
        methods: Vec<CsMethod>,
        name_resolver: &CppNameResolver,
        config: &CppGenerationConfig,
    ) {
        // 2 because each method gets a method struct and method decl
        // a constructor will add an additional one for each
        self.declarations.reserve(2 * (methods.len() as usize + 1));
        self.implementations.reserve(methods.len() as usize + 1);
        for method in methods {
            if method.name == ".cctor" {
                // info!("Skipping {}", m_name);
                return;
            }

            let return_type = name_resolver
                .resolve_name(self, method.return_type, TypeUsage::ReturnType, true)
                .combine_all();
            let method_decl = CppMethodDecl {
                cpp_name: method.name.clone(),
                return_type,
                parameters: method
                    .parameters
                    .into_iter()
                    .map(|p| self.make_param(p, name_resolver, config))
                    .collect(),
                instance: method.instance,
                template: method.template.map(|v| v.into()),
                brief: method.brief.clone(),
                is_constexpr: false,
                is_const: false,
                is_no_except: false,
                is_virtual: false,
                is_implicit_operator: false,
                is_explicit_operator: false,
                is_inline: true,
                prefix_modifiers: vec![],
                suffix_modifiers: vec![],
                body: None,
            };

            self.declarations
                .push(CppMember::MethodDecl(method_decl).into());
        }
    }

    fn make_param(
        &mut self,
        p: CsParam,
        name_resolver: &CppNameResolver,
        config: &CppGenerationConfig,
    ) -> CppParam {
        let mut modifier = String::new();

        CppParam {
            name: config.name_cpp(&p.name),
            ty: name_resolver.resolve_name(self, p.il2cpp_ty, TypeUsage::Parameter, false),
            modifiers: "".to_string(), // TODO: Convert flags
            def_value: p.def_value.as_ref().map(|v| v),
        }
    }

    fn make_properties(
        &mut self,
        properties: Vec<CsProperty>,
        name_resolver: &CppNameResolver,
        config: &CppGenerationConfig,
    ) {
        self.declarations.reserve(properties.len());
        for prop in properties {
            if !prop.instance {
                continue;
            }

            let prop_decl = CppPropertyDecl {
                cpp_name: prop.name.clone(),
                prop_ty: name_resolver.resolve_name(self, prop.prop_ty, TypeUsage::Property, true),
                getter: prop.getter.clone(),
                setter: prop.setter.clone(),
                indexable: prop.indexable,
                brief_comment: prop.brief_comment.clone(),
                instance: prop.instance,
            };

            self.declarations
                .push(CppMember::Property(prop_decl).into());
        }
    }

    fn make_constructors(
        &mut self,
        constructors: Vec<CsConstructor>,
        name_resolver: &CppNameResolver,
        config: &CppGenerationConfig,
    ) {
        self.declarations.reserve(constructors.len());
        for ctor in constructors {
            let ctor_decl = CppConstructorDecl {
                cpp_name: ctor.cpp_name.clone(),
                parameters: ctor
                    .parameters
                    .into_iter()
                    .map(|p| self.make_param(p, name_resolver, config))
                    .collect(),
                template: ctor.template.clone().into(),
                brief: ctor.brief.clone(),
                is_constexpr: true,
                is_explicit: false,
                is_default: false,
                is_no_except: true,
                is_delete: false,
                is_protected: false,
                base_ctor: None,
                initialized_values: Default::default(),
                body: ctor.body.clone(),
            };

            self.declarations
                .push(CppMember::ConstructorDecl(ctor_decl).into());
        }
    }

    fn make_parent(
        &mut self,
        parent: Option<ResolvedType>,
        name_resolver: &CppNameResolver,
        config: &CppGenerationConfig,
    ) {
        let Some(parent) = parent else {
            return;
        };

        let parent_name = name_resolver.resolve_name(self, parent, TypeUsage::TypeName, true);
        let cordl_metadata = name_resolver.metadata;
        let parent_ty = &cordl_metadata.metadata_registration.types[parent.ty as usize];

        let parent_tag = CsTypeTag::from_type_data(parent.ty, cordl_metadata);
        let parent_tdi: TypeDefinitionIndex = parent_tag.into();
        let ctx_collection = name_resolver.collection;

        let base_type_context = ctx_collection
            .get_context(parent_name)
            .or_else(|| ctx_collection.get_context(parent_tdi.into()))
            .unwrap_or_else(|| panic!("No CppContext for base type {parent_name:?}."));

        let base_type_cpp_type = ctx_collection
            .get_cpp_type(parent_name)
            .or_else(|| ctx_collection.get_cpp_type(parent_tdi.into()))
            .unwrap_or_else(|| panic!("No CppType for base type {parent_name:?}."));

        self.requirements.add_impl_include(
            Some(base_type_cpp_type),
            CppInclude::new_context_typeimpl(base_type_context),
        );

        self.parent = Some(parent_name.remove_pointer().combine_all());
    }

    fn make_interfaces(
        &mut self,
        interfaces: Vec<ResolvedType>,
        name_resolver: &CppNameResolver,
        config: &CppGenerationConfig,
    ) {
        let self_td = self
            .self_tag
            .get_tdi()
            .get_type_definition(name_resolver.metadata);

        for &interface in interfaces {
            // We have an interface, lets do something with it
            let interface_name_il2cpp =
                name_resolver.resolve_name(self, interface, TypeUsage::TypeName, true);

            let interface_cpp_name = interface_name_il2cpp.remove_pointer().combine_all();
            let interface_cpp_pointer = interface_name_il2cpp.as_pointer().combine_all();

            let operator_method_decl = CppMethodDecl {
                body: Default::default(),
                brief: Some(format!("Convert operator to {interface_cpp_name:?}")),
                cpp_name: interface_cpp_pointer.clone(),
                return_type: "".to_string(),
                instance: true,
                is_const: false,
                is_constexpr: true,
                is_no_except: !self_td.is_value_type() && !self_td.is_enum_type(),
                is_implicit_operator: true,
                is_explicit_operator: false,

                is_virtual: false,
                is_inline: true,
                parameters: vec![],
                template: None,
                prefix_modifiers: vec![],
                suffix_modifiers: vec![],
            };
            let helper_method_decl = CppMethodDecl {
                brief: Some(format!("Convert to {interface_cpp_name:?}")),
                is_implicit_operator: false,
                return_type: interface_cpp_pointer.clone(),
                cpp_name: format!("i_{}", config.sanitize_to_cpp_name(&interface_cpp_name)),
                ..operator_method_decl.clone()
            };

            let method_impl_template = self
                .cpp_template
                .as_ref()
                .is_some_and(|c| !c.names.is_empty())
                .then(|| self.cpp_template.clone())
                .flatten();

            let convert_line = match self_td.is_value_type() || self_td.is_enum_type() {
                true => {
                    // box
                    "static_cast<void*>(::il2cpp_utils::Box(this))".to_string()
                }
                false => "static_cast<void*>(this)".to_string(),
            };

            let body: Vec<Arc<dyn CppWritable>> = vec![Arc::new(CppLine::make(format!(
                "return static_cast<{interface_cpp_pointer}>({convert_line});"
            )))];
            let declaring_cpp_full_name = self.cpp_name_components.remove_pointer().combine_all();
            let operator_method_impl = CppMethodImpl {
                body: body.clone(),
                declaring_cpp_full_name: declaring_cpp_full_name.clone(),
                template: method_impl_template.clone(),
                ..operator_method_decl.clone().into()
            };

            let helper_method_impl = CppMethodImpl {
                body: body.clone(),
                declaring_cpp_full_name,
                template: method_impl_template,
                ..helper_method_decl.clone().into()
            };

            // operator
            self.declarations
                .push(CppMember::MethodDecl(operator_method_decl).into());
            self.implementations
                .push(CppMember::MethodImpl(operator_method_impl).into());

            // helper method
            self.declarations
                .push(CppMember::MethodDecl(helper_method_decl).into());
            self.implementations
                .push(CppMember::MethodImpl(helper_method_impl).into());
        }
    }

    fn make_nested_types(
        &mut self,
        nested_types: HashSet<ResolvedType>,
        name_resolver: &CppNameResolver,
        config: &CppGenerationConfig,
    ) {
        if nested_types.len() == 0 {
            return;
        }

        let metadata = name_resolver.metadata;
        let ctx_collection = name_resolver.collection;
        let generic_instantiation_args = self.cpp_name_components.generics.clone();

        let aliases = nested_types
            .into_iter()
            .map(|nested_resolved_ty| {
                let nested_ty =
                    &metadata.metadata_registration.types[nested_resolved_ty.ty as usize];
                let nested_tag = CsTypeTag::from_type_data(nested_ty.data, metadata.metadata);

                (
                    nested_tag,
                    nested_tag.get_tdi().get_type_definition(metadata.metadata),
                )
            })
            .filter(|t| !metadata.blacklisted_types.contains(&t.0.get_tdi()))
            .map(|(nested_tag, nested_td)| {
                let nested_context = ctx_collection
                    .get_context(nested_tag)
                    .expect("Unable to find CppContext");
                let nested = ctx_collection
                    .get_cpp_type(nested_tag)
                    .expect("Unable to find nested CppType");

                let alias = CppUsingAlias::from_cpp_type(
                    config.name_cpp(nested_td.name(metadata.metadata)),
                    nested,
                    generic_instantiation_args.clone(),
                    // if no generic args are made, we can do the generic fixup
                    // ORDER OF PASSES MATTERS
                    nested.generic_instantiations_args_types.is_none(),
                );
                let fd = CppForwardDeclare::from_cpp_type(nested);
                let inc = CppInclude::new_context_typedef(nested_context);

                (alias, fd, inc)
            })
            .collect_vec();

        for (alias, fd, inc) in aliases {
            self.declarations
                .insert(0, CppMember::CppUsingAlias(alias).into());
            self.requirements.add_forward_declare((fd, inc));
        }
    }

    fn create_method(
        &mut self,
        declaring_type: &Il2CppTypeDefinition,
        method_index: MethodIndex,

        metadata: &CordlMetadata,
        ctx_collection: &CppContextCollection,
        config: &CppGenerationConfig,
        is_generic_method_inst: bool,
    ) {
        let method = &metadata.metadata.global_metadata.methods[method_index];

        // TODO: sanitize method name for c++
        let m_name = method.name(metadata.metadata);

        let m_ret_type = metadata
            .metadata_registration
            .types
            .get(method.return_type as usize)
            .unwrap();

        let m_params_with_def: Vec<CppParam> = self.make_parameters(
            method,
            method_index,
            is_generic_method_inst,
            metadata,
            config,
            ctx_collection,
        );

        let m_params_no_def: Vec<CppParam> = m_params_with_def
            .iter()
            .cloned()
            .map(|mut p| {
                p.def_value = None;
                p
            })
            .collect_vec();

        // TODO: Add template<typename ...> if a generic inst e.g
        // T UnityEngine.Component::GetComponent<T>() -> bs_hook::Il2CppWrapperType UnityEngine.Component::GetComponent()
        let template = if method.generic_container_index.is_valid() {
            match is_generic_method_inst {
                true => Some(CppTemplate { names: vec![] }),
                false => {
                    let generics = method
                        .generic_container(metadata.metadata)
                        .unwrap()
                        .generic_parameters(metadata.metadata)
                        .iter()
                        .map(|param| param.name(metadata.metadata).to_string());

                    Some(CppTemplate::make_typenames(generics))
                }
            }
        } else {
            None
        };

        let declaring_type_template = if self
            .cpp_template
            .as_ref()
            .is_some_and(|t| !t.names.is_empty())
        {
            self.cpp_template.clone()
        } else {
            None
        };

        let literal_types = if is_generic_method_inst {
            self.method_generic_instantiation_map
                .get(&method_index)
                .cloned()
        } else {
            None
        };

        let resolved_generic_types = literal_types.map(|literal_types| {
            literal_types
                .iter()
                .map(|t| &metadata.metadata_registration.types[*t as usize])
                .map(|t| {
                    self.cppify_name_il2cpp(ctx_collection, metadata, t, 0, TypeUsage::GenericArg)
                        .combine_all()
                })
                .collect_vec()
        });

        // Lazy cppify
        let make_ret_cpp_type_name = |cpp_type: &mut CppType| -> String {
            let full_name = m_ret_type.full_name(metadata.metadata);
            if full_name == "System.Enum" {
                cpp_type.requirements.needs_enum_include();
                ENUM_PTR_TYPE.into()
            } else if full_name == "System.ValueType" {
                cpp_type.requirements.needs_value_include();
                VT_PTR_TYPE.into()
            } else {
                cpp_type
                    .cppify_name_il2cpp(
                        ctx_collection,
                        metadata,
                        m_ret_type,
                        0,
                        TypeUsage::ReturnType,
                    )
                    .combine_all()
            }
        };

        let m_ret_cpp_type_name = {
            let fixup_name = match is_generic_method_inst {
                false => self.il2cpp_mvar_use_param_name(
                    metadata,
                    method_index,
                    make_ret_cpp_type_name,
                    m_ret_type,
                ),
                true => make_ret_cpp_type_name(self),
            };

            self.il2cpp_byref(fixup_name, m_ret_type)
        };

        // Reference type constructor
        if m_name == ".ctor" {
            Self::create_ref_constructor(self, declaring_type, &m_params_with_def, &template);
        }
        let cpp_m_name = {
            let cpp_m_name = config.name_cpp(m_name);

            // static functions with same name and params but
            // different ret types can exist
            // so we add their ret types
            let fixup_name = match cpp_m_name == "op_Implicit" || cpp_m_name == "op_Explicit" {
                true => {
                    cpp_m_name
                        + "_"
                        + &config
                            .sanitize_to_cpp_name(&m_ret_cpp_type_name)
                            .replace('*', "_")
                }
                false => cpp_m_name,
            };

            match &resolved_generic_types {
                Some(resolved_generic_types) => {
                    format!("{fixup_name}<{}>", resolved_generic_types.join(", "))
                }
                None => fixup_name,
            }
        };

        let declaring_type = method.declaring_type(metadata.metadata);
        let tag = CsTypeTag::TypeDefinitionIndex(method.declaring_type);

        let method_calc = metadata.method_calculations.get(&method_index);

        // generic methods don't have definitions if not an instantiation
        let method_stub = !is_generic_method_inst && template.is_some();

        let method_decl = CppMethodDecl {
            body: None,
            brief: format!(
                "Method {m_name}, addr 0x{:x}, size 0x{:x}, virtual {}, abstract: {}, final {}",
                method_calc.map(|m| m.addrs).unwrap_or(u64::MAX),
                method_calc.map(|m| m.estimated_size).unwrap_or(usize::MAX),
                method.is_virtual_method(),
                method.is_abstract_method(),
                method.is_final_method()
            )
            .into(),
            is_const: false,
            is_constexpr: false,
            is_no_except: false,
            cpp_name: cpp_m_name.clone(),
            return_type: m_ret_cpp_type_name.clone(),
            parameters: m_params_no_def.clone(),
            instance: !method.is_static_method(),
            template: template.clone(),
            suffix_modifiers: Default::default(),
            prefix_modifiers: Default::default(),
            is_virtual: false,
            is_implicit_operator: false,
            is_explicit_operator: false,

            is_inline: true,
        };

        let instance_ptr: String = if method.is_static_method() {
            "nullptr".into()
        } else {
            "this".into()
        };

        const METHOD_INFO_VAR_NAME: &str = "___internal_method";

        let method_invoke_params = vec![instance_ptr.as_str(), METHOD_INFO_VAR_NAME];
        let param_names = CppParam::params_names(&method_decl.parameters).map(|s| s.as_str());
        let declaring_type_cpp_full_name = self.cpp_name_components.remove_pointer().combine_all();

        let declaring_classof_call = format!(
            "::il2cpp_utils::il2cpp_type_check::il2cpp_no_arg_class<{}>::get()",
            self.cpp_name_components.combine_all()
        );

        let extract_self_class =
            "il2cpp_functions::object_get_class(reinterpret_cast<Il2CppObject*>(this))";

        let params_types_format: String = CppParam::params_types(&method_decl.parameters)
            .map(|t| format!("::il2cpp_utils::il2cpp_type_check::il2cpp_no_arg_type<{t}>::get()"))
            .join(", ");
        let params_types_count = method_decl.parameters.len();

        let resolve_instance_slot_lines = if method.slot != u16::MAX {
            let slot = &method.slot;
            vec![format!(
                "auto* {METHOD_INFO_VAR_NAME} = THROW_UNLESS((::il2cpp_utils::ResolveVtableSlot(
                    {extract_self_class},
                    {declaring_classof_call},
                    {slot}
                )));"
            )]
        } else {
            vec![]
        };

        // TODO: link the method to the interface that originally declared it
        // then the resolve should look something like:
        // resolve(classof(GlobalNamespace::BeatmapLevelPack*), classof(GlobalNamespace::IBeatmapLevelPack*), 0);
        // that way the resolve should work correctly, but it should only happen like that for non-interfaces

        let _resolve_metadata_slot_lines = if method.slot != u16::MAX {
            let self_classof_call = "";
            let declaring_classof_call = "";
            let slot = &method.slot;

            vec![format!(
                "auto* {METHOD_INFO_VAR_NAME} = THROW_UNLESS((::il2cpp_utils::ResolveVtableSlot(
                    {self_classof_call},
                    {declaring_classof_call},
                    {slot}
                )));"
            )]
        } else {
            vec![]
        };

        // if no params, just empty span
        // avoid allocs
        let params_types_array_cpp = match params_types_count {
            0 => "::std::span<const Il2CppType* const, 0>()".to_string(),
            _ => format!(
                "::std::array<const Il2CppType*, {params_types_count}>{{{params_types_format}}}"
            ),
        };

        let method_info_lines = match &template {
            Some(template) => {
                // generic
                let template_names = template
                    .just_names()
                    .map(|t| {
                        format!(
                            "::il2cpp_utils::il2cpp_type_check::il2cpp_no_arg_class<{t}>::get()"
                        )
                    })
                    .join(", ");
                let template_count = template.names.len();

                // if no template params, just empty span
                // avoid allocs
                let template_classes_array_cpp = match template_count {
                    0 => "std::span<const Il2CppClass* const, 0>()".to_string(),
                    _ => format!(
                        "std::array<const Il2CppClass*, {template_count}>{{{template_names}}}"
                    ),
                };

                vec![
                format!("static auto* ___internal_method_base = THROW_UNLESS((::il2cpp_utils::FindMethod(
                    {declaring_classof_call},
                    \"{m_name}\",
                    {template_classes_array_cpp},
                    {params_types_array_cpp}
                )));"),
                format!("static auto* {METHOD_INFO_VAR_NAME} = THROW_UNLESS(::il2cpp_utils::MakeGenericMethod(
                    ___internal_method_base,
                    {template_classes_array_cpp}
                ));"),
                ]
            }
            None => {
                vec![
                    format!("static auto* {METHOD_INFO_VAR_NAME} = THROW_UNLESS((::il2cpp_utils::FindMethod(
                        {declaring_classof_call},
                        \"{m_name}\",
                        std::span<const Il2CppClass* const, 0>(),
                        {params_types_array_cpp}
                    )));"),
                    ]
            }
        };

        let method_body_lines = [format!(
            "return ::cordl_internals::RunMethodRethrow<{m_ret_cpp_type_name}, false>({});",
            method_invoke_params
                .into_iter()
                .chain(param_names)
                .join(", ")
        )];

        // instance methods should resolve slots if this is an interface, or if this is a virtual/abstract method, and not a final method
        // static methods can't be virtual or interface anyway so checking for that here is irrelevant
        let should_resolve_slot = declaring_type.is_interface()
            || ((method.is_virtual_method() || method.is_abstract_method())
                && !method.is_final_method());

        let method_body = match should_resolve_slot {
            true => resolve_instance_slot_lines
                .iter()
                .chain(method_body_lines.iter())
                .cloned()
                .map(|l| -> Arc<dyn CppWritable> { Arc::new(CppLine::make(l)) })
                .collect_vec(),
            false => method_info_lines
                .iter()
                .chain(method_body_lines.iter())
                .cloned()
                .map(|l| -> Arc<dyn CppWritable> { Arc::new(CppLine::make(l)) })
                .collect_vec(),
        };

        let method_impl = CppMethodImpl {
            body: method_body,
            parameters: m_params_with_def.clone(),
            brief: None,
            declaring_cpp_full_name: declaring_type_cpp_full_name,
            instance: !method.is_static_method(),
            suffix_modifiers: Default::default(),
            prefix_modifiers: Default::default(),
            template: template.clone(),
            declaring_type_template: declaring_type_template.clone(),

            // defaults
            ..method_decl.clone().into()
        };

        // check if declaring type is the current type or the interface
        // we check TDI because if we are a generic instantiation
        // we just use ourselves if the declaring type is also the same TDI
        let interface_declaring_cpp_type: Option<&CppType> =
            if tag.get_tdi() == self.self_tag.get_tdi() {
                Some(self)
            } else {
                ctx_collection.get_cpp_type(tag)
            };

        // don't emit method size structs for generic methods

        // don't emit method size structs for generic methods

        // if type is a generic
        let has_template_args = self
            .cpp_template
            .as_ref()
            .is_some_and(|t| !t.names.is_empty());

        // don't emit method size structs for generic methods
        if let Some(method_calc) = method_calc
            && template.is_none()
            && !has_template_args
            && !is_generic_method_inst
        {
            self.nonmember_implementations
                .push(Arc::new(CppNonMember::SizeStruct(
                    CppMethodSizeStruct {
                        ret_ty: method_decl.return_type.clone(),
                        cpp_method_name: method_decl.cpp_name.clone(),
                        method_name: m_name.to_string(),
                        declaring_type_name: method_impl.declaring_cpp_full_name.clone(),
                        declaring_classof_call,
                        method_info_lines,
                        method_info_var: METHOD_INFO_VAR_NAME.to_string(),
                        instance: method_decl.instance,
                        params: method_decl.parameters.clone(),
                        template: template.clone(),
                        generic_literals: resolved_generic_types,
                        method_data: CppMethodData {
                            addrs: method_calc.addrs,
                            estimated_size: method_calc.estimated_size,
                        },
                        interface_clazz_of: interface_declaring_cpp_type
                            .map(|d| d.classof_cpp_name())
                            .unwrap_or_else(|| format!("Bad stuff happened {declaring_type:?}")),
                        is_final: method.is_final_method(),
                        slot: if method.slot != u16::MAX {
                            Some(method.slot)
                        } else {
                            None
                        },
                    }
                    .into(),
                )));
        }

        // TODO: Revise this
        const ALLOW_GENERIC_METHOD_STUBS_IMPL: bool = true;
        // If a generic instantiation or not a template
        if !method_stub || ALLOW_GENERIC_METHOD_STUBS_IMPL {
            self.implementations
                .push(CppMember::MethodImpl(method_impl).into());
        }

        if !is_generic_method_inst {
            self.declarations
                .push(CppMember::MethodDecl(method_decl).into());
        }
    }

    pub fn classof_cpp_name(&self) -> String {
        format!(
            "::il2cpp_utils::il2cpp_type_check::il2cpp_no_arg_class<{}>::get",
            self.cpp_name_components.combine_all()
        )
    }

    fn add_interface_operators(
        &mut self,
        metadata: &CordlMetadata<'_>,
        ctx_collection: &CppContextCollection,
        config: &CppGenerationConfig,
        tdi: TypeDefinitionIndex,
    ) {
        let t = &metadata.metadata.global_metadata.type_definitions[tdi];

        for &interface_index in t.interfaces(metadata.metadata) {
            let int_ty = &metadata.metadata_registration.types[interface_index as usize];

            // We have an interface, lets do something with it
            let interface_name_il2cpp =
                &self.cppify_name_il2cpp(ctx_collection, metadata, int_ty, 0, TypeUsage::TypeName);
            let interface_cpp_name = interface_name_il2cpp.remove_pointer().combine_all();
            let interface_cpp_pointer = interface_name_il2cpp.as_pointer().combine_all();

            let operator_method_decl = CppMethodDecl {
                body: Default::default(),
                brief: Some(format!("Convert operator to {interface_cpp_name:?}")),
                cpp_name: interface_cpp_pointer.clone(),
                return_type: "".to_string(),
                instance: true,
                is_const: false,
                is_constexpr: true,
                is_no_except: !t.is_value_type() && !t.is_enum_type(),
                is_implicit_operator: true,
                is_explicit_operator: false,

                is_virtual: false,
                is_inline: true,
                parameters: vec![],
                template: None,
                prefix_modifiers: vec![],
                suffix_modifiers: vec![],
            };
            let helper_method_decl = CppMethodDecl {
                brief: Some(format!("Convert to {interface_cpp_name:?}")),
                is_implicit_operator: false,
                return_type: interface_cpp_pointer.clone(),
                cpp_name: format!("i_{}", config.sanitize_to_cpp_name(&interface_cpp_name)),
                ..operator_method_decl.clone()
            };

            let method_impl_template = self
                .cpp_template
                .as_ref()
                .is_some_and(|c| !c.names.is_empty())
                .then(|| self.cpp_template.clone())
                .flatten();

            let convert_line = match t.is_value_type() || t.is_enum_type() {
                true => {
                    // box
                    "static_cast<void*>(::il2cpp_utils::Box(this))".to_string()
                }
                false => "static_cast<void*>(this)".to_string(),
            };

            let body: Vec<Arc<dyn CppWritable>> = vec![Arc::new(CppLine::make(format!(
                "return static_cast<{interface_cpp_pointer}>({convert_line});"
            )))];
            let declaring_cpp_full_name = self.cpp_name_components.remove_pointer().combine_all();
            let operator_method_impl = CppMethodImpl {
                body: body.clone(),
                declaring_cpp_full_name: declaring_cpp_full_name.clone(),
                template: method_impl_template.clone(),
                ..operator_method_decl.clone().into()
            };

            let helper_method_impl = CppMethodImpl {
                body: body.clone(),
                declaring_cpp_full_name,
                template: method_impl_template,
                ..helper_method_decl.clone().into()
            };

            // operator
            self.declarations
                .push(CppMember::MethodDecl(operator_method_decl).into());
            self.implementations
                .push(CppMember::MethodImpl(operator_method_impl).into());

            // helper method
            self.declarations
                .push(CppMember::MethodDecl(helper_method_decl).into());
            self.implementations
                .push(CppMember::MethodImpl(helper_method_impl).into());
        }
    }

    fn create_size_assert(&mut self) {
        // FIXME: make this work with templated types that either: have a full template (complete instantiation), or only require a pointer (size should be stable)
        // for now, skip templated types
        if self.cpp_template.is_some() {
            return;
        }

        if let Some(size) = self.size_info.as_ref().map(|s| s.instance_size) {
            let cpp_name = self.cpp_name_components.remove_pointer().combine_all();

            assert!(!cpp_name.trim().is_empty(), "CPP Name cannot be empty!");

            let assert = CppStaticAssert {
                condition: format!("::cordl_internals::size_check_v<{cpp_name}, 0x{size:x}>"),
                message: Some("Size mismatch!".to_string()),
            };

            self.nonmember_declarations
                .push(Arc::new(CppNonMember::CppStaticAssert(assert)));
        } else {
            todo!("Why does this type not have a valid size??? {self:?}");
        }
    }

    ///
    /// add missing size for type
    ///
    fn create_size_padding(
        &mut self,
        metadata: &CordlMetadata,
        name_resolver: &CppNameResolver,
        config: &CppGenerationConfig,
    ) {
        // // get type metadata size
        let Some(type_definition_sizes) = &metadata.metadata_registration.type_definition_sizes
        else {
            return;
        };

        let metadata_size = &type_definition_sizes.get(tdi.index() as usize);

        let Some(metadata_size) = metadata_size else {
            return;
        };

        // // ignore types that aren't sized
        if metadata_size.instance_size == 0 || metadata_size.instance_size == u32::MAX {
            return;
        }

        // // if the size matches what we calculated, we're fine
        // if metadata_size.instance_size == calculated_size {
        //     return;
        // }
        // let remaining_size = metadata_size.instance_size.abs_diff(calculated_size);

        let Some(size_info) = self.size_info.as_ref() else {
            return;
        };

        // for all types, the size il2cpp metadata says the type should be, for generics this is calculated though
        let metadata_size_instance = size_info.instance_size;

        // align the calculated size to the next multiple of natural_alignment, similiar to what happens when clang compiles our generated code
        // this comes down to adding our size, and removing any bits that make it more than the next multiple of alignment
        #[cfg(feature = "il2cpp_v29")]
        let aligned_calculated_size = match size_info.natural_alignment as u32 {
            0 => size_info.calculated_instance_size,
            alignment => (size_info.calculated_instance_size + alignment) & !(alignment - 1),
        };
        #[cfg(feature = "il2cpp_v31")]
        let aligned_calculated_size = size_info.calculated_instance_size;

        // return if calculated layout size == metadata size
        if aligned_calculated_size == metadata_size_instance {
            return;
        }

        let remaining_size = metadata_size_instance.abs_diff(size_info.calculated_instance_size);

        // pack the remaining size to fit the packing of the type
        let closest_packing = |size: u32| match size {
            0 => 0,
            1 => 1,
            2 => 2,
            3 => 4,
            4 => 4,
            _ => 8,
        };

        let packing = self
            .packing
            .unwrap_or_else(|| closest_packing(size_info.calculated_instance_size));
        let packed_remaining_size = match packing == 0 {
            true => remaining_size,
            false => remaining_size & !(packing - 1),
        };

        // if the packed remaining size ends up being 0, don't emit padding
        if packed_remaining_size == 0 {
            return;
        }

        self.declarations.push(
            CppMember::FieldDecl(CppFieldDecl {
                cpp_name: format!("_cordl_size_padding[0x{packed_remaining_size:x}]").to_string(),
                field_ty: "uint8_t".into(),
                offset: Some(size_info.instance_size),
                instance: true,
                readonly: false,
                const_expr: false,
                value: None,
                brief_comment: Some(format!(
                    "Size padding 0x{:x} - 0x{:x} = 0x{remaining_size:x}, packed as 0x{packed_remaining_size:x}",
                    metadata_size_instance, size_info.calculated_instance_size
                )),
                is_private: false,
            })
            .into(),
        );
    }

    fn create_ref_size(&mut self) {
        if let Some(size) = self.size_info.as_ref().map(|s| s.instance_size) {
            self.declarations.push(
                CppMember::FieldDecl(CppFieldDecl {
                    cpp_name: REFERENCE_TYPE_WRAPPER_SIZE.to_string(),
                    field_ty: "auto".to_string(),
                    offset: None,
                    instance: false,
                    readonly: false,
                    const_expr: true,
                    value: Some(format!("0x{size:x}")),
                    brief_comment: Some("The size of the true reference type".to_string()),
                    is_private: false,
                })
                .into(),
            );

            // here we push an instance field like uint8_t __fields[total_size - base_size] to make sure ref types are the exact size they should be
            let inherits = self.get_inherits().collect_vec();
            let fixup_size = match inherits.first() {
                Some(base_type) => format!("0x{size:x} - sizeof({base_type})"),
                None => format!("0x{size:x}"),
            };

            self.declarations.push(
                CppMember::FieldDecl(CppFieldDecl {
                    cpp_name: format!("{REFERENCE_TYPE_FIELD_SIZE}[{fixup_size}]"),
                    field_ty: "uint8_t".to_string(),
                    offset: None,
                    instance: true,
                    readonly: false,
                    const_expr: false,
                    value: Some("".into()),
                    brief_comment: Some(
                        "The size this ref type adds onto its base type, may evaluate to 0"
                            .to_string(),
                    ),
                    is_private: false,
                })
                .into(),
            );
        } else {
            todo!("Why does this type not have a valid size??? {:?}", self);
        }
    }

    fn create_enum_backing_type_constant(
        &mut self,
        metadata: &CordlMetadata,
        name_resolver: &CppNameResolver,
        config: &CppGenerationConfig,
    ) {
        let t = tdi.get_type_definition(metadata.metadata);

        let backing_field_idx = t.element_type_index as usize;
        let backing_field_ty = &metadata.metadata_registration.types[backing_field_idx];

        let enum_base = self
            .cppify_name_il2cpp(
                ctx_collection,
                metadata,
                backing_field_ty,
                0,
                TypeUsage::TypeName,
            )
            .remove_pointer()
            .combine_all();

        self.declarations.push(
            CppMember::CppUsingAlias(CppUsingAlias {
                alias: __CORDL_BACKING_ENUM_TYPE.to_string(),
                result: enum_base,
                template: None,
            })
            .into(),
        );
    }

    fn create_enum_wrapper(
        &mut self,
        metadata: &CordlMetadata,
        name_resolver: &CppNameResolver,
        config: &CppGenerationConfig,
    ) {
        let t = tdi.get_type_definition(metadata.metadata);
        let unwrapped_name = format!("__{}_Unwrapped", self.cpp_name());
        let backing_field = metadata
            .metadata_registration
            .types
            .get(t.element_type_index as usize)
            .unwrap();

        let enum_base = self
            .cppify_name_il2cpp(
                ctx_collection,
                metadata,
                backing_field,
                0,
                TypeUsage::TypeName,
            )
            .remove_pointer()
            .combine_all();

        let enum_entries = t
            .fields(metadata.metadata)
            .iter()
            .enumerate()
            .map(|(i, field)| {
                let field_index = FieldIndex::new(t.field_start.index() + i as u32);

                (field_index, field)
            })
            .filter_map(|(field_index, field)| {
                let f_type = metadata
                    .metadata_registration
                    .types
                    .get(field.type_index as usize)
                    .unwrap();

                f_type.is_static().then(|| {
                    // enums static fields are always the enum values
                    let f_name = field.name(metadata.metadata);
                    let value = Self::field_default_value(metadata, field_index)
                        .expect("Enum without value!");

                    // prepend enum name with __E_ to prevent accidentally creating enum values that are reserved for builtin macros
                    format!("__E_{f_name} = {value},")
                })
            })
            .map(|s| -> CppMember { CppMember::CppLine(s.into()) });

        let nested_struct = CppNestedStruct {
            base_type: Some(enum_base.clone()),
            declaring_name: unwrapped_name.clone(),
            is_class: false,
            is_enum: true,
            is_private: false,
            declarations: enum_entries.map(Rc::new).collect(),
            brief_comment: Some(format!("Nested struct {unwrapped_name}")),
            packing: None,
        };
        self.declarations
            .push(CppMember::NestedStruct(nested_struct).into());

        let operator_body = format!("return static_cast<{unwrapped_name}>(this->value__);");
        let unwrapped_operator_decl = CppMethodDecl {
            cpp_name: Default::default(),
            instance: true,
            return_type: unwrapped_name,

            brief: Some("Conversion into unwrapped enum value".to_string()),
            body: Some(vec![Arc::new(CppLine::make(operator_body))]),
            is_const: true,
            is_constexpr: true,
            is_virtual: false,
            is_explicit_operator: false,
            is_implicit_operator: true,
            is_no_except: true,
            parameters: vec![],
            prefix_modifiers: vec![],
            suffix_modifiers: vec![],
            template: None,
            is_inline: true,
        };
        // convert to proper backing type
        let backing_operator_body = format!("return static_cast<{enum_base}>(this->value__);");
        let backing_operator_decl = CppMethodDecl {
            brief: Some("Conversion into unwrapped enum value".to_string()),
            return_type: enum_base,
            body: Some(vec![Arc::new(CppLine::make(backing_operator_body))]),
            is_explicit_operator: true,
            ..unwrapped_operator_decl.clone()
        };

        self.declarations
            .push(CppMember::MethodDecl(unwrapped_operator_decl).into());
        self.declarations
            .push(CppMember::MethodDecl(backing_operator_decl).into());
    }

    fn type_default_value(
        metadata: &CordlMetadata,
        cpp_type: Option<&CppType>,
        ty: &Il2CppType,
    ) -> String {
        let matched_ty: &Il2CppType = match ty.data {
            // get the generic inst
            TypeData::GenericClassIndex(inst_idx) => {
                let gen_class = &metadata
                    .metadata
                    .runtime_metadata
                    .metadata_registration
                    .generic_classes[inst_idx];

                &metadata.metadata_registration.types[gen_class.type_index]
            }
            // get the underlying type of the generic param
            TypeData::GenericParameterIndex(param) => match param.is_valid() {
                true => {
                    let gen_param = &metadata.metadata.global_metadata.generic_parameters[param];

                    cpp_type
                        .and_then(|cpp_type| {
                            cpp_type
                                .generic_instantiations_args_types
                                .as_ref()
                                .and_then(|gen_args| gen_args.get(gen_param.num as usize))
                                .map(|t| &metadata.metadata_registration.types[*t])
                        })
                        .unwrap_or(ty)
                }
                false => ty,
            },
            _ => ty,
        };

        match matched_ty.valuetype {
            true => "{}".to_string(),
            false => "nullptr".to_string(),
        }
    }

    fn field_default_value(metadata: &CordlMetadata, field_index: FieldIndex) -> Option<String> {
        metadata
            .metadata
            .global_metadata
            .field_default_values
            .as_vec()
            .iter()
            .find(|f| f.field_index == field_index)
            .map(|def| {
                let ty: &Il2CppType = metadata
                    .metadata_registration
                    .types
                    .get(def.type_index as usize)
                    .unwrap();

                // get default value for given type
                if !def.data_index.is_valid() {
                    return Self::type_default_value(metadata, None, ty);
                }

                todo!()
                // Self::default_value_blob(metadata, ty, def.data_index.index() as usize, true, true)
            })
    }
    fn param_default_value(
        metadata: &CordlMetadata,
        parameter_index: ParameterIndex,
    ) -> Option<String> {
        metadata
            .metadata
            .global_metadata
            .parameter_default_values
            .as_vec()
            .iter()
            .find(|p| p.parameter_index == parameter_index)
            .map(|def| {
                let mut ty = metadata
                    .metadata_registration
                    .types
                    .get(def.type_index as usize)
                    .unwrap();

                todo!();

                // ty = Self::unbox_nullable_valuetype(metadata, ty);

                // This occurs when the type is `null` or `default(T)` for value types
                if !def.data_index.is_valid() {
                    return Self::type_default_value(metadata, None, ty);
                }

                if let Il2CppTypeEnum::Valuetype = ty.ty {
                    match ty.data {
                        TypeData::TypeDefinitionIndex(tdi) => {
                            let type_def = &metadata.metadata.global_metadata.type_definitions[tdi];

                            // System.Nullable`1
                            if type_def.name(metadata.metadata) == "Nullable`1"
                                && type_def.namespace(metadata.metadata) == "System"
                            {
                                ty = metadata
                                    .metadata_registration
                                    .types
                                    .get(type_def.byval_type_index as usize)
                                    .unwrap();
                            }
                        }
                        _ => todo!(),
                    }
                }

                todo!()

                // Self::default_value_blob(metadata, ty, def.data_index.index() as usize, true, true)
            })
    }

    fn create_valuetype_field_wrapper(&mut self) {
        if self.size_info.is_none() {
            todo!("Why does this type not have a valid size??? {:?}", self);
        }

        let size = self.size_info.as_ref().map(|s| s.instance_size).unwrap();

        self.requirements.needs_byte_include();
        self.declarations.push(
            CppMember::FieldDecl(CppFieldDecl {
                cpp_name: VALUE_TYPE_WRAPPER_SIZE.to_string(),
                field_ty: "auto".to_string(),
                offset: None,
                instance: false,
                readonly: false,
                const_expr: true,
                value: Some(format!("0x{size:x}")),
                brief_comment: Some("The size of the true value type".to_string()),
                is_private: false,
            })
            .into(),
        );
    }

    fn create_valuetype_constructor(
        &mut self,
        metadata: &CordlMetadata,
        name_resolver: &CppNameResolver,
        config: &CppGenerationConfig,
    ) {
        let t = &metadata.metadata.global_metadata.type_definitions[tdi];

        let instance_fields = t
            .fields(metadata.metadata)
            .iter()
            .filter_map(|field| {
                let f_type = metadata
                    .metadata_registration
                    .types
                    .get(field.type_index as usize)
                    .unwrap();

                // ignore statics or constants
                if f_type.is_static() || f_type.is_constant() {
                    return None;
                }

                let f_type_cpp_name = self
                    .cppify_name_il2cpp(ctx_collection, metadata, f_type, 0, TypeUsage::Field)
                    .combine_all();

                // Get the inner type of a Generic Inst
                // e.g ReadOnlySpan<char> -> ReadOnlySpan<T>
                let def_value = Self::type_default_value(metadata, Some(self), f_type);

                let f_cpp_name = config
                    .name_cpp_plus(field.name(metadata.metadata), &[self.cpp_name().as_str()]);

                Some(CppParam {
                    name: f_cpp_name,
                    ty: f_type_cpp_name,
                    modifiers: "".to_string(),
                    // no default value for first param
                    def_value: Some(def_value),
                })
            })
            .collect_vec();

        if instance_fields.is_empty() {
            return;
        }
        // Maps into the first parent -> ""
        // so then Parent()
        let base_ctor = self.parent.as_ref().map(|s| (s.clone(), "".to_string()));

        let body: Vec<Arc<dyn CppWritable>> = instance_fields
            .iter()
            .map(|p| {
                let name = &p.name;
                CppLine::make(format!("this->{name} = {name};"))
            })
            .map(Arc::new)
            // Why is this needed? _sigh_
            .map(|arc| -> Arc<dyn CppWritable> { arc })
            .collect_vec();

        let params_no_def = instance_fields
            .iter()
            .cloned()
            .map(|mut c| {
                c.def_value = None;
                c
            })
            .collect_vec();

        let constructor_decl = CppConstructorDecl {
            cpp_name: self.cpp_name().clone(),
            template: None,
            is_constexpr: true,
            is_explicit: false,
            is_default: false,
            is_no_except: true,
            is_delete: false,
            is_protected: false,

            base_ctor,
            initialized_values: HashMap::new(),
            // initialize values with params
            // initialized_values: instance_fields
            //     .iter()
            //     .map(|p| (p.name.to_string(), p.name.to_string()))
            //     .collect(),
            parameters: params_no_def,
            brief: None,
            body: None,
        };

        let method_impl_template = if self
            .cpp_template
            .as_ref()
            .is_some_and(|c| !c.names.is_empty())
        {
            self.cpp_template.clone()
        } else {
            None
        };

        let constructor_impl = CppConstructorImpl {
            body,
            template: method_impl_template,
            parameters: instance_fields,
            declaring_full_name: self.cpp_name_components.remove_pointer().combine_all(),
            ..constructor_decl.clone().into()
        };

        self.declarations
            .push(CppMember::ConstructorDecl(constructor_decl).into());
        self.implementations
            .push(CppMember::ConstructorImpl(constructor_impl).into());
    }

    fn create_valuetype_default_constructors(&mut self) {
        // create the various copy and move ctors and operators
        let cpp_name = self.cpp_name();
        let wrapper = format!("{VALUE_WRAPPER_TYPE}<{VALUE_TYPE_WRAPPER_SIZE}>::instance");

        let move_ctor = CppConstructorDecl {
            cpp_name: cpp_name.clone(),
            parameters: vec![CppParam {
                ty: cpp_name.clone(),
                name: "".to_string(),
                modifiers: "&&".to_string(),
                def_value: None,
            }],
            template: None,
            is_constexpr: true,
            is_explicit: false,
            is_default: true,
            is_no_except: false,
            is_delete: false,
            is_protected: false,
            base_ctor: None,
            initialized_values: Default::default(),
            brief: None,
            body: None,
        };

        let copy_ctor = CppConstructorDecl {
            cpp_name: cpp_name.clone(),
            parameters: vec![CppParam {
                ty: cpp_name.clone(),
                name: "".to_string(),
                modifiers: "const &".to_string(),
                def_value: None,
            }],
            template: None,
            is_constexpr: true,
            is_explicit: false,
            is_default: true,
            is_no_except: false,
            is_delete: false,
            is_protected: false,
            base_ctor: None,
            initialized_values: Default::default(),
            brief: None,
            body: None,
        };

        let move_operator_eq = CppMethodDecl {
            cpp_name: "operator=".to_string(),
            return_type: format!("{cpp_name}&"),
            parameters: vec![CppParam {
                ty: cpp_name.clone(),
                name: "o".to_string(),
                modifiers: "&&".to_string(),
                def_value: None,
            }],
            instance: true,
            template: None,
            suffix_modifiers: vec![],
            prefix_modifiers: vec![],
            is_virtual: false,
            is_constexpr: true,
            is_const: false,
            is_no_except: true,
            is_implicit_operator: false,
            is_explicit_operator: false,

            is_inline: false,
            brief: None,
            body: Some(vec![
                Arc::new(CppLine::make(format!(
                    "this->{wrapper} = std::move(o.{wrapper});"
                ))),
                Arc::new(CppLine::make("return *this;".to_string())),
            ]),
        };

        let copy_operator_eq = CppMethodDecl {
            cpp_name: "operator=".to_string(),
            return_type: format!("{cpp_name}&"),
            parameters: vec![CppParam {
                ty: cpp_name.clone(),
                name: "o".to_string(),
                modifiers: "const &".to_string(),
                def_value: None,
            }],
            instance: true,
            template: None,
            suffix_modifiers: vec![],
            prefix_modifiers: vec![],
            is_virtual: false,
            is_constexpr: true,
            is_const: false,
            is_no_except: true,
            is_implicit_operator: false,
            is_explicit_operator: false,

            is_inline: false,
            brief: None,
            body: Some(vec![
                Arc::new(CppLine::make(format!("this->{wrapper} = o.{wrapper};"))),
                Arc::new(CppLine::make("return *this;".to_string())),
            ]),
        };

        self.declarations
            .push(CppMember::ConstructorDecl(move_ctor).into());
        self.declarations
            .push(CppMember::ConstructorDecl(copy_ctor).into());
        self.declarations
            .push(CppMember::MethodDecl(move_operator_eq).into());
        self.declarations
            .push(CppMember::MethodDecl(copy_operator_eq).into());
    }

    fn create_ref_default_constructor(&mut self) {
        let cpp_name = self.cpp_name().clone();

        let cs_name = self.name().clone();

        // Skip if System.ValueType or System.Enum
        if self.namespace() == "System" && (cs_name == "ValueType" || cs_name == "Enum") {
            return;
        }

        let default_ctor = CppConstructorDecl {
            cpp_name: cpp_name.clone(),
            parameters: vec![],
            template: None,
            is_constexpr: true,
            is_explicit: false,
            is_default: true,
            is_no_except: true,
            is_delete: false,
            is_protected: true,

            base_ctor: None,
            initialized_values: HashMap::new(),
            brief: Some("Default ctor for custom type constructor invoke".to_string()),
            body: None,
        };
        let copy_ctor = CppConstructorDecl {
            cpp_name: cpp_name.clone(),
            parameters: vec![CppParam {
                name: "".to_string(),
                modifiers: " const&".to_string(),
                ty: cpp_name.clone(),
                def_value: None,
            }],
            template: None,
            is_constexpr: true,
            is_explicit: false,
            is_default: true,
            is_no_except: true,
            is_delete: false,
            is_protected: false,

            base_ctor: None,
            initialized_values: HashMap::new(),
            brief: None,
            body: None,
        };
        let move_ctor = CppConstructorDecl {
            cpp_name: cpp_name.clone(),
            parameters: vec![CppParam {
                name: "".to_string(),
                modifiers: "&&".to_string(),
                ty: cpp_name.clone(),
                def_value: None,
            }],
            template: None,
            is_constexpr: true,
            is_explicit: false,
            is_default: true,
            is_no_except: true,
            is_delete: false,
            is_protected: false,

            base_ctor: None,
            initialized_values: HashMap::new(),
            brief: None,
            body: None,
        };

        self.declarations
            .push(CppMember::ConstructorDecl(default_ctor).into());
        self.declarations
            .push(CppMember::ConstructorDecl(copy_ctor).into());
        self.declarations
            .push(CppMember::ConstructorDecl(move_ctor).into());

        // // Delegates and such are reference types with no inheritance
        // if self.inherit.is_empty() {
        //     return;
        // }

        // let base_type = self
        //     .inherit
        //     .get(0)
        //     .expect("No parent for reference type?");

        // self.declarations.push(
        //     CppMember::ConstructorDecl(CppConstructorDecl {
        //         cpp_name: cpp_name.clone(),
        //         parameters: vec![CppParam {
        //             name: "ptr".to_string(),
        //             modifiers: "".to_string(),
        //             ty: "void*".to_string(),
        //             def_value: None,
        //         }],
        //         template: None,
        //         is_constexpr: true,
        //         is_explicit: true,
        //         is_default: false,
        //         is_no_except: true,
        //         is_delete: false,
        //         is_protected: false,

        //         base_ctor: Some((base_type.clone(), "ptr".to_string())),
        //         initialized_values: HashMap::new(),
        //         brief: None,
        //         body: Some(vec![]),
        //     })
        //     .into(),
        // );
    }
    fn make_interface_constructors(&mut self) {
        let cpp_name = self.cpp_name().clone();

        let base_type = self.parent.as_ref().expect("No parent for interface type?");

        self.declarations.push(
            CppMember::ConstructorDecl(CppConstructorDecl {
                cpp_name: cpp_name.clone(),
                parameters: vec![CppParam {
                    name: "ptr".to_string(),
                    modifiers: "".to_string(),
                    ty: "void*".to_string(),
                    def_value: None,
                }],
                template: None,
                is_constexpr: true,
                is_explicit: true,
                is_default: false,
                is_no_except: true,
                is_delete: false,
                is_protected: false,

                base_ctor: Some((base_type.clone(), "ptr".to_string())),
                initialized_values: HashMap::new(),
                brief: None,
                body: Some(vec![]),
            })
            .into(),
        );
    }
    fn create_ref_default_operators(&mut self) {
        let cpp_name = self.cpp_name();

        // Skip if System.ValueType or System.Enum
        if self.namespace() == "System"
            && (self.cpp_name() == "ValueType" || self.cpp_name() == "Enum")
        {
            return;
        }

        // Delegates and such are reference types with no inheritance
        if self.get_inherits().count() > 0 {
            return;
        }

        self.declarations.push(
            CppMember::CppLine(CppLine {
                line: format!(
                    "
  constexpr {cpp_name}& operator=(std::nullptr_t) noexcept {{
    this->{REFERENCE_WRAPPER_INSTANCE_NAME} = nullptr;
    return *this;
  }};

  constexpr {cpp_name}& operator=(void* o) noexcept {{
    this->{REFERENCE_WRAPPER_INSTANCE_NAME} = o;
    return *this;
  }};

  constexpr {cpp_name}& operator=({cpp_name}&& o) noexcept = default;
  constexpr {cpp_name}& operator=({cpp_name} const& o) noexcept = default;
                "
                ),
            })
            .into(),
        );
    }

    fn delete_move_ctor(&mut self) {
        let t = &self.cpp_name_components.name;

        let move_ctor = CppConstructorDecl {
            cpp_name: t.clone(),
            parameters: vec![CppParam {
                def_value: None,
                modifiers: "&&".to_string(),
                name: "".to_string(),
                ty: t.clone(),
            }],
            template: None,
            is_constexpr: false,
            is_explicit: false,
            is_default: false,
            is_no_except: false,
            is_protected: false,
            is_delete: true,
            base_ctor: None,
            initialized_values: Default::default(),
            brief: Some("delete move ctor to prevent accidental deref moves".to_string()),
            body: None,
        };

        self.declarations
            .push(CppMember::ConstructorDecl(move_ctor).into());
    }

    fn delete_copy_ctor(&mut self) {
        let t = &self.cpp_name_components.name;

        let move_ctor = CppConstructorDecl {
            cpp_name: t.clone(),
            parameters: vec![CppParam {
                def_value: None,
                modifiers: "const&".to_string(),
                name: "".to_string(),
                ty: t.clone(),
            }],
            template: None,
            is_constexpr: false,
            is_explicit: false,
            is_default: false,
            is_no_except: false,
            is_delete: true,
            is_protected: false,
            base_ctor: None,
            initialized_values: Default::default(),
            brief: Some("delete copy ctor to prevent accidental deref copies".to_string()),
            body: None,
        };

        self.declarations
            .push(CppMember::ConstructorDecl(move_ctor).into());
    }

    fn add_default_ctor(&mut self, protected: bool) {
        let t = &self.cpp_name_components.name;

        let default_ctor_decl = CppConstructorDecl {
            cpp_name: t.clone(),
            parameters: vec![],
            template: None,
            is_constexpr: true,
            is_explicit: false,
            is_default: false,
            is_no_except: false,
            is_delete: false,
            is_protected: protected,
            base_ctor: None,
            initialized_values: Default::default(),
            brief: Some("default ctor".to_string()),
            body: None,
        };

        let default_ctor_impl = CppConstructorImpl {
            body: vec![],
            declaring_full_name: self.cpp_name_components.remove_pointer().combine_all(),
            template: self.cpp_template.clone(),
            ..default_ctor_decl.clone().into()
        };

        self.declarations
            .push(CppMember::ConstructorDecl(default_ctor_decl).into());

        self.implementations
            .push(CppMember::ConstructorImpl(default_ctor_impl).into());
    }

    fn add_type_index_member(&mut self) {
        let tdi: TypeDefinitionIndex = self.self_tag.get_tdi();

        let il2cpp_metadata_type_index = CppFieldDecl {
            cpp_name: "__IL2CPP_TYPE_DEFINITION_INDEX".into(),
            field_ty: "uint32_t".into(),
            offset: None,
            instance: false,
            readonly: true,
            const_expr: true,
            value: Some(tdi.index().to_string()),
            brief_comment: Some("IL2CPP Metadata Type Index".into()),
            is_private: false,
        };

        self.declarations
            .push(CppMember::FieldDecl(il2cpp_metadata_type_index).into());
    }

    fn delete_default_ctor(&mut self) {
        let t = &self.cpp_name_components.name;

        let default_ctor = CppConstructorDecl {
            cpp_name: t.clone(),
            parameters: vec![],
            template: None,
            is_constexpr: false,
            is_explicit: false,
            is_default: false,
            is_no_except: false,
            is_delete: true,
            is_protected: false,
            base_ctor: None,
            initialized_values: Default::default(),
            brief: Some(
                "delete default ctor to prevent accidental value type instantiations of ref types"
                    .to_string(),
            ),
            body: None,
        };

        self.declarations
            .push(CppMember::ConstructorDecl(default_ctor).into());
    }

    fn create_ref_constructor(
        &mut self,
        declaring_type: &Il2CppTypeDefinition,
        m_params: &[CppParam],
        template: &Option<CppTemplate>,
    ) {
        if declaring_type.is_value_type() || declaring_type.is_enum_type() {
            return;
        }

        let params_no_default = m_params
            .iter()
            .cloned()
            .map(|mut c| {
                c.def_value = None;
                c
            })
            .collect_vec();

        let ty_full_cpp_name = self.cpp_name_components.combine_all();

        let decl: CppMethodDecl = CppMethodDecl {
            cpp_name: "New_ctor".into(),
            return_type: ty_full_cpp_name.clone(),
            parameters: params_no_default,
            template: template.clone(),
            body: None, // TODO:
            brief: None,
            is_no_except: false,
            is_constexpr: false,
            instance: false,
            is_const: false,
            is_implicit_operator: false,
            is_explicit_operator: false,

            is_virtual: false,
            is_inline: true,
            prefix_modifiers: vec![],
            suffix_modifiers: vec![],
        };

        // To avoid trailing ({},)
        let base_ctor_params = CppParam::params_names(&decl.parameters).join(", ");

        let allocate_call = format!(
            "THROW_UNLESS(::il2cpp_utils::NewSpecific<{ty_full_cpp_name}>({base_ctor_params}))"
        );

        let declaring_template = if self
            .cpp_template
            .as_ref()
            .is_some_and(|t| !t.names.is_empty())
        {
            self.cpp_template.clone()
        } else {
            None
        };

        let cpp_constructor_impl = CppMethodImpl {
            body: vec![Arc::new(CppLine::make(format!("return {allocate_call};")))],

            declaring_cpp_full_name: self.cpp_name_components.remove_pointer().combine_all(),
            parameters: m_params.to_vec(),
            template: declaring_template,
            ..decl.clone().into()
        };

        self.implementations
            .push(CppMember::MethodImpl(cpp_constructor_impl).into());

        self.declarations.push(CppMember::MethodDecl(decl).into());
    }

    pub fn get_inherits(&self) -> impl Iterator<Item = &String> {
        std::iter::once(&self.parent)
            .flatten()
            .chain(self.interfaces.iter())
    }

    pub fn cpp_namespace(&self) -> String {
        self.cpp_name_components
            .namespace
            .clone()
            .unwrap_or("GlobalNamespace".to_owned())
    }

    pub fn namespace(&self) -> String {
        self.cs_name_components
            .namespace
            .clone()
            .unwrap_or("GlobalNamespace".to_owned())
    }

    pub fn cpp_name(&self) -> &std::string::String {
        &self.cpp_name_components.name
    }

    pub fn name(&self) -> &String {
        &self.cpp_name_components.name
    }
}

///
/// This makes generic args for types such as ValueTask<List<T>> work
/// by recursively checking if any generic arg is a reference or numeric type (for enums)
///
fn parse_generic_arg(
    t: &Il2CppType,
    gen_name: String,
    cpp_type: &mut CppType,
    ctx_collection: &CppContextCollection,
    metadata: &CordlMetadata<'_>,
    template_args: &mut Vec<(String, String)>,
) -> NameComponents {
    // If reference type, we use a template and add a requirement
    if !t.valuetype {
        template_args.push((
            CORDL_REFERENCE_TYPE_CONSTRAINT.to_string(),
            gen_name.clone(),
        ));
        return gen_name.into();
    }

    /*
       mscorelib.xml
       <type fullname="System.SByteEnum" />
       <type fullname="System.Int16Enum" />
       <type fullname="System.Int32Enum" />
       <type fullname="System.Int64Enum" />

       <type fullname="System.ByteEnum" />
       <type fullname="System.UInt16Enum" />
       <type fullname="System.UInt32Enum" />
       <type fullname="System.UInt64Enum" />
    */
    let enum_system_type_discriminator = match t.data {
        TypeData::TypeDefinitionIndex(tdi) => {
            let td = &metadata.metadata.global_metadata.type_definitions[tdi];
            let namespace = td.namespace(metadata.metadata);
            let name = td.name(metadata.metadata);

            if namespace == "System" {
                match name {
                    "SByteEnum" => Some(Il2CppTypeEnum::I1),
                    "Int16Enum" => Some(Il2CppTypeEnum::I2),
                    "Int32Enum" => Some(Il2CppTypeEnum::I4),
                    "Int64Enum" => Some(Il2CppTypeEnum::I8),
                    "ByteEnum" => Some(Il2CppTypeEnum::U1),
                    "UInt16Enum" => Some(Il2CppTypeEnum::U2),
                    "UInt32Enum" => Some(Il2CppTypeEnum::U4),
                    "UInt64Enum" => Some(Il2CppTypeEnum::U8),
                    _ => None,
                }
            } else {
                None
            }
        }
        _ => None,
    };

    let inner_enum_type = enum_system_type_discriminator.map(|e| Il2CppType {
        attrs: u16::MAX,
        byref: false,
        data: TypeData::TypeIndex(usize::MAX),
        pinned: false,
        ty: e,
        valuetype: true,
    });

    // if int, int64 etc.
    // this allows for enums to be supported
    if let Some(inner_enum_type) = inner_enum_type {
        let inner_enum_type_cpp = cpp_type
            .cppify_name_il2cpp(
                ctx_collection,
                metadata,
                &inner_enum_type,
                0,
                TypeUsage::GenericArg,
            )
            .combine_all();

        template_args.push((
            format!("{CORDL_NUM_ENUM_TYPE_CONSTRAINT}<{inner_enum_type_cpp}>",),
            gen_name.clone(),
        ));

        return gen_name.into();
    }

    let inner_type =
        cpp_type.cppify_name_il2cpp(ctx_collection, metadata, t, 0, TypeUsage::TypeName);

    match t.data {
        TypeData::GenericClassIndex(gen_class_idx) => {
            let gen_class = &metadata.metadata_registration.generic_classes[gen_class_idx];
            let gen_class_ty = &metadata.metadata_registration.types[gen_class.type_index];
            let TypeData::TypeDefinitionIndex(gen_class_tdi) = gen_class_ty.data else {
                todo!()
            };
            let gen_class_td = &metadata.metadata.global_metadata.type_definitions[gen_class_tdi];

            let gen_container = gen_class_td.generic_container(metadata.metadata);

            let gen_class_inst = &metadata.metadata_registration.generic_insts
                [gen_class.context.class_inst_idx.unwrap()];

            // this relies on the fact TDIs do not include their generic params
            let non_generic_inner_type = cpp_type.cppify_name_il2cpp(
                ctx_collection,
                metadata,
                gen_class_ty,
                0,
                TypeUsage::GenericArg,
            );

            let inner_generic_params = gen_class_inst
                .types
                .iter()
                .enumerate()
                .map(|(param_idx, u)| {
                    let t = metadata.metadata_registration.types.get(*u).unwrap();
                    let gen_param = gen_container
                        .generic_parameters(metadata.metadata)
                        .iter()
                        .find(|p| p.num as usize == param_idx)
                        .expect("No generic param at this num");

                    (t, gen_param)
                })
                .map(|(t, gen_param)| {
                    let inner_gen_name = gen_param.name(metadata.metadata).to_owned();
                    let mangled_gen_name =
                        format!("{inner_gen_name}_cordlgen_{}", template_args.len());
                    parse_generic_arg(
                        t,
                        mangled_gen_name,
                        cpp_type,
                        ctx_collection,
                        metadata,
                        template_args,
                    )
                })
                .map(|n| n.combine_all())
                .collect_vec();

            NameComponents {
                generics: Some(inner_generic_params),
                ..non_generic_inner_type
            }
        }
        _ => inner_type,
    }
}
