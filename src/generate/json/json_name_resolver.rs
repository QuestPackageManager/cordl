use itertools::Itertools;

use crate::{
    data::{
        name_components::NameComponents,
        type_resolver::{ResolvedType, ResolvedTypeData},
    },
    generate::{cs_context_collection::TypeContextCollection, metadata::CordlMetadata},
};

use super::json_data::JsonTypeEnum;

pub struct JsonNameResolver<'a, 'b> {
    pub cordl_metadata: &'a CordlMetadata<'b>,
    pub collection: &'a TypeContextCollection,
}

impl<'a, 'b> JsonNameResolver<'a, 'b> {
    pub fn resolve_name(&self, ty: &ResolvedType) -> NameComponents {
        let metadata = self.cordl_metadata;
        match &ty.data {
            ResolvedTypeData::Array(array_type) => {
                let generic = self.resolve_name(array_type);
                let generic_formatted = generic.combine_all();

                NameComponents {
                    name: "Array".into(),
                    namespace: Some("".into()),
                    generics: Some(vec![generic_formatted.clone()]),
                    ..Default::default()
                }
            }
            ResolvedTypeData::GenericInst(resolved_type, vec) => {
                let type_def_name_components = self.resolve_name(resolved_type);
                let generic_types_formatted = vec
                    .iter()
                    .map(|(r, _inc)| self.resolve_name(r))
                    .map(|n| n.combine_all())
                    .collect_vec();

                // add generics to type def
                NameComponents {
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
                let generic_formatted = self.resolve_name(resolved_type);
                NameComponents {
                    namespace: None,
                    name: "Ptr".into(),
                    generics: Some(vec![generic_formatted.combine_all()]),
                    ..Default::default()
                }
            }
            ResolvedTypeData::Type(resolved_tag) => {
                let _incl_context = self
                    .collection
                    .get_context(*resolved_tag)
                    .unwrap_or_else(|| panic!("Unable to find type {ty:#?}"));
                let incl_ty = self
                    .collection
                    .get_cs_type(*resolved_tag)
                    .unwrap_or_else(|| {
                        let td = &metadata.metadata.global_metadata.type_definitions
                            [resolved_tag.get_tdi()];

                        panic!(
                            "Unable to find type {ty:#?} {}",
                            td.full_name(metadata.metadata, true)
                        );
                    });
                incl_ty.cs_name_components.clone()
            }
            ResolvedTypeData::Primitive(il2_cpp_type_enum) => {
                let json_type = JsonTypeEnum::from(*il2_cpp_type_enum);

                let s: String = json_type.to_string();
                NameComponents::from(s)
            }
            ResolvedTypeData::Blacklisted(cs_type_tag) => {
                let td = &metadata.metadata.global_metadata.type_definitions[cs_type_tag.get_tdi()];

                NameComponents {
                    name: "Blacklisted".into(),
                    namespace: None,
                    generics: Some(vec![td.full_name(metadata.metadata, true)]),
                    ..Default::default()
                }
            }
            ResolvedTypeData::ByRef(resolved_type) => {
                let generic = self.resolve_name(resolved_type);
                let generic_formatted = generic.combine_all();

                NameComponents {
                    name: "ByRef".into(),
                    namespace: Some("".into()),
                    generics: Some(vec![generic_formatted.clone()]),
                    ..Default::default()
                }
            }
            ResolvedTypeData::ByRefConst(resolved_type) => {
                let generic = self.resolve_name(resolved_type);
                let generic_formatted = generic.combine_all();

                NameComponents {
                    name: "ByRefConst".into(),
                    namespace: Some("".into()),
                    generics: Some(vec![generic_formatted.clone()]),
                    ..Default::default()
                }
            }
        }
    }
}
