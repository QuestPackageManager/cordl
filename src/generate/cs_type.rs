use std::{
    collections::{HashMap, HashSet},
    io::{Cursor, Write},
    rc::Rc,
};

use byteorder::ReadBytesExt;
use color_eyre::eyre::Context;

use brocolib::{
    global_metadata::{
        FieldIndex, Il2CppFieldDefinition, Il2CppTypeDefinition, MethodIndex, ParameterIndex,
        TypeDefinitionIndex, TypeIndex,
    },
    runtime_metadata::{Il2CppMethodSpec, Il2CppType, Il2CppTypeEnum, TypeData},
};
use itertools::Itertools;
use log::{debug, info, warn};

use crate::{
    data::name_components::NameComponents,
    generate::{cs_fields::FieldInfo, members::CsField, metadata::TypeUsage},
    helpers::cursor::ReadBytesExtensions,
    Endian,
};

use super::{
    config::GenerationConfig,
    context_collection::CppContextCollection,
    cs_fields::{
        handle_const_fields, handle_referencetype_fields, handle_static_fields,
        handle_valuetype_fields,
    },
    cs_type_tag::CsTypeTag,
    members::{
        CppInclude, CsMember, CsMethodData, CsMethodDecl, CsParam, CsPropertyDecl, CsValue,
        GenericTemplate,
    },
    metadata::Metadata,
    offsets::{self, SizeInfo},
    writer::{CppWritable, CppWriter, Sortable},
};

pub enum CsRequirementEnum {
    Wrapper,
    Int,
    Byte,
    Math,
    StringW,
    ArrayW,
    ByRef,
    EnumWrapper,
    ValueTypeWrapper,
}

#[derive(Debug, Clone, Default)]
pub struct CsTypeRequirements {
    // Lists both types we forward declare or include
    pub depending_types: HashSet<CsTypeTag>,
    // TODO: Bitflag
    pub others: HashSet<CsRequirementEnum>,
}

impl CsTypeRequirements {
    pub fn add_dependency(&mut self, ty: &CsType) {
        self.depending_types.insert(ty.self_tag);
    }
    pub fn add_dependency_tag(&mut self, tag: CsTypeTag) {
        self.depending_types.insert(tag);
    }
    pub fn add_requirement(&mut self, tag: CsRequirementEnum) {
        self.others.insert(tag);
    }
}

// Represents all of the information necessary for a C++ TYPE!
// A C# type will be TURNED INTO this
#[derive(Debug, Clone)]
pub struct CsType {
    pub self_tag: CsTypeTag,
    pub nested: bool,

    pub(crate) prefix_comments: Vec<String>,

    pub size_info: Option<SizeInfo>,
    pub packing: Option<u8>,

    // Computed by TypeDefinition.full_name()
    // Then fixed for generic types in CppContextCollection::make_generic_from/fill_generic_inst
    // pub cpp_name_components: NameComponents,
    pub cs_name_components: NameComponents,

    pub members: Vec<Rc<CsMember>>,

    pub is_value_type: bool,
    pub is_enum_type: bool,
    pub is_reference_type: bool,
    pub requirements: CsTypeRequirements,

    pub parent: Option<CsTypeTag>,
    pub interfaces: Vec<CsTypeTag>,
    pub generic_template: Option<GenericTemplate>, // Names of templates e.g T, TKey etc.

    /// contains the array of generic Il2CppType indexes
    pub generic_instantiations_args_types: Option<Vec<usize>>, // GenericArg -> Instantiation Arg
    pub method_generic_instantiation_map: HashMap<MethodIndex, Vec<TypeIndex>>, // MethodIndex -> Generic Args
    pub is_stub: bool,
    pub is_interface: bool,
    pub is_hidden: bool,

    pub nested_types: HashMap<CsTypeTag, CsType>,
}

impl CsType {
    pub fn namespace(&self) -> String {
        self.cs_name_components
            .namespace
            .clone()
            .unwrap_or_default()
    }

    pub fn name(&self) -> &String {
        &self.cs_name_components.name
    }

    pub fn nested_types_flattened(&self) -> HashMap<CsTypeTag, &CsType> {
        self.nested_types
            .iter()
            .flat_map(|(_, n)| n.nested_types_flattened())
            .chain(self.nested_types.iter().map(|(tag, n)| (*tag, n)))
            .collect()
    }
    pub fn get_nested_type_mut(&mut self, tag: CsTypeTag) -> Option<&mut CsType> {
        // sadly
        if self.nested_types.get_mut(&tag).is_some() {
            return self.nested_types.get_mut(&tag);
        }

        self.nested_types.values_mut().find_map(|n| {
            // Recurse
            n.get_nested_type_mut(tag)
        })
    }
    pub fn get_nested_type(&self, tag: CsTypeTag) -> Option<&CsType> {
        self.nested_types.get(&tag).or_else(|| {
            self.nested_types.iter().find_map(|(_, n)| {
                // Recurse
                n.get_nested_type(tag)
            })
        })
    }

    pub fn borrow_nested_type_mut<F>(
        &mut self,
        ty: CsTypeTag,
        context: &mut CppContextCollection,
        func: &F,
    ) -> bool
    where
        F: Fn(&mut CppContextCollection, CsType) -> CsType,
    {
        let nested_index = self.nested_types.get(&ty);

        match nested_index {
            None => {
                for nested_ty in self.nested_types.values_mut() {
                    if nested_ty.borrow_nested_type_mut(ty, context, func) {
                        return true;
                    }
                }

                false
            }
            Some(old_nested_self) => {
                // clone to avoid breaking il2cpp
                let old_nested_self_tag = old_nested_self.self_tag;
                let new_self = func(context, old_nested_self.clone());

                // Remove old type, which may have a new type tag
                self.nested_types.remove(&old_nested_self_tag);
                self.nested_types.insert(new_self.self_tag, new_self);

                true
            }
        }
    }

    fn get_tag_tdi(tag: TypeData) -> TypeDefinitionIndex {
        match tag {
            TypeData::TypeDefinitionIndex(tdi) => tdi,
            _ => panic!("Unsupported type: {tag:?}"),
        }
    }
    fn get_tag_tdi(tag: CsTypeTag) -> TypeDefinitionIndex {
        tag.into()
    }

    ////
    ///
    ///

    fn add_method_generic_inst(
        &mut self,
        method_spec: &Il2CppMethodSpec,
        metadata: &Metadata,
    ) -> &mut CsType {
        assert!(method_spec.method_inst_index != u32::MAX);

        let inst = metadata
            .metadata_registration
            .generic_insts
            .get(method_spec.method_inst_index as usize)
            .unwrap();

        self.method_generic_instantiation_map.insert(
            method_spec.method_definition_index,
            inst.types.iter().map(|t| *t as TypeIndex).collect(),
        );

        self
    }

    fn make_cs_type(
        metadata: &Metadata,
        config: &GenerationConfig,
        tdi: TypeDefinitionIndex,
        tag: CsTypeTag,
        generic_inst_types: Option<&Vec<usize>>,
    ) -> Option<CsType> {
        // let iface = metadata.interfaces.get(t.interfaces_start);
        // Then, handle interfaces

        // Then, handle methods
        // - This includes constructors
        // inherited methods will be inherited

        let t = &metadata.metadata.global_metadata.type_definitions[tdi];

        // Generics
        // This is a generic type def
        // TODO: Constraints!
        let generics = t.generic_container_index.is_valid().then(|| {
            t.generic_container(metadata.metadata)
                .generic_parameters(metadata.metadata)
                .iter()
                .collect_vec()
        });

        let cpp_template = generics.as_ref().map(|g| {
            GenericTemplate::make_typenames(g.iter().map(|g| g.name(metadata.metadata).to_string()))
        });

        let ns = t.namespace(metadata.metadata);
        let name = t.name(metadata.metadata);
        let full_name = t.full_name(metadata.metadata, false);

        if metadata.blacklisted_types.contains(&tdi) {
            info!("Skipping {full_name} ({tdi:?}) because it's blacklisted");

            return None;
        }

        // all nested types are unnested
        let nested = false; // t.declaring_type_index != u32::MAX;
        let cs_name_components = t.get_name_components(metadata.metadata);

        let is_pointer = cs_name_components.is_pointer;

        let cpp_name_components = NameComponents {
            declaring_types: cs_name_components
                .declaring_types
                .as_ref()
                .map(|declaring_types| {
                    declaring_types
                        .iter()
                        .map(|s| config.name_cpp(s))
                        .collect_vec()
                }),
            generics: cs_name_components.generics.clone(),
            name: config.name_cpp(&cs_name_components.name),
            namespace: cs_name_components
                .namespace
                .as_ref()
                .map(|s| config.namespace_cpp(s)),
            is_pointer,
        };

        // TODO: Come up with a way to avoid this extra call to layout the entire type
        // We really just want to call it once for a given size and then move on
        // Every type should have a valid metadata size, even if it is 0
        let size_info: offsets::SizeInfo =
            offsets::get_size_info(t, tdi, generic_inst_types, metadata);

        // best results of cordl are when specified packing is strictly what is used, but experimentation may be required
        let packing = size_info.specified_packing;

        // Modified later for nested types
        let mut cpptype = CsType {
            self_tag: tag,
            nested,
            prefix_comments: vec![format!("Type: {ns}::{name}"), format!("{size_info:?}")],

            size_info: Some(size_info),
            packing,

            cs_name_components,

            members: Default::default(),

            is_value_type: t.is_value_type(),
            is_enum_type: t.is_enum_type(),
            is_reference_type: is_pointer,
            requirements: Default::default(),

            interfaces: Default::default(),
            parent: Default::default(),

            is_interface: t.is_interface(),
            generic_template: cpp_template,

            generic_instantiations_args_types: generic_inst_types.cloned(),
            method_generic_instantiation_map: Default::default(),

            is_stub: false,
            is_hidden: true,
            nested_types: Default::default(),
        };

        if cpptype.generic_instantiations_args_types.is_some() {
            cpptype.fixup_into_generic_instantiation();
        }

        // Nested type unnesting fix
        if t.declaring_type_index != u32::MAX {
            let declaring_ty = &metadata
                .metadata
                .runtime_metadata
                .metadata_registration
                .types[t.declaring_type_index as usize];

            let declaring_tag = CsTypeTag::from_type_data(declaring_ty.data, metadata.metadata);
            let declaring_tdi: TypeDefinitionIndex = declaring_tag.into();
            let declaring_td = &metadata.metadata.global_metadata.type_definitions[declaring_tdi];
        }

        if t.parent_index == u32::MAX {
            if !t.is_interface() && t.full_name(metadata.metadata, true) != "System.Object" {
                info!("Skipping type: {ns}::{name} because it has parent index: {} and is not an interface!", t.parent_index);
                return None;
            }
        } else if metadata
            .metadata_registration
            .types
            .get(t.parent_index as usize)
            .is_none()
        {
            panic!("NO PARENT! But valid index found: {}", t.parent_index);
        }

        Some(cpptype)
    }

    fn fill_from_il2cpp(
        &mut self,
        metadata: &Metadata,
        config: &GenerationConfig,
        ctx_collection: &CppContextCollection,
    ) {
        if self.get_self().is_stub {
            // Do not fill stubs
            return;
        }

        let tdi: TypeDefinitionIndex = self.get_self().self_tag.into();

        let t = &metadata.metadata.global_metadata.type_definitions[tdi];

        self.make_generics_args(metadata, ctx_collection, tdi);
        self.make_parents(metadata, ctx_collection, tdi);
        self.make_interfaces(metadata, ctx_collection, config, tdi);

        // we depend on parents and generic args here
        // default ctor
        if t.is_value_type() || t.is_enum_type() {
            self.create_valuetype_constructor(metadata, ctx_collection, config, tdi);
            self.create_valuetype_field_wrapper();
            if t.is_enum_type() {
                self.create_enum_wrapper(metadata, ctx_collection, tdi);
                self.create_enum_backing_type_constant(metadata, ctx_collection, tdi);
            }
            self.add_default_ctor(false);
        } else if t.is_interface() {
            // self.make_interface_constructors();
            self.delete_move_ctor();
            self.delete_copy_ctor();
            // self.delete_default_ctor();
        } else {
            // ref type
            self.delete_move_ctor();
            self.delete_copy_ctor();
            self.add_default_ctor(true);
            // self.delete_default_ctor();
        }

        if !t.is_interface() {
            self.create_size_assert();
        }

        self.add_type_index_member();

        self.make_nested_types(metadata, ctx_collection, config, tdi);
        self.make_fields(metadata, ctx_collection, config, tdi);
        self.make_properties(metadata, ctx_collection, config, tdi);
        self.make_methods(metadata, config, ctx_collection, tdi);

        if !t.is_interface() {
            self.create_size_padding(metadata, tdi);
        }

        if let Some(func) = metadata.custom_type_handler.get(&tdi) {
            func({
                let this = &mut *self;
                this
            })
        }
    }


    fn make_parameters(
        &mut self,
        method: &brocolib::global_metadata::Il2CppMethodDefinition,
        method_index: MethodIndex,
        is_generic_method_inst: bool,
        metadata: &Metadata<'_>,
        config: &GenerationConfig,
        ctx_collection: &CppContextCollection,
    ) -> Vec<CsParam> {
        method
            .parameters(metadata.metadata)
            .iter()
            .enumerate()
            .map(|(pi, param)| {
                let param_index = ParameterIndex::new(method.parameter_start.index() + pi as u32);

                self.make_parameter(
                    param,
                    method_index,
                    param_index,
                    is_generic_method_inst,
                    metadata,
                    config,
                    ctx_collection,
                )
            })
            .collect()
    }

    fn make_parameter(
        &mut self,
        param: &brocolib::global_metadata::Il2CppParameterDefinition,
        method_index: MethodIndex,
        param_index: ParameterIndex,
        is_generic_method_inst: bool,
        metadata: &Metadata<'_>,
        config: &GenerationConfig,
        ctx_collection: &CppContextCollection,
    ) -> CsParam {
        let param_type = metadata
            .metadata_registration
            .types
            .get(param.type_index as usize)
            .unwrap();

        let def_value = Self::param_default_value(metadata, param_index);

        CsParam {
            name: config.name_cpp(param.name(metadata.metadata)),
            def_value,
            il2cpp_ty: param_type.clone(),
            modifiers: Default::default(),
        }
    }

    fn make_methods(
        &mut self,
        metadata: &Metadata,
        config: &GenerationConfig,
        ctx_collection: &CppContextCollection,
        tdi: TypeDefinitionIndex,
    ) {
        let t = Self::get_type_definition(metadata, tdi);

        // Then, handle methods
        if t.method_count > 0 {
            // 2 because each method gets a method struct and method decl
            // a constructor will add an additional one for each
            self.members.reserve(2 * (t.method_count as usize + 1));
            self.implementations.reserve(t.method_count as usize + 1);

            // Then, for each method, write it out
            for (i, _method) in t.methods(metadata.metadata).iter().enumerate() {
                let method_index = MethodIndex::new(t.method_start.index() + i as u32);
                self.create_method(t, method_index, metadata, ctx_collection, config, false);
            }
        }
    }

    fn make_fields(
        &mut self,
        metadata: &Metadata,
        ctx_collection: &CppContextCollection,
        config: &GenerationConfig,
        tdi: TypeDefinitionIndex,
    ) {
        let t = Self::get_type_definition(metadata, tdi);

        // if no fields, skip
        if t.field_count == 0 {
            return;
        }

        let field_offsets = &metadata
            .metadata_registration
            .field_offsets
            .as_ref()
            .unwrap()[tdi.index() as usize];

        let mut offsets = Vec::<u32>::new();
        if let Some(sz) = offsets::get_size_of_type_table(metadata, tdi) {
            if sz.instance_size == 0 {
                // At this point we need to compute the offsets
                debug!(
                    "Computing offsets for TDI: {:?}, as it has a size of 0",
                    tdi
                );
                let _resulting_size = offsets::layout_fields(
                    metadata,
                    t,
                    tdi,
                    self.generic_instantiations_args_types.as_ref(),
                    Some(&mut offsets),
                    false,
                );
            }
        }
        let mut offset_iter = offsets.iter();

        let get_offset = |field: &Il2CppFieldDefinition, i: usize, iter| {
            let f_type = metadata
                .metadata_registration
                .types
                .get(field.type_index as usize)
                .unwrap();
            let f_name = field.name(metadata.metadata);

            match f_type.is_static() || f_type.is_constant() {
                // return u32::MAX for static fields as an "invalid offset" value
                true => None,
                false => Some({
                    // If we have a hotfix offset, use that instead
                    // We can safely assume this always returns None even if we "next" past the end
                    let offset = if let Some(computed_offset) = iter.next() {
                        *computed_offset
                    } else {
                        field_offsets[i]
                    };

                    if offset < metadata.object_size() as u32 {
                        warn!("Field {f_name} ({offset:x}) of {} is smaller than object size {:x} is value type {}",
                            t.full_name(metadata.metadata, true),
                            metadata.object_size(),
                            t.is_value_type() || t.is_enum_type()
                        );
                    }

                    // TODO: Is the offset supposed to be smaller than object size for fixups?
                    match t.is_value_type() && offset >= metadata.object_size() as u32 {
                        true => {
                            // value type fixup
                            offset - metadata.object_size() as u32
                        }
                        false => offset,
                    }
                }),
            }
        };

        fn get_size(
            field: &Il2CppFieldDefinition,
            gen_args: Option<&Vec<usize>>,
            metadata: &&Metadata<'_>,
        ) -> usize {
            let f_type = metadata
                .metadata_registration
                .types
                .get(field.type_index as usize)
                .unwrap();

            let sa = offsets::get_il2cpptype_sa(*metadata, f_type, gen_args);

            sa.size
        }

        let fields = t
            .fields(metadata.metadata)
            .iter()
            .enumerate()
            .filter_map(|(i, field)| {
                let f_type = metadata
                    .metadata_registration
                    .types
                    .get(field.type_index as usize)
                    .unwrap();

                let field_index = FieldIndex::new(t.field_start.index() + i as u32);
                let f_name = field.name(metadata.metadata);

                let f_cpp_name = config.name_cpp_plus(f_name, &[self.cpp_name().as_str()]);

                let f_offset = get_offset(field, i, &mut offset_iter);

                // calculate / fetch the field size
                let f_size = if let Some(generics) = &self.generic_instantiations_args_types {
                    get_size(field, Some(generics), &metadata)
                } else {
                    get_size(field, None, &metadata)
                };

                if let TypeData::TypeDefinitionIndex(field_tdi) = f_type.data
                    && metadata.blacklisted_types.contains(&field_tdi)
                {
                    if !self.is_value_type && !self.is_enum_type {
                        return None;
                    }
                    warn!("Value type uses {tdi:?} which is blacklisted! TODO");
                }

                // Var types are default pointers so we need to get the name component's pointer bool
                let (field_ty_cpp_name, field_is_pointer) =
                    if f_type.is_constant() && f_type.ty == Il2CppTypeEnum::String {
                        ("::ConstString".to_string(), false)
                    } else {
                        let include_depth = match f_type.valuetype {
                            true => usize::MAX,
                            false => 0,
                        };

                        let field_name_components = self.cppify_name_il2cpp(
                            ctx_collection,
                            metadata,
                            f_type,
                            include_depth,
                            TypeUsage::FieldName
                        );

                        (
                            field_name_components.combine_all(),
                            field_name_components.is_pointer,
                        )
                    };

                // TODO: Check a flag to look for default values to speed this up
                let def_value = Self::field_default_value(metadata, field_index);

                assert!(def_value.is_none() || (def_value.is_some() && f_type.is_param_optional()));

                let cpp_field_decl = CsField {
                    name: f_cpp_name,
                    field_ty: f_type.clone(),
                    offset: f_offset,
                    instance: !f_type.is_static() && !f_type.is_constant(),
                    readonly: f_type.is_constant(),
                    brief_comment: Some(format!("Field {f_name}, offset: 0x{:x}, size: 0x{f_size:x}, def value: {def_value:?}", f_offset.unwrap_or(u32::MAX))),
                    value: def_value,
                    const_expr: false,
                    is_private: false,
                };

                Some(FieldInfo {
                    cpp_field: cpp_field_decl,
                    field,
                    field_type: f_type,
                    is_constant: f_type.is_constant(),
                    is_static: f_type.is_static(),
                    is_pointer: field_is_pointer,
                    offset: f_offset,
                    size: f_size,
                })
            })
            .collect_vec();

        for field_info in fields.iter() {
            let f_type = field_info.field_type;

            // only push def dependency if valuetype field & not a primitive builtin
            if f_type.valuetype && !f_type.ty.is_primitive_builtin() {
                let field_cpp_tag: CsTypeTag =
                    CsTypeTag::from_type_data(f_type.data, metadata.metadata);
                let field_cpp_td_tag: CsTypeTag = field_cpp_tag.get_tdi().into();
                let field_self = ctx_collection.get_self(field_cpp_td_tag);

                if field_self.is_some() {
                    let field_cpp_context = ctx_collection
                        .get_context(field_cpp_td_tag)
                        .expect("No context for cpp value type");

                    self.requirements.add_def_include(
                        field_self,
                        CppInclude::new_context_typedef(field_cpp_context),
                    );

                    self.requirements.add_impl_include(
                        field_self,
                        CppInclude::new_context_typeimpl(field_cpp_context),
                    );
                }
            }
        }

        if t.is_value_type() || t.is_enum_type() {
            handle_valuetype_fields(self, &fields, metadata, tdi);
        } else {
            handle_referencetype_fields(self, &fields, metadata, tdi);
        }

        handle_static_fields(self, &fields, metadata, tdi);
        handle_const_fields(self, &fields, ctx_collection, metadata, tdi);
    }

    fn make_parents(
        &mut self,
        metadata: &Metadata,
        ctx_collection: &CppContextCollection,
        tdi: TypeDefinitionIndex,
    ) {
        let t = &metadata.metadata.global_metadata.type_definitions[tdi];

        let ns = t.namespace(metadata.metadata);
        let name = t.name(metadata.metadata);

        if t.parent_index == u32::MAX {
            // TYPE_ATTRIBUTE_INTERFACE = 0x00000020
            match t.is_interface() {
                true => {
                    // FIXME: should interfaces have a base type? I don't think they need to
                    // self.inherit.push(INTERFACE_WRAPPER_TYPE.to_string());
                }
                false => {
                    info!("Skipping type: {ns}::{name} because it has parent index: {} and is not an interface!", t.parent_index);
                }
            }
            return;
        }

        let parent_type = metadata
            .metadata_registration
            .types
            .get(t.parent_index as usize)
            .unwrap_or_else(|| panic!("NO PARENT! But valid index found: {}", t.parent_index));

        let parent_ty: CsTypeTag = CsTypeTag::from_type_data(parent_type.data, metadata.metadata);

        // handle value types and enum types specially
        match t.is_value_type() || t.is_enum_type() {
            // parent will be a value wrapper type
            // which will have the size
            // OF THE TYPE ITSELF, NOT PARENT
            true => {
                // let Some(size_info) = &self.size_info else {
                //     panic!("No size for value/enum type!")
                // };

                // if t.is_enum_type() {
                //     self.requirements.needs_enum_include();
                // } else if t.is_value_type() {
                //     self.requirements.needs_value_include();
                // }

                // let wrapper = wrapper_type_for_tdi(t);

                // self.inherit.push(wrapper.to_string());
            }
            // handle as reference type
            false => {
                // make sure our parent is intended\
                let is_ref_type = matches!(
                    parent_type.ty,
                    Il2CppTypeEnum::Class | Il2CppTypeEnum::Genericinst | Il2CppTypeEnum::Object
                );
                assert!(is_ref_type, "Not a class, object or generic inst!");

                // We have a parent, lets do something with it
                let inherit_type = self.cppify_name_il2cpp(
                    ctx_collection,
                    metadata,
                    parent_type,
                    usize::MAX,
                    TypeUsage::TypeName,
                );

                if is_ref_type {
                    // TODO: Figure out why some generic insts don't work here
                    let parent_tdi: TypeDefinitionIndex = parent_ty.into();

                    let base_type_context = ctx_collection
                                    .get_context(parent_ty)
                                    .or_else(|| ctx_collection.get_context(parent_tdi.into()))
                                    .unwrap_or_else(|| {
                                        panic!(
                                        "No CppContext for base type {inherit_type:?}. Using tag {parent_ty:?}"
                                    )
                                    });

                    let base_type_self = ctx_collection
                                        .get_self(parent_ty)
                                        .or_else(|| ctx_collection.get_self(parent_tdi.into()))
                                        .unwrap_or_else(|| {
                                panic!(
                                    "No CppType for base type {inherit_type:?}. Using tag {parent_ty:?}"
                                )
                            });

                    self.requirements.add_impl_include(
                        Some(base_type_self),
                        CppInclude::new_context_typeimpl(base_type_context),
                    )
                }

                self.inherit
                    .push(inherit_type.remove_pointer().combine_all());
            }
        }
    }

    fn make_interfaces(
        &mut self,
        metadata: &Metadata<'_>,
        ctx_collection: &CppContextCollection,
        config: &GenerationConfig,
        tdi: TypeDefinitionIndex,
    ) {
        let t = &metadata.metadata.global_metadata.type_definitions[tdi];

        for &interface_index in t.interfaces(metadata.metadata) {
            let int_ty = &metadata.metadata_registration.types[interface_index as usize];

            self.interfaces
                .push(CsTypeTag::from_type_data(int_ty.data, metadata));
        }
    }

    fn make_nested_types(
        &mut self,
        metadata: &Metadata,
        ctx_collection: &CppContextCollection,
        config: &GenerationConfig,
        tdi: TypeDefinitionIndex,
    ) {
        let t = &metadata.metadata.global_metadata.type_definitions[tdi];

        if t.nested_type_count == 0 {
            return;
        }

        let aliases = t
            .nested_types(metadata.metadata)
            .iter()
            .filter(|t| !metadata.blacklisted_types.contains(t))
            .map(|nested_tdi| {
                let nested_td = &metadata.metadata.global_metadata.type_definitions[*nested_tdi];
                let nested_tag = CsTypeTag::TypeDefinitionIndex(*nested_tdi);

                let nested_context = ctx_collection
                    .get_context(nested_tag)
                    .expect("Unable to find CppContext");
                let nested = ctx_collection
                    .get_self(nested_tag)
                    .expect("Unable to find nested CppType");

                let alias = CppUsingAlias::from_self(
                    config.name_cpp(nested_td.name(metadata.metadata)),
                    nested,
                    generic_instantiation_args.clone(),
                    // if no generic args are made, we can do the generic fixup
                    // ORDER OF PASSES MATTERS
                    nested.generic_instantiations_args_types.is_none(),
                );
                let fd = CppForwardDeclare::from_self(nested);
                let inc = CppInclude::new_context_typedef(nested_context);

                (alias, fd, inc)
            })
            .collect_vec();

        for (alias, fd, inc) in aliases {
            self.members
                .insert(0, CsMember::CppUsingAlias(alias).into());
            self.requirements.add_forward_declare((fd, inc));
        }
    }

    fn make_properties(
        &mut self,
        metadata: &Metadata,
        ctx_collection: &CppContextCollection,
        config: &GenerationConfig,
        tdi: TypeDefinitionIndex,
    ) {
        let t = Self::get_type_definition(metadata, tdi);

        // Then, handle properties
        if t.property_count == 0 {
            return;
        }

        self.members.reserve(t.property_count as usize);
        // Then, for each field, write it out
        for prop in t.properties(metadata.metadata) {
            let p_name = prop.name(metadata.metadata);
            let p_setter = (prop.set != u32::MAX).then(|| prop.set_method(t, metadata.metadata));
            let p_getter = (prop.get != u32::MAX).then(|| prop.get_method(t, metadata.metadata));

            // if this is a static property, skip emitting a cpp property since those can't be static
            if p_getter.or(p_setter).unwrap().is_static_method() {
                continue;
            }

            let p_type_index = match p_getter {
                Some(g) => g.return_type as usize,
                None => p_setter.unwrap().parameters(metadata.metadata)[0].type_index as usize,
            };

            let p_type = metadata
                .metadata_registration
                .types
                .get(p_type_index)
                .unwrap();

            let p_ty_cpp_name = self
                .cppify_name_il2cpp(ctx_collection, metadata, p_type, 0, TypeUsage::PropertyName)
                .combine_all();

            let _method_map = |p: MethodIndex| {
                let method_calc = metadata.method_calculations.get(&p).unwrap();
                CsMethodData {
                    estimated_size: method_calc.estimated_size,
                    addrs: method_calc.addrs,
                }
            };

            let _abstr = p_getter.is_some_and(|p| p.is_abstract_method())
                || p_setter.is_some_and(|p| p.is_abstract_method());

            let index = p_getter.is_some_and(|p| p.parameter_count > 0);

            // Need to include this type
            self.members.push(
                CsMember::Property(CsPropertyDecl {
                    cpp_name: config.name_cpp(p_name),
                    prop_ty: p_ty_cpp_name.clone(),
                    // methods generated in make_methods
                    setter: p_setter.map(|m| config.name_cpp(m.name(metadata.metadata))),
                    getter: p_getter.map(|m| config.name_cpp(m.name(metadata.metadata))),
                    indexable: index,
                    brief_comment: None,
                    instance: true,
                })
                .into(),
            );
        }
    }

    fn create_method(
        &mut self,
        declaring_type: &Il2CppTypeDefinition,
        method_index: MethodIndex,

        metadata: &Metadata,
        ctx_collection: &CppContextCollection,
        config: &GenerationConfig,
        is_generic_method_inst: bool,
    ) {
        let method = &metadata.metadata.global_metadata.methods[method_index];

        // TODO: sanitize method name for c++
        let m_name = method.name(metadata.metadata);
        if m_name == ".cctor" {
            // info!("Skipping {}", m_name);
            return;
        }

        let m_ret_type = metadata
            .metadata_registration
            .types
            .get(method.return_type as usize)
            .unwrap();

        let m_params_with_def: Vec<CsParam> = self.make_parameters(
            method,
            method_index,
            is_generic_method_inst,
            metadata,
            config,
            ctx_collection,
        );

        let m_params_no_def: Vec<CsParam> = m_params_with_def
            .iter()
            .cloned()
            .map(|mut p| {
                p.def_value = None;
                p
            })
            .collect_vec();

        // TODO: Add template<typename ...> if a generic inst e.g
        // T UnityEngine.Component::GetComponent<T>() -> bs_hook::Il2CppWrapperType UnityEngine.Component::GetComponent()
        let template =
            method
                .generic_container_index
                .is_valid()
                .then(|| match is_generic_method_inst {
                    true => Some(GenericTemplate { names: vec![] }),
                    false => {
                        let generics = method
                            .generic_container(metadata.metadata)
                            .unwrap()
                            .generic_parameters(metadata.metadata)
                            .iter()
                            .map(|param| param.name(metadata.metadata).to_string());

                        Some(GenericTemplate::make_typenames(generics))
                    }
                });

        let declaring_type_template = self
            .generic_template
            .as_ref()
            .is_some_and(|t| !t.names.is_empty())
            .then(|| self.generic_template.clone());

        let literal_types = is_generic_method_inst
            .then(|| {
                self.method_generic_instantiation_map
                    .get(&method_index)
                    .cloned()
            })
            .flatten();

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

        // Reference type constructor
        if m_name == ".ctor" {
            Self::create_ref_constructor(self, declaring_type, &m_params_with_def, &template);
        };

        let declaring_type = method.declaring_type(metadata.metadata);
        let tag = CsTypeTag::TypeDefinitionIndex(method.declaring_type);

        let method_calc = metadata.method_calculations.get(&method_index);

        // generic methods don't have definitions if not an instantiation
        let method_stub = !is_generic_method_inst && template.is_some();

        let method_decl = CsMethodDecl {
            brief: format!(
                "Method {m_name}, addr 0x{:x}, size 0x{:x}, virtual {}, abstract: {}, final {}",
                method_calc.map(|m| m.addrs).unwrap_or(u64::MAX),
                method_calc.map(|m| m.estimated_size).unwrap_or(usize::MAX),
                method.is_virtual_method(),
                method.is_abstract_method(),
                method.is_final_method()
            )
            .into(),
            name: m_name.to_string(),
            return_type: m_ret_type.clone(),
            parameters: m_params_no_def.clone(),
            instance: !method.is_static_method(),
            template: template.clone(),
            method_data: None,
        };

        let instance_ptr: String = if method.is_static_method() {
            "nullptr".into()
        } else {
            "this".into()
        };

        const METHOD_INFO_VAR_NAME: &str = "___internal_method";

        // instance methods should resolve slots if this is an interface, or if this is a virtual/abstract method, and not a final method
        // static methods can't be virtual or interface anyway so checking for that here is irrelevant
        let should_resolve_slot = self.is_interface
            || ((method.is_virtual_method() || method.is_abstract_method())
                && !method.is_final_method());

        // check if declaring type is the current type or the interface
        // we check TDI because if we are a generic instantiation
        // we just use ourselves if the declaring type is also the same TDI
        let interface_declaring_self: Option<&CsType> = if tag.get_tdi() == self.self_tag.get_tdi()
        {
            Some(self)
        } else {
            ctx_collection.get_self(tag)
        };

        // don't emit method size structs for generic methods

        // don't emit method size structs for generic methods

        // if type is a generic
        let has_template_args = self
            .generic_template
            .as_ref()
            .is_some_and(|t| !t.names.is_empty());

        // don't emit method size structs for generic methods
        if let Some(method_calc) = method_calc
            && template.is_none()
            && !has_template_args
            && !is_generic_method_inst
        {
            method_decl.method_data = Some(CsMethodData {
                addrs: method_calc.addrs,
                estimated_size: method_calc.estimated_size,
            })
        }

        if !is_generic_method_inst {
            self.members.push(CsMember::MethodDecl(method_decl).into());
        }
    }

    fn default_value_blob(
        metadata: &Metadata,
        ty: &Il2CppType,
        data_index: usize,
        string_quotes: bool,
        string_as_u16: bool,
    ) -> CsValue {
        let data = &metadata
            .metadata
            .global_metadata
            .field_and_parameter_default_value_data
            .as_vec()[data_index..];

        let mut cursor = Cursor::new(data);

        const UNSIGNED_SUFFIX: &str = "u";
        match ty.ty {
            Il2CppTypeEnum::Boolean => (if data[0] == 0 { "false" } else { "true" }).to_string(),
            Il2CppTypeEnum::I1 => CsValue::Num(cursor.read_i8().unwrap()),
            Il2CppTypeEnum::I2 => CsValue::Num(cursor.read_i16::<Endian>().unwrap()),
            Il2CppTypeEnum::I4 => CsValue::Num(cursor.read_compressed_i32::<Endian>().unwrap()),
            // TODO: We assume 64 bit
            Il2CppTypeEnum::I | Il2CppTypeEnum::I8 => {
                CsValue::Num(cursor.read_i64::<Endian>().unwrap())
            }
            Il2CppTypeEnum::U1 => CsValue::Num(cursor.read_u8::<Endian>().unwrap()),
            Il2CppTypeEnum::U2 => CsValue::Num(cursor.read_u16::<Endian>().unwrap()),
            Il2CppTypeEnum::U4 => CsValue::Num(cursor.read_u32::<Endian>().unwrap()),
            // TODO: We assume 64 bit
            Il2CppTypeEnum::U | Il2CppTypeEnum::U8 => {
                CsValue::Num(cursor.read_u64::<Endian>().unwrap())
            }
            // https://learn.microsoft.com/en-us/nimbusml/concepts/types
            // https://en.cppreference.com/w/cpp/types/floating-point
            Il2CppTypeEnum::R4 => CsValue::FloatingNum(cursor.read_f32::<Endian>().unwrap()),
            Il2CppTypeEnum::R8 => CsValue::FloatingNum(cursor.read_f64::<Endian>().unwrap()),
            Il2CppTypeEnum::Char => {
                let res = String::from_utf16_lossy(&[cursor.read_u16::<Endian>().unwrap()])
                    .escape_default()
                    .to_string();

                CsValue::String(res)
            }
            Il2CppTypeEnum::String => {
                let stru16_len = cursor.read_compressed_i32::<Endian>().unwrap();
                if stru16_len == -1 {
                    return "".to_string();
                }

                let mut buf = vec![0u8; stru16_len as usize];

                cursor.read_exact(buf.as_mut_slice()).unwrap();

                let res = String::from_utf8(buf).unwrap().escape_default().to_string();

                CsValue::String(res)
            }
            Il2CppTypeEnum::Genericinst
            | Il2CppTypeEnum::Byref
            | Il2CppTypeEnum::Ptr
            | Il2CppTypeEnum::Array
            | Il2CppTypeEnum::Object
            | Il2CppTypeEnum::Class
            | Il2CppTypeEnum::Szarray => {
                // let def = Self::type_default_value(metadata, None, ty);
                // format!("/* TODO: Fix these default values */ {ty:?} */ {def}")
                CsValue::Null
            }

            _ => todo!("Unsupported blob type {:#?}", ty),
        }
    }

    fn unbox_nullable_valuetype<'a>(metadata: &'a Metadata, ty: &'a Il2CppType) -> &'a Il2CppType {
        if let Il2CppTypeEnum::Valuetype = ty.ty {
            match ty.data {
                TypeData::TypeDefinitionIndex(tdi) => {
                    let type_def = &metadata.metadata.global_metadata.type_definitions[tdi];

                    // System.Nullable`1
                    if type_def.name(metadata.metadata) == "Nullable`1"
                        && type_def.namespace(metadata.metadata) == "System"
                    {
                        return metadata
                            .metadata_registration
                            .types
                            .get(type_def.byval_type_index as usize)
                            .unwrap();
                    }
                }
                _ => todo!(),
            }
        }

        ty
    }

    fn field_default_value(metadata: &Metadata, field_index: FieldIndex) -> Option<CsValue> {
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

                Self::default_value_blob(metadata, ty, def.data_index.index() as usize, true, true)
            })
    }
    fn param_default_value(metadata: &Metadata, parameter_index: ParameterIndex) -> Option<String> {
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

                ty = Self::unbox_nullable_valuetype(metadata, ty);

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

                Self::default_value_blob(metadata, ty, def.data_index.index() as usize, true, true)
            })
    }

    pub fn get_type_definition<'a>(
        metadata: &'a Metadata,
        tdi: TypeDefinitionIndex,
    ) -> &'a Il2CppTypeDefinition {
        &metadata.metadata.global_metadata.type_definitions[tdi]
    }
}
