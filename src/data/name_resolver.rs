use std::borrow::Cow;

use brocolib::runtime_metadata::{Il2CppType, Il2CppTypeEnum, TypeData};
use clap::builder::Str;
use itertools::Itertools;
use log::warn;

use crate::generate::{
    cs_context_collection::TypeContextCollection,
    cs_type::{CsType, CsTypeRequirements},
    cs_type_tag::CsTypeTag,
    metadata::CordlMetadata,
    type_extensions::TypeDefinitionIndexExtensions,
};

use super::name_components::NameComponents;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum TypeUsage {
    // Method usage
    Parameter,
    ReturnType,

    // References
    Field,
    Property,

    // naming the CppType itself
    TypeName,
    GenericArg,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum ResolvedType {
    Array(Box<ResolvedType>),
    GenericInst(Box<ResolvedType>, Vec<ResolvedType>),
    GenericArg(u16),       // points to class generic
    GenericMethodArg(u16), // points to method generic
    Ptr(Box<ResolvedType>),
    Type(CsTypeTag),
    Primitive(Il2CppTypeEnum),
    Blacklisted(CsTypeTag),
}

pub trait NameResolver<'a> {
    // resolves the type to its proper name in C++, Rust etc.
    fn resolve_type_name(ty: ResolvedType, usage: TypeUsage) -> NameComponents;
}

pub struct TypeResolver<'a> {
    pub cordl_metadata: &'a CordlMetadata<'a>,
    pub collection: &'a TypeContextCollection,
}

impl<'a> TypeResolver<'a> {
    pub fn resolve_type(
        &self,
        declaring_cs_type: &mut CsType,
        to_resolve: &Il2CppType,
        typ_usage: TypeUsage,
    ) -> ResolvedType {
        self.resolve_type_recurse(declaring_cs_type, to_resolve, typ_usage, true)
    }

    /// [declaring_generic_inst_types] the generic instantiation of the declaring type
    fn resolve_type_recurse(
        &self,
        declaring_cs_type: &mut CsType,
        to_resolve: &Il2CppType,
        typ_usage: TypeUsage,
        add_include: bool,
    ) -> ResolvedType {
        let typ_tag = to_resolve.data;
        let metadata = self.cordl_metadata;
        let ctx_collection = self.collection;

        match to_resolve.ty {
            // https://learn.microsoft.com/en-us/nimbusml/concepts/types
            // https://en.cppreference.com/w/cpp/types/floating-point
            Il2CppTypeEnum::I1
            | Il2CppTypeEnum::U1
            | Il2CppTypeEnum::I2
            | Il2CppTypeEnum::U2
            | Il2CppTypeEnum::I4
            | Il2CppTypeEnum::U4
            | Il2CppTypeEnum::I8
            | Il2CppTypeEnum::U8
            | Il2CppTypeEnum::R4
            | Il2CppTypeEnum::R8
            | Il2CppTypeEnum::Void
            | Il2CppTypeEnum::Boolean
            | Il2CppTypeEnum::Char
            | Il2CppTypeEnum::String => {
                declaring_cs_type
                    .requirements
                    .add_dependency_tag(CsTypeTag::from_type_data(
                        to_resolve.data,
                        self.cordl_metadata.metadata,
                    ));
                return ResolvedType::Primitive(to_resolve.ty);
            }
            _ => (),
        };

        let ret = match to_resolve.ty {
            Il2CppTypeEnum::Object
            | Il2CppTypeEnum::Valuetype
            | Il2CppTypeEnum::Class
            | Il2CppTypeEnum::Typedbyref
            // ptr types
            | Il2CppTypeEnum::I
            | Il2CppTypeEnum::U => {
                let typ_cpp_tag: CsTypeTag = typ_tag.into();

                // Self
                if typ_cpp_tag == declaring_cs_type.self_tag {
                    return ResolvedType::Type(typ_cpp_tag);
                }

                // blacklist if needed
                if let TypeData::TypeDefinitionIndex(tdi) = to_resolve.data {
                    let td = &metadata.metadata.global_metadata.type_definitions[tdi];


                    if metadata.blacklisted_types.contains(&tdi) {
                        return ResolvedType::Blacklisted(typ_cpp_tag);
                    }
                }

                if add_include {
                    declaring_cs_type.requirements.add_dependency_tag(typ_cpp_tag);
                }

                let to_incl = ctx_collection.get_context(typ_cpp_tag).unwrap_or_else(|| {
                    let t = &typ_cpp_tag.get_tdi().get_type_definition(metadata.metadata);

                    panic!(
                        "no context for type {to_resolve:?} {}",
                        t.full_name(metadata.metadata, true)
                    )
                });

                let other_context_ty = ctx_collection.get_context_root_tag(typ_cpp_tag);
                let own_context_ty = ctx_collection.get_context_root_tag(declaring_cs_type.self_tag);


                let to_incl_cpp_ty = ctx_collection
                    .get_cs_type(to_resolve.data.into())
                    .unwrap_or_else(|| panic!("Unable to get type to include {:?}", to_resolve.data));

                let own_context = other_context_ty == own_context_ty;

                ResolvedType::Type(to_incl_cpp_ty.self_tag)
            }

            // Single dimension array
            Il2CppTypeEnum::Szarray => {
                let generic = match to_resolve.data {
                    TypeData::TypeIndex(e) => {
                        let ty = &metadata.metadata_registration.types[e];

                        self.resolve_type_recurse(
                            declaring_cs_type,
                            ty,
                            typ_usage,
                            add_include
                        )
                    }

                    _ => panic!("Unknown type data for array {to_resolve:?}!"),
                };

                ResolvedType::Array(Box::new(generic))
            }
            // multi dimensional array
            Il2CppTypeEnum::Array => {
                // FIXME: when stack further implements the TypeData::ArrayType we can actually implement this fully to be a multidimensional array, whatever that might mean
                warn!("Multidimensional array was requested but this is not implemented, typ: {to_resolve:?}, instead returning Il2CppObject!");
                ResolvedType::Primitive(Il2CppTypeEnum::Object)
            }
            //
            Il2CppTypeEnum::Mvar => match to_resolve.data {
                TypeData::GenericParameterIndex(index) => {
                    let generic_param: &brocolib::global_metadata::Il2CppGenericParameter =
                        &metadata.metadata.global_metadata.generic_parameters[index];

                    let owner = generic_param.owner(metadata.metadata);
                    assert!(owner.is_method != u32::MAX);

                    let (gen_idx, gen_param) = owner
                        .generic_parameters(metadata.metadata)
                        .iter()
                        .find_position(|&p| p.name_index == generic_param.name_index)
                        .unwrap();


                        ResolvedType::GenericMethodArg(gen_param.num)
                }
                _ => todo!(),
            },
            Il2CppTypeEnum::Var => match to_resolve.data {
                // Il2CppMetadataGenericParameterHandle
                TypeData::GenericParameterIndex(index) => {
                    let generic_param: &brocolib::global_metadata::Il2CppGenericParameter =
                        &metadata.metadata.global_metadata.generic_parameters[index];

                    let owner = generic_param.owner(metadata.metadata);

                    ResolvedType::GenericArg(generic_param.num)
                }
                _ => todo!(),
            },
            Il2CppTypeEnum::Genericinst => match to_resolve.data {
                TypeData::GenericClassIndex(e) => {
                    let mr = &metadata.metadata_registration;
                    let generic_class = mr.generic_classes.get(e).unwrap();
                    let generic_inst = mr
                        .generic_insts
                        .get(generic_class.context.class_inst_idx.unwrap())
                        .unwrap();

                    let new_generic_inst_types = &generic_inst.types;

                    let generic_type_def = &mr.types[generic_class.type_index];
                    let TypeData::TypeDefinitionIndex(tdi) = generic_type_def.data else {
                        panic!()
                    };

                    if add_include {
                        let generic_tag = CsTypeTag::from_type_data(to_resolve.data, metadata.metadata);

                        // depend on both tdi and generic instantiation
                        declaring_cs_type.requirements.add_dependency_tag(tdi.into());
                        declaring_cs_type.requirements.add_dependency_tag(generic_tag);
                    }

                    let generic_resolved_args = new_generic_inst_types
                        // let generic_types_formatted = new_generic_inst_types
                        .iter()
                        .map(|t| mr.types.get(*t).unwrap())
                        .map(|gen_arg_t| {
                            // we must include if the type is a value type
                            let should_include = gen_arg_t.valuetype && add_include;

                            self.resolve_type_recurse(
                                declaring_cs_type,
                                gen_arg_t,
                                TypeUsage::GenericArg,
                                should_include
                            )
                        })
                        .collect_vec();

                    let generic_type_def = &mr.types[generic_class.type_index];
                    let generic_resolved_type = self.resolve_type_recurse(
                        declaring_cs_type,
                        generic_type_def,
                        typ_usage,
                        add_include
                    );

                    // add generics to type def
                    ResolvedType::GenericInst(Box::new(generic_resolved_type), generic_resolved_args)
                }

                _ => panic!("Unknown type data for generic inst {to_resolve:?}!"),
            },


            Il2CppTypeEnum::Ptr => {
                let ptr_type = match to_resolve.data {
                    TypeData::TypeIndex(e) => {
                        let ty = &metadata.metadata_registration.types[e];
                        self.resolve_type_recurse(
                            declaring_cs_type,
                            ty,
                            typ_usage,
                            add_include
                        )
                    }

                    _ => panic!("Unknown type data for array {to_resolve:?}!"),
                };

                ResolvedType::Ptr(Box::new(ptr_type))
            }
            _ => panic!("/* UNKNOWN TYPE! {to_resolve:?} */"),
        };

        ret
    }
}
