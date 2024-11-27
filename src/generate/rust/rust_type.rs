use std::{collections::HashSet, sync::Arc};

use color_eyre::eyre::Result;
use itertools::Itertools;

use crate::{
    data::{
        name_components::NameComponents,
        type_resolver::{ResolvedType, TypeUsage},
    },
    generate::{
        cs_members::CsField,
        cs_type::CsType,
        cs_type_tag::CsTypeTag,
        offsets::SizeInfo,
        writer::{Writable, Writer},
    },
};

use super::{
    config::RustGenerationConfig,
    rust_members::{RustField, RustFunction, RustTrait, Visibility},
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
        todo!()
    }

    pub(crate) fn needs_byref_const_include(&mut self) {
        todo!()
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

    pub generics: Option<Vec<String>>,
    pub cs_name_components: NameComponents,
    pub rs_name_components: RustNameComponents,
    pub(crate) prefix_comments: Vec<String>,

    pub requirements: RustTypeRequirements,
    pub packing: Option<u32>,
    pub size_info: Option<SizeInfo>,
}
impl RustType {
    pub(crate) fn make_rust_type(
        tag: CsTypeTag,
        cs_type: &CsType,
        config: &RustGenerationConfig,
    ) -> Self {
        let cs_name_components = &cs_type.cs_name_components;

        let rs_name_components = RustNameComponents {
            declaring_types: None,
            generics: cs_name_components.generics.clone(),
            name: config.name_rs(&cs_name_components.name),
            namespace: cs_name_components
                .namespace
                .as_ref()
                .map(|s| config.namespace_rs(s)),
        };

        RustType {
            fields: Default::default(),
            methods: Default::default(),
            traits: Default::default(),

            is_value_type: cs_type.is_value_type,
            is_enum_type: cs_type.is_enum_type,
            is_reference_type: cs_type.is_reference_type,
            is_interface: cs_type.is_interface,

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
    }

    fn make_parent(
        &mut self,
        parent: Option<&ResolvedType>,
        name_resolver: &RustNameResolver<'_, '_>,
    ) {
        if let Some(parent) = parent {
            let parent = name_resolver.resolve_name(self, parent, TypeUsage::TypeName, true);
            let parent_field = RustField {
                name: PARENT_FIELD.to_string(),
                field_type: parent.combine_all(),
                visibility: Visibility::Private,
            };

            self.fields.push(parent_field);
        }
    }

    fn make_fields(
        &mut self,
        fields: &[CsField],
        name_resolver: &RustNameResolver,
        config: &RustGenerationConfig,
    ) {
        for f in fields {
            if !f.instance || f.is_const {
                continue;
            }
            let field_type = name_resolver.resolve_name(self, &f.field_ty, TypeUsage::Field, true);

            let rust_field = RustField {
                name: config.name_rs(&f.name),
                field_type: field_type.combine_all(),
                visibility: Visibility::Public,
            };
            self.fields.push(rust_field);
        }
    }

    pub fn name(&self) -> &String {
        &self.cs_name_components.name
    }

    pub fn namespace(&self) -> &str {
        self.cs_name_components
            .namespace
            .as_deref()
            .unwrap_or("GlobalNamespace")
    }

    pub fn rs_name(&self) -> &String {
        &self.rs_name_components.name
    }
    pub fn rs_namespace(&self) -> &Option<String> {
        &self.rs_name_components.namespace
    }

    pub(crate) fn write(&self, writer: &mut Writer, config: &RustGenerationConfig) -> Result<()> {
        

        writeln!(writer, "#[repr(c)]")?;
        writeln!(writer, "pub struct {name} {{", name = self.rs_name())?;
        for f in &self.fields {
            f.write(writer)?;
        }
        writeln!(writer, "}}")?;

        Ok(())
    }
}
