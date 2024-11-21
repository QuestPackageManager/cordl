use brocolib::{global_metadata::Il2CppTypeDefinition, runtime_metadata::Il2CppTypeEnum};
use itertools::Itertools;

use crate::{
    data::{
        name_components::NameComponents,
        type_resolver::{ResolvedType, ResolvedTypeData, TypeUsage},
    },
    generate::{metadata::CordlMetadata, type_extensions::TypeDefinitionExtensions},
};

use super::{cpp_context_collection::CppContextCollection, cpp_type::CppType};

pub const VALUE_WRAPPER_TYPE: &str = "::bs_hook::ValueType";
pub const ENUM_WRAPPER_TYPE: &str = "::bs_hook::EnumType";
pub const INTERFACE_WRAPPER_TYPE: &str = "::cordl_internals::InterfaceW";
pub const IL2CPP_OBJECT_TYPE: &str = "Il2CppObject";

pub struct CppNameResolver<'a, 'b> {
    pub cordl_metadata: &'a CordlMetadata<'b>,
    pub collection: &'a CppContextCollection,
}

impl<'a, 'b> CppNameResolver<'a, 'b> {
    pub fn resolve_name(
        &self,
        declaring_cpp_type: &mut CppType,
        ty: ResolvedType,
        type_usage: TypeUsage,
        add_include: bool,
    ) -> NameComponents {
        let metadata = self.cordl_metadata;
        match ty.data {
            ResolvedTypeData::Array(array_type) => {
                let generic =
                    self.resolve_name(declaring_cpp_type, *array_type, type_usage, add_include);
                let generic_formatted = generic.combine_all();

                NameComponents {
                    name: "ArrayW".into(),
                    namespace: Some("".into()),
                    generics: Some(vec![
                        generic_formatted.clone(),
                        format!("::Array<{generic_formatted}>*"),
                    ]),
                    is_pointer: false,
                    ..Default::default()
                }
            }
            ResolvedTypeData::GenericInst(resolved_type, vec) => {
                let type_def_name_components =
                    self.resolve_name(declaring_cpp_type, *resolved_type, type_usage, add_include);
                let generic_types_formatted = vec
                    .into_iter()
                    .map(|(r, inc)| {
                        self.resolve_name(declaring_cpp_type, r, type_usage, inc && add_include)
                    })
                    .map(|n| n.combine_all())
                    .collect_vec();

                // add generics to type def
                NameComponents {
                    generics: Some(generic_types_formatted),
                    ..type_def_name_components
                }
            }
            ResolvedTypeData::GenericArg(gen_param_idx, arg_idx) => {
                let generic_param =
                    &metadata.metadata.global_metadata.generic_parameters[gen_param_idx];

                generic_param.name(metadata.metadata).to_string().into()
            }
            ResolvedTypeData::GenericMethodArg(method_index, gen_param_idx, method_arg) => {
                let generic_param =
                    &metadata.metadata.global_metadata.generic_parameters[gen_param_idx];

                // let arg = declaring_cpp_type
                //     .method_generic_instantiation_map
                //     .get(&method_index)
                //     .and_then(|v| v.get(method_arg as usize));

                generic_param.name(metadata.metadata).to_string().into()
            }
            ResolvedTypeData::Ptr(resolved_type) => {
                let generic_formatted =
                    self.resolve_name(declaring_cpp_type, *resolved_type, type_usage, add_include);
                NameComponents {
                    namespace: Some("cordl_internals".into()),
                    generics: Some(vec![generic_formatted.combine_all()]),
                    name: "Ptr".into(),
                    ..Default::default()
                }
            }
            ResolvedTypeData::Type(cs_type_tag) => {
                let incl_context = self
                    .collection
                    .get_context(cs_type_tag)
                    .unwrap_or_else(|| panic!("Unable to find type {ty:#?}"));
                let incl_ty = self
                    .collection
                    .get_cpp_type(cs_type_tag)
                    .unwrap_or_else(|| panic!("Unable to find type {ty:#?}"));

                incl_ty.cpp_name_components.clone()
            }
            ResolvedTypeData::Primitive(il2_cpp_type_enum) => {
                let requirements = &mut declaring_cpp_type.requirements;

                match il2_cpp_type_enum {
                    Il2CppTypeEnum::I1
                    | Il2CppTypeEnum::U1
                    | Il2CppTypeEnum::I2
                    | Il2CppTypeEnum::U2
                    | Il2CppTypeEnum::I4
                    | Il2CppTypeEnum::U4
                    | Il2CppTypeEnum::I8
                    | Il2CppTypeEnum::U8
                    | Il2CppTypeEnum::I
                    | Il2CppTypeEnum::U => {
                        requirements.needs_int_include();
                    }
                    Il2CppTypeEnum::R4 | Il2CppTypeEnum::R8 => {
                        requirements.needs_math_include();
                    }
                    _ => (),
                };

                let s: String = match il2_cpp_type_enum {
                    Il2CppTypeEnum::I1 => "int8_t".to_string(),
                    Il2CppTypeEnum::I2 => "int16_t".to_string(),
                    Il2CppTypeEnum::I4 => "int32_t".to_string(),
                    Il2CppTypeEnum::I8 => "int64_t".to_string(),
                    Il2CppTypeEnum::U1 => "uint8_t".to_string(),
                    Il2CppTypeEnum::U2 => "uint16_t".to_string(),
                    Il2CppTypeEnum::U4 => "uint32_t".to_string(),
                    Il2CppTypeEnum::U8 => "uint64_t".to_string(),

                    Il2CppTypeEnum::R4 => "float_t".to_string(),
                    Il2CppTypeEnum::R8 => "double_t".to_string(),

                    Il2CppTypeEnum::Void => "void".to_string(),
                    Il2CppTypeEnum::Boolean => "bool".to_string(),
                    Il2CppTypeEnum::Char => "char16_t".to_string(),
                    Il2CppTypeEnum::Object => "void*".to_string(),

                    Il2CppTypeEnum::String => {
                        requirements.needs_stringw_include();
                        "::StringW".to_string()
                    }

                    _ => panic!("Unsupported type {il2_cpp_type_enum:#?}"),
                };
                NameComponents::from(s)
            }
            ResolvedTypeData::Blacklisted(cs_type_tag) => {
                let td = &metadata.metadata.global_metadata.type_definitions[cs_type_tag.get_tdi()];

                match td.is_value_type() {
                    true => NameComponents {
                        name: IL2CPP_OBJECT_TYPE.to_string(),
                        is_pointer: true,
                        generics: None,
                        namespace: None,
                        declaring_types: None,
                    },
                    false => Self::wrapper_type_for_tdi(td).to_string().into(),
                }
            }
            ResolvedTypeData::ByRef(resolved_type) => {
                let generic =
                    self.resolve_name(declaring_cpp_type, *resolved_type, type_usage, add_include);
                let generic_formatted = generic.combine_all();

                NameComponents {
                    name: "ByRef".into(),
                    namespace: Some("".into()),
                    generics: Some(vec![generic_formatted.clone()]),
                    is_pointer: false,
                    ..Default::default()
                }
            }
            ResolvedTypeData::ByRefConst(resolved_type) => {
                let generic =
                    self.resolve_name(declaring_cpp_type, *resolved_type, type_usage, add_include);
                let generic_formatted = generic.combine_all();

                NameComponents {
                    name: "ByRefConst".into(),
                    namespace: Some("".into()),
                    generics: Some(vec![generic_formatted.clone()]),
                    is_pointer: false,
                    ..Default::default()
                }
            }
        }
    }

    fn wrapper_type_for_tdi(td: &Il2CppTypeDefinition) -> &str {
        if td.is_enum_type() {
            return ENUM_WRAPPER_TYPE;
        }

        if td.is_value_type() {
            return VALUE_WRAPPER_TYPE;
        }

        if td.is_interface() {
            return INTERFACE_WRAPPER_TYPE;
        }

        IL2CPP_OBJECT_TYPE
    }
}
