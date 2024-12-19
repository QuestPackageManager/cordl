use brocolib::{global_metadata::Il2CppTypeDefinition, runtime_metadata::Il2CppTypeEnum};
use itertools::Itertools;

use crate::{
    data::type_resolver::{ResolvedType, ResolvedTypeData, TypeUsage},
    generate::{metadata::CordlMetadata, type_extensions::TypeDefinitionExtensions},
};

use super::{
    cpp_context_collection::CppContextCollection,
    cpp_members::{CppForwardDeclare, CppInclude},
    cpp_name_components::CppNameComponents,
    cpp_type::CppType,
    handlers::unity,
};

pub const VALUE_WRAPPER_TYPE: &str = "::bs_hook::ValueType";
pub const ENUM_WRAPPER_TYPE: &str = "::bs_hook::EnumType";
pub const INTERFACE_WRAPPER_TYPE: &str = "::cordl_internals::InterfaceW";
pub const IL2CPP_OBJECT_TYPE: &str = "Il2CppObject";

pub struct CppNameResolver<'a, 'b> {
    pub cordl_metadata: &'a CordlMetadata<'b>,
    pub collection: &'a CppContextCollection,
}

impl<'b> CppNameResolver<'_, 'b> {
    pub fn resolve_name(
        &self,
        declaring_cpp_type: &mut CppType,
        ty: &ResolvedType,
        type_usage: TypeUsage,
        hard_include: bool,
    ) -> CppNameComponents {
        let metadata = self.cordl_metadata;
        match &ty.data {
            ResolvedTypeData::Array(array_type) => {
                let generic =
                    self.resolve_name(declaring_cpp_type, array_type, type_usage, hard_include);
                let generic_formatted = generic.combine_all();

                CppNameComponents {
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
                    self.resolve_name(declaring_cpp_type, resolved_type, type_usage, hard_include);
                let generic_types_formatted = vec
                    .iter()
                    .map(|(r, inc)| {
                        self.resolve_name(
                            declaring_cpp_type,
                            r,
                            TypeUsage::GenericArg,
                            *inc && hard_include,
                        )
                    })
                    .map(|n| n.combine_all())
                    .collect_vec();

                // add generics to type def
                CppNameComponents {
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
                CppNameComponents {
                    namespace: Some("cordl_internals".into()),
                    generics: Some(vec![generic_formatted.combine_all()]),
                    name: "Ptr".into(),
                    ..Default::default()
                }
            }
            ResolvedTypeData::Type(resolved_tag) => self.resolve_type(
                resolved_tag,
                declaring_cpp_type,
                metadata,
                hard_include,
                type_usage,
            ),
            ResolvedTypeData::Primitive(il2_cpp_type_enum)
                if *il2_cpp_type_enum == Il2CppTypeEnum::Object =>
            {
                self.resolve_type(
                    &metadata.object_tdi.into(),
                    declaring_cpp_type,
                    metadata,
                    hard_include,
                    type_usage,
                )
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

                    Il2CppTypeEnum::String => {
                        requirements.needs_stringw_include();
                        "::StringW".to_string()
                    }

                    _ => panic!("Unsupported type {il2_cpp_type_enum:#?}"),
                };
                CppNameComponents::from(s)
            }
            ResolvedTypeData::Blacklisted(cs_type_tag) => {
                let td = &metadata.metadata.global_metadata.type_definitions[cs_type_tag.get_tdi()];

                Self::wrapper_type_for_tdi(td)
            }
            ResolvedTypeData::ByRef(resolved_type) => {
                let generic =
                    self.resolve_name(declaring_cpp_type, resolved_type, type_usage, hard_include);
                let generic_formatted = generic.combine_all();

                CppNameComponents {
                    name: "ByRef".into(),
                    namespace: Some("".into()),
                    generics: Some(vec![generic_formatted.clone()]),
                    is_pointer: false,
                    ..Default::default()
                }
            }
            ResolvedTypeData::ByRefConst(resolved_type) => {
                let generic =
                    self.resolve_name(declaring_cpp_type, resolved_type, type_usage, hard_include);
                let generic_formatted = generic.combine_all();

                CppNameComponents {
                    name: "ByRefConst".into(),
                    namespace: Some("".into()),
                    generics: Some(vec![generic_formatted.clone()]),
                    is_pointer: false,
                    ..Default::default()
                }
            }
        }
    }

    fn resolve_type(
        &self,
        resolved_tag: &crate::generate::cs_type_tag::CsTypeTag,
        declaring_cpp_type: &mut CppType,
        metadata: &CordlMetadata<'b>,
        hard_include: bool,
        type_usage: TypeUsage,
    ) -> CppNameComponents {
        if *resolved_tag == declaring_cpp_type.self_tag {
            return self.resolve_redirect(declaring_cpp_type, type_usage);
        }
        let resolved_context_root_tag = self.collection.get_context_root_tag(*resolved_tag);
        let self_context_root_tag = self
            .collection
            .get_context_root_tag(declaring_cpp_type.self_tag);
        let incl_context = self
            .collection
            .get_context(*resolved_tag)
            .unwrap_or_else(|| panic!("Unable to find type {resolved_tag:#?}"));
        let incl_ty = self
            .collection
            .get_cpp_type(*resolved_tag)
            .unwrap_or_else(|| {
                let td =
                    &metadata.metadata.global_metadata.type_definitions[resolved_tag.get_tdi()];

                println!(
                    "ty {resolved_tag:#?} vs aliased {:#?}",
                    self.collection.alias_context.get(resolved_tag)
                );
                println!("{}", incl_context.fundamental_path.display());
                panic!(
                    "Unable to find type {resolved_tag:#?} {}",
                    td.full_name(metadata.metadata, true)
                );
            });
        if hard_include {
            declaring_cpp_type.requirements.add_dependency(incl_ty);
        }
        let is_own_context = resolved_context_root_tag == self_context_root_tag;
        if !is_own_context {
            match hard_include {
                // can add include
                true => {
                    declaring_cpp_type.requirements.add_def_include(
                        Some(incl_ty),
                        CppInclude::new_context_typedef(incl_context),
                    );
                    declaring_cpp_type.requirements.add_impl_include(
                        Some(incl_ty),
                        CppInclude::new_context_typeimpl(incl_context),
                    );
                }
                // add forward declare
                false => {
                    declaring_cpp_type.requirements.add_forward_declare((
                        CppForwardDeclare::from_cpp_type(incl_ty),
                        CppInclude::new_context_typedef(incl_context),
                    ));
                }
            }
        }
        self.resolve_redirect(incl_ty, type_usage)
    }

    fn resolve_redirect(&self, incl_ty: &CppType, type_usage: TypeUsage) -> CppNameComponents {
        let mut name_components = incl_ty.cpp_name_components.clone();
        name_components = unity::unity_object_resolve_handler(
            name_components,
            incl_ty,
            self.cordl_metadata,
            type_usage,
        );
        name_components
    }

    fn wrapper_type_for_tdi(td: &Il2CppTypeDefinition) -> CppNameComponents {
        if td.is_enum_type() {
            return ENUM_WRAPPER_TYPE.to_string().into();
        }

        if td.is_value_type() {
            return VALUE_WRAPPER_TYPE.to_string().into();
        }

        if td.is_interface() {
            return INTERFACE_WRAPPER_TYPE.to_string().into();
        }

        il2cpp_object_name_component()
    }
}

fn il2cpp_object_name_component() -> CppNameComponents {
    CppNameComponents {
        name: IL2CPP_OBJECT_TYPE.to_string(),
        is_pointer: true,
        generics: None,
        namespace: None,
        declaring_types: None,
    }
}
