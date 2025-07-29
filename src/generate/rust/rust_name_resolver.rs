use brocolib::{global_metadata::Il2CppTypeDefinition, runtime_metadata::Il2CppTypeEnum};
use itertools::Itertools;

use crate::{
    data::type_resolver::{ResolvedType, ResolvedTypeData, TypeUsage},
    generate::{cs_type_tag::CsTypeTag, metadata::CordlMetadata, rust::rust_type::RustTypeRequirement},
};

use super::{
    config::RustGenerationConfig, rust_context_collection::RustContextCollection,
    rust_members::RustGeneric, rust_name_components::RustNameComponents, rust_type::RustType,
};

pub struct RustNameResolver<'a, 'b> {
    pub cordl_metadata: &'a CordlMetadata<'b>,
    pub collection: &'a RustContextCollection,
    pub config: &'a RustGenerationConfig,
}

impl<'b> RustNameResolver<'_, 'b> {
    pub fn resolve_name(
        &self,
        declaring_cpp_type: &mut RustType,
        ty: &ResolvedType,
        type_usage: TypeUsage,
        add_to_impl: bool,
        require_impl: bool,
    ) -> RustNameComponents {
        let metadata = self.cordl_metadata;
        match &ty.data {
            ResolvedTypeData::Array(array_type) => {
                let generic = self
                    .resolve_name(declaring_cpp_type, array_type, type_usage, add_to_impl, require_impl)
                    .wrap_by_gc();
                let generic_formatted = generic.combine_all();

                // declaring_cpp_type.requirements.needs_array_include();

                RustNameComponents {
                    name: "Il2CppArray".into(),
                    namespace: Some("quest_hook::libil2cpp".to_string()),
                    generics: Some(vec![generic_formatted.clone().into()]),
                    is_ptr: true,
                    is_mut: true,

                    ..Default::default()
                }
            }
            ResolvedTypeData::GenericInst(resolved_type, vec) => {
                let type_def_name_components =
                    self.resolve_name(declaring_cpp_type, resolved_type, type_usage, add_to_impl, require_impl)
                        .wrap_by_gc();
                let generic_types_formatted = vec
                    .iter()
                    .map(|(r, inc)| {
                        self.resolve_name(declaring_cpp_type, r, type_usage, add_to_impl, *inc && require_impl)
                            .wrap_by_gc()
                    })
                    .map(|n| n.combine_all())
                    .map(RustGeneric::from)
                    .collect_vec();

                // add generics to type def
                RustNameComponents {
                    generics: Some(generic_types_formatted),
                    ..type_def_name_components
                }
            }
            ResolvedTypeData::GenericArg(gen_param_idx, _arg_idx) => {
                let generic_param =
                    &metadata.metadata.global_metadata.generic_parameters[*gen_param_idx];

                generic_param.name(metadata.metadata).to_string().into()
            }
            ResolvedTypeData::GenericMethodArg(_method_index, gen_param_idx, _method_arg) => {
                let generic_param =
                    &metadata.metadata.global_metadata.generic_parameters[*gen_param_idx];

                // let arg = declaring_cpp_type
                //     .method_generic_instantiation_map
                //     .get(&method_index)
                //     .and_then(|v| v.get(method_arg as usize));

                generic_param.name(metadata.metadata).to_string().into()
            }
            ResolvedTypeData::Ptr(resolved_type) => {
                let generic_formatted = self
                    .resolve_name(declaring_cpp_type, resolved_type, type_usage, add_to_impl, require_impl)
                    .wrap_by_gc();
                // RustNameComponents {
                //     namespace: Some("cordl_internals".into()),
                //     generics: Some(vec![generic_formatted.combine_all().into()]),
                //     name: "Ptr".into(),
                //     ..Default::default()
                // }

                // TODO: Ptr type
                RustNameComponents {
                    name: "Il2CppObject".into(),
                    namespace: Some("quest_hook::libil2cpp".to_string()),
                    is_ptr: true,
                    is_mut: true,

                    ..Default::default()
                }
            }
            ResolvedTypeData::Type(resolved_tag) => {
                self.get_type_from_tag(*resolved_tag, declaring_cpp_type, metadata, add_to_impl, require_impl)
            }
            ResolvedTypeData::Primitive(s) if *s == Il2CppTypeEnum::String => {
                RustNameComponents {
                    name: "Il2CppString".into(),
                    namespace: Some("quest_hook::libil2cpp".to_string()),
                    is_mut: true,
                    is_ptr: true,

                    ..Default::default()
                }
                // let tag = CsTypeTag::TypeDefinitionIndex(self.cordl_metadata.string_tdi);
                // self.get_type_from_tag(tag, declaring_cpp_type, metadata)
            }
            ResolvedTypeData::Primitive(s) if *s == Il2CppTypeEnum::Object => {
                // let tag = CsTypeTag::TypeDefinitionIndex(self.cordl_metadata.object_tdi);
                // self.get_type_from_tag(tag, declaring_cpp_type, metadata)
                il2cpp_object()
            }
            ResolvedTypeData::Primitive(s) if *s == Il2CppTypeEnum::Void => RustNameComponents {
                name: "Void".into(),
                namespace: Some("quest_hook::libil2cpp".to_string()),

                ..Default::default()
            },

            ResolvedTypeData::Primitive(il2_cpp_type_enum) => {
                let s = Self::primitive_to_rust_ty(il2_cpp_type_enum).to_string();
                RustNameComponents::from(s)
            }
            ResolvedTypeData::Blacklisted(cs_type_tag) => {
                let td = &metadata.metadata.global_metadata.type_definitions[cs_type_tag.get_tdi()];

                Self::wrapper_type_for_tdi(td)
            }
            ResolvedTypeData::ByRef(resolved_type) => {
                let generic = self
                    .resolve_name(declaring_cpp_type, resolved_type, type_usage, add_to_impl, require_impl)
                    .wrap_by_gc();
                let generic_formatted = generic.combine_all();

                // declaring_cpp_type.requirements.needs_byref_include();

                RustNameComponents {
                    name: "ByRefMut".into(),
                    namespace: Some("quest_hook::libil2cpp".to_string()),

                    generics: Some(vec![generic_formatted.clone().into()]),
                    ..Default::default()
                }
            }
            ResolvedTypeData::ByRefConst(resolved_type) => {
                let generic = self
                    .resolve_name(declaring_cpp_type, resolved_type, type_usage, add_to_impl, require_impl)
                    .wrap_by_gc();
                let generic_formatted = generic.combine_all();

                // declaring_cpp_type.requirements.needs_byref_const_include();

                RustNameComponents {
                    name: "ByRef".into(),
                    namespace: Some("quest_hook::libil2cpp".to_string()),

                    generics: Some(vec![generic_formatted.clone().into()]),
                    ..Default::default()
                }
            }
        }
    }

    /// Resolves a type from a tag, returning the Rust name components.
    /// If the tag is a self tag, it returns the Rust name components of the declaring type.
    /// If the tag is not found, it returns a blacklisted type.
    /// If hard_include is true, it means it is only required in the implementation
    fn get_type_from_tag(
        &self,
        resolved_tag: CsTypeTag,
        declaring_rust_type: &mut RustType,
        metadata: &CordlMetadata<'b>,
        add_to_impl: bool,
        require_impl: bool,
    ) -> RustNameComponents {
        if resolved_tag == declaring_rust_type.self_tag {
            return declaring_rust_type.rs_name_components.clone();
        }

        let resolved_context_root_tag = self.collection.get_context_root_tag(resolved_tag);
        let self_context_root_tag = self
            .collection
            .get_context_root_tag(declaring_rust_type.self_tag);

        let incl_context = self
            .collection
            .get_context(resolved_tag)
            .unwrap_or_else(|| panic!("Unable to find type {resolved_tag:#?}"));
        let incl_ty = self
            .collection
            .get_rust_type(resolved_tag)
            .unwrap_or_else(|| {
                let td =
                    &metadata.metadata.global_metadata.type_definitions[resolved_tag.get_tdi()];

                println!(
                    "ty {resolved_tag:#?} vs aliased {:#?}",
                    self.collection.alias_context.get(&resolved_tag)
                );
                println!("{}", incl_context.fundamental_path.display());
                panic!(
                    "Unable to find type {resolved_tag:#?} {}",
                    td.full_name(metadata.metadata, true)
                );
            });

        if incl_ty.is_compiler_generated {
            return if incl_ty.is_reference_type || incl_ty.is_interface {
                il2cpp_object()
            } else if incl_ty.is_enum_type {
                return incl_ty.backing_type_enum.clone().unwrap();
            } else {
                // TODO: not correct
                return il2cpp_object();
            };
        }

        let is_own_context = resolved_context_root_tag == self_context_root_tag;
        if !is_own_context {
            // declaring_cpp_type
            //     .requirements
            //     .add_module(&incl_context.get_module_path(self.config));
        }

        let include = match require_impl {
            true => RustTypeRequirement::Implementation(incl_ty.self_tag),
            false => RustTypeRequirement::Definition(incl_ty.self_tag),
        };

        // add dependency
        if incl_ty.self_tag != declaring_rust_type.self_tag {
            match add_to_impl {
                true => declaring_rust_type
                    .requirements
                    .add_impl_dependency(include),
                false => declaring_rust_type
                    .requirements
                    .add_def_dependency(include),
            }
        }

        incl_ty.rs_name_components.clone()
    }

    fn wrapper_type_for_tdi(_td: &Il2CppTypeDefinition) -> RustNameComponents {
        "Blacklisted".to_string().into()
    }

    pub fn primitive_to_rust_ty(il2_cpp_type_enum: &Il2CppTypeEnum) -> &str {
        match il2_cpp_type_enum {
            Il2CppTypeEnum::I1 => "i8",
            Il2CppTypeEnum::I2 => "i16",
            Il2CppTypeEnum::I4 => "i32",
            Il2CppTypeEnum::I8 => "i64",
            Il2CppTypeEnum::U1 => "u8",
            Il2CppTypeEnum::U2 => "u16",
            Il2CppTypeEnum::U4 => "u32",
            Il2CppTypeEnum::U8 => "u64",

            Il2CppTypeEnum::R4 => "f32",
            Il2CppTypeEnum::R8 => "f64",

            Il2CppTypeEnum::Void => "()",
            Il2CppTypeEnum::Boolean => "bool",
            Il2CppTypeEnum::Char => "char",

            _ => panic!("Unsupported type {il2_cpp_type_enum:#?}"),
        }
    }
}

fn il2cpp_object() -> RustNameComponents {
    RustNameComponents {
        name: "Il2CppObject".into(),
        namespace: Some("quest_hook::libil2cpp".to_string()),
        is_mut: true,
        is_ptr: true,

        ..Default::default()
    }
}
