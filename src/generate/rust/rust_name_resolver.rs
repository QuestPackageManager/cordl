use brocolib::{global_metadata::Il2CppTypeDefinition, runtime_metadata::Il2CppTypeEnum};
use itertools::Itertools;

use crate::{
    data::type_resolver::{ResolvedType, ResolvedTypeData, TypeUsage},
    generate::{metadata::CordlMetadata, type_extensions::TypeDefinitionExtensions},
};

use super::{
    config::RustGenerationConfig, rust_context_collection::RustContextCollection,
    rust_name_components::RustNameComponents, rust_type::RustType,
};

pub struct RustNameResolver<'a, 'b> {
    pub cordl_metadata: &'a CordlMetadata<'b>,
    pub collection: &'a RustContextCollection,
    pub config: &'a RustGenerationConfig,
}

impl<'a, 'b> RustNameResolver<'a, 'b> {
    pub fn resolve_name(
        &self,
        declaring_cpp_type: &mut RustType,
        ty: &ResolvedType,
        type_usage: TypeUsage,
        hard_include: bool,
    ) -> RustNameComponents {
        let metadata = self.cordl_metadata;
        match &ty.data {
            ResolvedTypeData::Array(array_type) => {
                let generic =
                    self.resolve_name(declaring_cpp_type, array_type, type_usage, hard_include);
                let generic_formatted = generic.combine_all();

                declaring_cpp_type.requirements.needs_array_include();

                RustNameComponents {
                    name: "Il2CppArray".into(),
                    namespace: Some("".into()),
                    generics: Some(vec![generic_formatted.clone()]),
                    ..Default::default()
                }
            }
            ResolvedTypeData::GenericInst(resolved_type, vec) => {
                let type_def_name_components =
                    self.resolve_name(declaring_cpp_type, resolved_type, type_usage, hard_include);
                let generic_types_formatted = vec
                    .iter()
                    .map(|(r, inc)| {
                        self.resolve_name(declaring_cpp_type, r, type_usage, *inc && hard_include)
                    })
                    .map(|n| n.combine_all())
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
                let generic_formatted =
                    self.resolve_name(declaring_cpp_type, resolved_type, type_usage, hard_include);
                RustNameComponents {
                    namespace: Some("cordl_internals".into()),
                    generics: Some(vec![generic_formatted.combine_all()]),
                    name: "Ptr".into(),
                    ..Default::default()
                }
            }
            ResolvedTypeData::Type(resolved_tag) => {
                if *resolved_tag == declaring_cpp_type.self_tag {
                    return declaring_cpp_type.rs_name_components.clone();
                }

                let resolved_context_root_tag = self.collection.get_context_root_tag(*resolved_tag);
                let self_context_root_tag = self
                    .collection
                    .get_context_root_tag(declaring_cpp_type.self_tag);

                let incl_context = self
                    .collection
                    .get_context(*resolved_tag)
                    .unwrap_or_else(|| panic!("Unable to find type {ty:#?}"));
                let incl_ty = self
                    .collection
                    .get_cpp_type(*resolved_tag)
                    .unwrap_or_else(|| {
                        let td = &metadata.metadata.global_metadata.type_definitions
                            [resolved_tag.get_tdi()];

                        println!(
                            "ty {resolved_tag:#?} vs aliased {:#?}",
                            self.collection.alias_context.get(resolved_tag)
                        );
                        println!("{}", incl_context.fundamental_path.display());
                        panic!(
                            "Unable to find type {ty:#?} {}",
                            td.full_name(metadata.metadata, true)
                        );
                    });

                let is_own_context = resolved_context_root_tag == self_context_root_tag;

                if !is_own_context {
                    declaring_cpp_type
                        .requirements
                        .add_module(&incl_context.get_module_path(self.config));
                }

                incl_ty.rs_name_components.clone()
            }
            ResolvedTypeData::Primitive(il2_cpp_type_enum) => {
                let requirements = &mut declaring_cpp_type.requirements;

                let s: String = match il2_cpp_type_enum {
                    Il2CppTypeEnum::I1 => "i8".to_string(),
                    Il2CppTypeEnum::I2 => "i16".to_string(),
                    Il2CppTypeEnum::I4 => "i32".to_string(),
                    Il2CppTypeEnum::I8 => "i64".to_string(),
                    Il2CppTypeEnum::U1 => "u8".to_string(),
                    Il2CppTypeEnum::U2 => "u16".to_string(),
                    Il2CppTypeEnum::U4 => "u32".to_string(),
                    Il2CppTypeEnum::U8 => "u64".to_string(),

                    Il2CppTypeEnum::R4 => "f32".to_string(),
                    Il2CppTypeEnum::R8 => "f64".to_string(),

                    Il2CppTypeEnum::Void => "()".to_string(),
                    Il2CppTypeEnum::Boolean => "bool".to_string(),
                    Il2CppTypeEnum::Char => "char".to_string(),
                    Il2CppTypeEnum::Object => {
                        requirements.needs_object_include();
                        "Il2CppObject".to_string()
                    }
                    Il2CppTypeEnum::String => {
                        requirements.needs_string_include();
                        "::StringW".to_string()
                    }

                    _ => panic!("Unsupported type {il2_cpp_type_enum:#?}"),
                };
                RustNameComponents::from(s)
            }
            ResolvedTypeData::Blacklisted(cs_type_tag) => {
                let td = &metadata.metadata.global_metadata.type_definitions[cs_type_tag.get_tdi()];

                Self::wrapper_type_for_tdi(td)
            }
            ResolvedTypeData::ByRef(resolved_type) => {
                let generic =
                    self.resolve_name(declaring_cpp_type, resolved_type, type_usage, hard_include);
                let generic_formatted = generic.combine_all();

                declaring_cpp_type.requirements.needs_byref_include();

                RustNameComponents {
                    name: "ByRef".into(),
                    generics: Some(vec![generic_formatted.clone()]),
                    ..Default::default()
                }
            }
            ResolvedTypeData::ByRefConst(resolved_type) => {
                let generic =
                    self.resolve_name(declaring_cpp_type, resolved_type, type_usage, hard_include);
                let generic_formatted = generic.combine_all();

                declaring_cpp_type.requirements.needs_byref_const_include();

                RustNameComponents {
                    name: "ByRefConst".into(),
                    generics: Some(vec![generic_formatted.clone()]),
                    ..Default::default()
                }
            }
        }
    }

    fn wrapper_type_for_tdi(td: &Il2CppTypeDefinition) -> RustNameComponents {
        "Blacklisted".to_string().into()
    }
}
