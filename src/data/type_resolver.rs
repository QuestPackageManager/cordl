use brocolib::{
    global_metadata::{GenericParameterIndex, MethodIndex},
    runtime_metadata::{Il2CppType, Il2CppTypeEnum, TypeData},
};

use itertools::Itertools;
use log::warn;

use crate::generate::{
    cs_context_collection::TypeContextCollection, cs_type::CsType, cs_type_tag::CsTypeTag,
    metadata::CordlMetadata, type_extensions::ParameterDefinitionExtensions,
};

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
#[cfg_attr(feature = "json", derive(serde::Serialize, serde::Deserialize))]
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
pub enum ResolvedTypeData {
    Array(Box<ResolvedType>),
    GenericInst(Box<ResolvedType>, Vec<(ResolvedType, bool)>),
    GenericArg(GenericParameterIndex, u16), // points to class generic
    GenericMethodArg(MethodIndex, GenericParameterIndex, u16), // points to method generic
    Ptr(Box<ResolvedType>),
    Type(CsTypeTag),
    Primitive(Il2CppTypeEnum),
    Blacklisted(CsTypeTag),
    ByRef(Box<ResolvedType>),
    ByRefConst(Box<ResolvedType>),
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ResolvedType {
    pub data: ResolvedTypeData,
    pub ty: usize,
}

pub struct TypeResolver<'a, 'b> {
    pub cordl_metadata: &'a CordlMetadata<'b>,
    pub collection: &'a TypeContextCollection,
}

impl TypeResolver<'_, '_> {
    pub fn resolve_type(
        &self,
        declaring_cs_type: &mut CsType,
        to_resolve_idx: usize,
        typ_usage: TypeUsage,
        add_include: bool,
    ) -> ResolvedType {
        let to_resolve = &self.cordl_metadata.metadata_registration.types[to_resolve_idx];
        let data = self.resolve_type_recurse(
            declaring_cs_type,
            to_resolve,
            to_resolve_idx,
            typ_usage,
            add_include,
        );

        ResolvedType {
            data,
            ty: to_resolve_idx,
        }
    }

    /// [declaring_generic_inst_types] the generic instantiation of the declaring type
    fn resolve_type_recurse(
        &self,
        declaring_cs_type: &mut CsType,
        to_resolve: &Il2CppType,
        to_resolve_idx: usize,
        typ_usage: TypeUsage,
        add_include: bool,
    ) -> ResolvedTypeData {
        let typ_tag = to_resolve.data;
        let metadata = self.cordl_metadata;

        let ret = match to_resolve.ty {
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
            | Il2CppTypeEnum::Object
            | Il2CppTypeEnum::String => {
                declaring_cs_type
                    .requirements
                    .add_dependency_tag(CsTypeTag::from_type_data(
                        to_resolve.data,
                        self.cordl_metadata.metadata,
                    ));
                 ResolvedTypeData::Primitive(to_resolve.ty)
            }
            Il2CppTypeEnum::Class
            | Il2CppTypeEnum::Valuetype
            | Il2CppTypeEnum::Typedbyref
            // ptr types
            | Il2CppTypeEnum::I
            | Il2CppTypeEnum::U => self.resolve_ptr(typ_tag, declaring_cs_type, to_resolve, add_include),

            // Single dimension array
            Il2CppTypeEnum::Szarray => {
                let generic = match to_resolve.data {
                    TypeData::TypeIndex(e) => {

                        self.resolve_type(
                            declaring_cs_type,
                            e,
                            typ_usage,
                            add_include
                        )
                    }

                    _ => panic!("Unknown type data for array {to_resolve:?}!"),
                };

                ResolvedTypeData::Array(Box::new(generic))
            }
            // multi dimensional array
            Il2CppTypeEnum::Array => {
                // FIXME: when stack further implements the TypeData::ArrayType we can actually implement this fully to be a multidimensional array, whatever that might mean
                warn!("Multidimensional array was requested but this is not implemented, typ: {to_resolve:?}, instead returning Il2CppObject!");
                ResolvedTypeData::Primitive(Il2CppTypeEnum::Object)
            }
            //
            Il2CppTypeEnum::Mvar => match to_resolve.data {
                TypeData::GenericParameterIndex(index) => {
                    let generic_param: &brocolib::global_metadata::Il2CppGenericParameter =
                        &metadata.metadata.global_metadata.generic_parameters[index];

                    let owner = generic_param.owner(metadata.metadata);
                    assert!(owner.is_method != u32::MAX);

                    let (_gen_idx, gen_param) = owner
                        .generic_parameters(metadata.metadata)
                        .iter()
                        .find_position(|&p| p.name_index == generic_param.name_index)
                        .unwrap();

                                            let method_index = MethodIndex::new(owner.owner_index);

                        ResolvedTypeData::GenericMethodArg(method_index, index, gen_param.num)
                }
                _ => todo!(),
            },
            Il2CppTypeEnum::Var => match to_resolve.data {
                // Il2CppMetadataGenericParameterHandle
                TypeData::GenericParameterIndex(index) => {
                    let generic_param: &brocolib::global_metadata::Il2CppGenericParameter =
                        &metadata.metadata.global_metadata.generic_parameters[index];

                    let _owner = generic_param.owner(metadata.metadata);

                    ResolvedTypeData::GenericArg(index, generic_param.num)
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
                        .map(|gen_arg_t_idx| {
                            let gen_arg_ty = mr.types.get(*gen_arg_t_idx).unwrap();
                            // we must include if the type is a value type
                            let should_include = gen_arg_ty.valuetype && add_include;

                            let t =self.resolve_type(
                                declaring_cs_type,
                                *gen_arg_t_idx,
                                TypeUsage::GenericArg,
                                should_include
                            );
                            (t, should_include)
                        })
                        .collect_vec();


                    let generic_resolved_type = self.resolve_type(
                        declaring_cs_type,
                        generic_class.type_index,
                        typ_usage,
                        add_include
                    );

                    // add generics to type def
                    ResolvedTypeData::GenericInst(Box::new(generic_resolved_type), generic_resolved_args)
                }

                _ => panic!("Unknown type data for generic inst {to_resolve:?}!"),
            },


            Il2CppTypeEnum::Ptr => {
                let ptr_type = match to_resolve.data {
                    TypeData::TypeIndex(e) => {
                        self.resolve_type(
                            declaring_cs_type,
                            e,
                            typ_usage,
                            add_include
                        )
                    }

                    _ => panic!("Unknown type data for array {to_resolve:?}!"),
                };

                ResolvedTypeData::Ptr(Box::new(ptr_type))
            }
            _ => panic!("/* UNKNOWN TYPE! {to_resolve:?} */"),
        };

        let byref_allowed = matches!(
            typ_usage,
            TypeUsage::Parameter
                | TypeUsage::ReturnType
                | TypeUsage::TypeName
                | TypeUsage::GenericArg
        );

        if (to_resolve.is_param_out() || (to_resolve.byref && !to_resolve.valuetype))
            && byref_allowed
        {
            return ResolvedTypeData::ByRef(Box::new(ResolvedType {
                ty: to_resolve_idx,
                data: ret,
            }));
        }

        if to_resolve.is_param_in() && byref_allowed {
            return ResolvedTypeData::ByRefConst(Box::new(ResolvedType {
                ty: to_resolve_idx,
                data: ret,
            }));
        }

        ret
    }

    fn resolve_ptr(
        &self,
        typ_tag: TypeData,
        declaring_cs_type: &mut CsType,
        to_resolve: &Il2CppType,
        add_include: bool,
    ) -> ResolvedTypeData {
        let metadata = self.cordl_metadata;
        let ctx_collection = self.collection;
        let typ_cpp_tag: CsTypeTag = typ_tag.into();
        // Self
        if typ_cpp_tag == declaring_cs_type.self_tag {
            return ResolvedTypeData::Type(typ_cpp_tag);
        }

        if let TypeData::TypeDefinitionIndex(tdi) = to_resolve.data
            && metadata.blacklisted_types.contains(&tdi)
        {
            // blacklist if needed

            return ResolvedTypeData::Blacklisted(typ_cpp_tag);
        }

        if add_include {
            declaring_cs_type
                .requirements
                .add_dependency_tag(typ_cpp_tag);
        }

        let to_incl_cpp_ty = ctx_collection
            .get_cs_type(to_resolve.data.into())
            .unwrap_or_else(|| panic!("Unable to get type to include {:?}", to_resolve.data));

        ResolvedTypeData::Type(to_incl_cpp_ty.self_tag)
    }
}

impl ResolvedType {
    pub fn get_type<'a>(&self, metadata: &CordlMetadata<'a>) -> &'a Il2CppType {
        &metadata.metadata_registration.types[self.ty]
    }
}
