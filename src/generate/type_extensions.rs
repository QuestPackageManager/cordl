use core::panic;

use brocolib::{
    Metadata,
    global_metadata::{
        Il2CppGenericParameter, Il2CppMethodDefinition, Il2CppTypeDefinition, TypeDefinitionIndex,
    },
    runtime_metadata::{Il2CppType, Il2CppTypeEnum, TypeData},
};
use itertools::Itertools;

/// Abstracts over `brocolib::global_metadata::TypeIndex`'s representation, which differs
/// between brocolib versions: a plain `u32` on `il2cpp_v29`/`il2cpp_v31`, and a newtype struct
/// (exposing `.index()`/`.is_valid()`) on `il2cpp_v39`.
pub trait TypeIndexExt: Copy {
    fn idx(self) -> usize;
    fn idx_is_valid(self) -> bool;
}

#[cfg(any(feature = "il2cpp_v29", feature = "il2cpp_v31"))]
impl TypeIndexExt for u32 {
    fn idx(self) -> usize {
        self as usize
    }

    fn idx_is_valid(self) -> bool {
        self != u32::MAX
    }
}

#[cfg(feature = "il2cpp_v39")]
impl TypeIndexExt for brocolib::global_metadata::TypeIndex {
    fn idx(self) -> usize {
        self.index() as usize
    }

    fn idx_is_valid(self) -> bool {
        self.is_valid()
    }
}

use crate::{data::name_components::NameComponents, generate::cs_type_tag::CsTypeTag};

pub const PARAM_ATTRIBUTE_IN: u16 = 0x0001;
pub const PARAM_ATTRIBUTE_OUT: u16 = 0x0002;
pub const PARAM_ATTRIBUTE_OPTIONAL: u16 = 0x0010;

pub const TYPE_ATTRIBUTE_INTERFACE: u32 = 0x00000020;
pub const TYPE_ATTRIBUTE_NESTED_PUBLIC: u32 = 0x00000002;
pub const TYPE_ATTRIBUTE_EXPLICIT_LAYOUT: u32 = 0x00000010;
pub const TYPE_ATTRIBUTE_SPECIAL_NAME: u32 = 0x00000400;

pub const FIELD_ATTRIBUTE_PUBLIC: u16 = 0x0006;
pub const FIELD_ATTRIBUTE_PRIVATE: u16 = 0x0001;
pub const FIELD_ATTRIBUTE_STATIC: u16 = 0x0010;
pub const FIELD_ATTRIBUTE_LITERAL: u16 = 0x0040;

// ECMA-335 II.23.1.7 GenericParamAttributes
pub const GENERIC_PARAM_ATTRIBUTE_VARIANCE_MASK: u16 = 0x0003;
pub const GENERIC_PARAM_ATTRIBUTE_COVARIANT: u16 = 0x0001;
pub const GENERIC_PARAM_ATTRIBUTE_CONTRAVARIANT: u16 = 0x0002;
pub const GENERIC_PARAM_ATTRIBUTE_REFERENCE_TYPE_CONSTRAINT: u16 = 0x0004;
pub const GENERIC_PARAM_ATTRIBUTE_NOT_NULLABLE_VALUE_TYPE_CONSTRAINT: u16 = 0x0008;
pub const GENERIC_PARAM_ATTRIBUTE_DEFAULT_CONSTRUCTOR_CONSTRAINT: u16 = 0x0010;

pub const METHOD_ATTRIBUTE_PUBLIC: u16 = 0x0006;
pub const METHOD_ATTRIBUTE_STATIC: u16 = 0x0010;
pub const METHOD_ATTRIBUTE_FINAL: u16 = 0x0020;
pub const METHOD_ATTRIBUTE_VIRTUAL: u16 = 0x0040;
pub const METHOD_ATTRIBUTE_HIDE_BY_SIG: u16 = 0x0080;
pub const METHOD_ATTRIBUTE_ABSTRACT: u16 = 0x0400;
pub const METHOD_ATTRIBUTE_SPECIAL_NAME: u16 = 0x0800;

pub trait MethodDefintionExtensions {
    fn is_public_method(&self) -> bool;
    fn is_abstract_method(&self) -> bool;
    fn is_static_method(&self) -> bool;
    fn is_virtual_method(&self) -> bool;
    fn is_hidden_sig(&self) -> bool;
    fn is_special_name(&self) -> bool;
    fn is_final_method(&self) -> bool;
}

impl MethodDefintionExtensions for Il2CppMethodDefinition {
    fn is_public_method(&self) -> bool {
        (self.flags & METHOD_ATTRIBUTE_PUBLIC) != 0
    }

    fn is_virtual_method(&self) -> bool {
        (self.flags & METHOD_ATTRIBUTE_VIRTUAL) != 0
    }

    fn is_static_method(&self) -> bool {
        (self.flags & METHOD_ATTRIBUTE_STATIC) != 0
    }

    fn is_abstract_method(&self) -> bool {
        (self.flags & METHOD_ATTRIBUTE_ABSTRACT) != 0
    }

    fn is_hidden_sig(&self) -> bool {
        (self.flags & METHOD_ATTRIBUTE_HIDE_BY_SIG) != 0
    }

    fn is_special_name(&self) -> bool {
        (self.flags & METHOD_ATTRIBUTE_SPECIAL_NAME) != 0
    }

    fn is_final_method(&self) -> bool {
        (self.flags & METHOD_ATTRIBUTE_FINAL) != 0
    }
}

pub trait GenericParameterExtensions {
    /// `where T : class`
    fn has_reference_type_constraint(&self) -> bool;
    /// `where T : struct`
    fn has_not_nullable_value_type_constraint(&self) -> bool;
    /// `where T : new()`
    fn has_default_constructor_constraint(&self) -> bool;
    /// `out T`
    fn is_covariant(&self) -> bool;
    /// `in T`
    fn is_contravariant(&self) -> bool;
}

impl GenericParameterExtensions for Il2CppGenericParameter {
    fn has_reference_type_constraint(&self) -> bool {
        (self.flags & GENERIC_PARAM_ATTRIBUTE_REFERENCE_TYPE_CONSTRAINT) != 0
    }

    fn has_not_nullable_value_type_constraint(&self) -> bool {
        (self.flags & GENERIC_PARAM_ATTRIBUTE_NOT_NULLABLE_VALUE_TYPE_CONSTRAINT) != 0
    }

    fn has_default_constructor_constraint(&self) -> bool {
        (self.flags & GENERIC_PARAM_ATTRIBUTE_DEFAULT_CONSTRUCTOR_CONSTRAINT) != 0
    }

    fn is_covariant(&self) -> bool {
        (self.flags & GENERIC_PARAM_ATTRIBUTE_VARIANCE_MASK) == GENERIC_PARAM_ATTRIBUTE_COVARIANT
    }

    fn is_contravariant(&self) -> bool {
        (self.flags & GENERIC_PARAM_ATTRIBUTE_VARIANCE_MASK) == GENERIC_PARAM_ATTRIBUTE_CONTRAVARIANT
    }
}

pub trait ParameterDefinitionExtensions {
    fn is_param_optional(&self) -> bool;
    fn is_param_in(&self) -> bool;
    fn is_param_out(&self) -> bool;
}

impl ParameterDefinitionExtensions for Il2CppType {
    fn is_param_optional(&self) -> bool {
        (self.attrs & PARAM_ATTRIBUTE_OPTIONAL) != 0
    }

    fn is_param_in(&self) -> bool {
        (self.attrs & PARAM_ATTRIBUTE_IN) != 0
    }

    fn is_param_out(&self) -> bool {
        (self.attrs & PARAM_ATTRIBUTE_OUT) != 0
    }
}

pub trait TypeExtentions {
    fn is_static(&self) -> bool;
    fn is_constant(&self) -> bool;
    fn is_byref(&self) -> bool;

    fn fill_generic_inst<'a>(
        &'a self,
        generic_types: &[&'a Il2CppType],
        metadata: &'a Metadata,
    ) -> (&'a Il2CppType, Option<Vec<&'a Il2CppType>>);
}

impl TypeExtentions for Il2CppType {
    fn is_static(&self) -> bool {
        (self.attrs & FIELD_ATTRIBUTE_STATIC) != 0
    }

    // FIELD_ATTRIBUTE_LITERAL
    fn is_constant(&self) -> bool {
        (self.attrs & FIELD_ATTRIBUTE_LITERAL) != 0
    }

    fn is_byref(&self) -> bool {
        self.byref
    }

    /// Returns the actual type for the given generic inst
    /// or drills down and fixes it in generic instantiations
    fn fill_generic_inst<'a>(
        &'a self,
        generic_types: &[&'a Il2CppType],
        metadata: &'a Metadata,
    ) -> (&'a Il2CppType, Option<Vec<&'a Il2CppType>>) {
        match &self.data {
            TypeData::GenericParameterIndex(gen_param_idx) => {
                let gen_param = &metadata.global_metadata.generic_parameters[*gen_param_idx];

                (generic_types[gen_param.num as usize], None)
            }
            TypeData::GenericClassIndex(generic_class_idx) => {
                let generic_class = &metadata
                    .runtime_metadata
                    .metadata_registration
                    .generic_classes[*generic_class_idx];

                let class_inst_idx = generic_class.context.class_inst_idx.unwrap();
                let class_inst = &metadata
                    .runtime_metadata
                    .metadata_registration
                    .generic_insts[class_inst_idx];
                let instantiated_generic_types = class_inst
                    .types
                    .iter()
                    .map(|t| {
                        let ty = &metadata.runtime_metadata.metadata_registration.types[*t];

                        ty.fill_generic_inst(generic_types, metadata)
                    })
                    .map(|(t, _ts)| t)
                    .collect_vec();

                let td_type_idx = &generic_class.type_index;
                let td_type = &metadata.runtime_metadata.metadata_registration.types[*td_type_idx];
                let TypeData::TypeDefinitionIndex(_tdi) = td_type.data else {
                    panic!()
                };
                // let td = &metadata.global_metadata.type_definitions[tdi];

                (td_type, Some(instantiated_generic_types))
            }
            _ => (self, None),
        }
    }
}

pub trait TypeDefinitionExtensions {
    fn is_value_type(&self) -> bool;
    fn is_enum_type(&self) -> bool;
    fn is_special_name(&self) -> bool;
    fn is_compiler_generated(&self, metadata: &Metadata) -> bool;
    fn is_interface(&self) -> bool;
    fn is_explicit_layout(&self) -> bool;
    fn is_assignable_to(&self, other_td: &Il2CppTypeDefinition, metadata: &Metadata) -> bool;

    fn get_name_components(&self, metadata: &Metadata) -> NameComponents;

    // fn full_name_cpp(&self, metadata: &Metadata, with_generics: bool) -> String;
    // fn full_name_nested(&self, metadata: &Metadata, with_generics: bool) -> String;

    fn is_reference_type(&self, metadata: &Metadata) -> bool;

    /// An enum's backing integer type, read off its sole non-static instance field
    /// (conventionally named `value__`) - the metadata counterpart of what
    /// `Class::GetEnumBaseType` (`vm/Class.cpp:270`) reads off the runtime `Il2CppClass`.
    /// `None` if this isn't an enum, or it has no backing type on record.
    fn enum_backing_type<'a>(&self, metadata: &'a Metadata) -> Option<&'a Il2CppType>;

    /// Same as [`Self::enum_backing_type`], but returns the raw index into
    /// `metadata_registration.types` instead of the resolved type.
    fn enum_backing_type_index(&self, metadata: &Metadata) -> Option<usize>;
}

impl TypeDefinitionExtensions for Il2CppTypeDefinition {
    fn is_value_type(&self) -> bool {
        self.bitfield & 1 != 0
    }

    fn is_enum_type(&self) -> bool {
        self.bitfield & 2 != 0
    }

    fn is_compiler_generated(&self, metadata: &Metadata) -> bool {
        if self.is_special_name()
            || (self.name(metadata).starts_with('<') && self.name(metadata).contains(">d__"))
            || self.name(metadata).contains("<>c")
            || self
                .name(metadata)
                .starts_with("<PrivateImplementationDetails>")
        {
            return true;
        }

        // check recursively if any declaring type is compiler generated
        if !self.declaring_type_index.idx_is_valid() {
            return false;
        }
        let declaring_ty = metadata.runtime_metadata.metadata_registration.types
            [self.declaring_type_index.idx()];
        let declaring_td = CsTypeTag::from_type_data(declaring_ty.data, metadata);
        let declaring_td = declaring_td.get_tdi().get_type_definition(metadata);

        declaring_td.is_compiler_generated(metadata)
    }

    fn is_special_name(&self) -> bool {
        self.flags & TYPE_ATTRIBUTE_SPECIAL_NAME != 0
    }

    fn is_interface(&self) -> bool {
        self.flags & TYPE_ATTRIBUTE_INTERFACE != 0
    }
    fn is_explicit_layout(&self) -> bool {
        self.flags & TYPE_ATTRIBUTE_EXPLICIT_LAYOUT != 0
    }

    fn is_assignable_to(&self, other_td: &Il2CppTypeDefinition, metadata: &Metadata) -> bool {
        // same type
        if self.name_index == other_td.name_index
            && self.namespace_index == other_td.namespace_index
            && self.declaring_type_index == other_td.declaring_type_index
            && self.generic_container_index == other_td.generic_container_index
        {
            return true;
        }
        if self.byval_type_index == other_td.byval_type_index {
            return true;
        }

        // does not inherit anything
        if !self.parent_index.idx_is_valid() {
            return false;
        }

        let parent_ty =
            &metadata.runtime_metadata.metadata_registration.types[self.parent_index.idx()];

        // direct inheritance
        if other_td.byval_type_index == self.parent_index {
            return true;
        }

        // if object, clearly this does not inherit `other_td`
        if !matches!(
            parent_ty.ty,
            Il2CppTypeEnum::Genericinst
                | Il2CppTypeEnum::Byref
                | Il2CppTypeEnum::Class
                | Il2CppTypeEnum::Internal
                | Il2CppTypeEnum::Array,
        ) {
            return false;
        }

        let parent_tdi = match parent_ty.data {
            TypeData::TypeDefinitionIndex(tdi) => tdi,
            TypeData::GenericClassIndex(gen_idx) => {
                let gen_inst = &metadata
                    .runtime_metadata
                    .metadata_registration
                    .generic_classes[gen_idx];

                let gen_ty =
                    &metadata.runtime_metadata.metadata_registration.types[gen_inst.type_index];

                let TypeData::TypeDefinitionIndex(gen_tdi) = gen_ty.data else {
                    todo!()
                };

                gen_tdi
            }
            _ => panic!(
                "Unsupported type: {:?} {}",
                parent_ty,
                parent_ty.full_name(metadata)
            ),
        };

        // check if parent is descendant of `other_td`
        let parent_td = &metadata.global_metadata.type_definitions[parent_tdi];
        parent_td.is_assignable_to(other_td, metadata)
    }

    fn get_name_components(&self, metadata: &Metadata) -> NameComponents {
        let namespace_str = self.namespace(metadata);
        let namespace = if namespace_str.is_empty() {
            None
        } else {
            Some(namespace_str.to_string())
        };

        let name = self.name(metadata);

        let generics = match self.generic_container_index.is_valid() {
            true => {
                let gc = self.generic_container(metadata);
                Some(
                    gc.generic_parameters(metadata)
                        .iter()
                        .map(|p| p.name(metadata).to_string())
                        .collect_vec(),
                )
            }
            false => None,
        };

        let _ty =
            &metadata.runtime_metadata.metadata_registration.types[self.byval_type_index.idx()];

        match self.declaring_type_index.idx_is_valid() {
            true => {
                let declaring_ty = metadata.runtime_metadata.metadata_registration.types
                    [self.declaring_type_index.idx()];

                let declaring_ty_names = match declaring_ty.data {
                    brocolib::runtime_metadata::TypeData::TypeDefinitionIndex(tdi) => {
                        let declaring_td = &metadata.global_metadata.type_definitions[tdi];
                        declaring_td.get_name_components(metadata)
                    }
                    _ => todo!(),
                };

                let mut declaring_types = declaring_ty_names.declaring_types.unwrap_or_default();
                declaring_types.push(declaring_ty_names.name);

                NameComponents {
                    namespace: declaring_ty_names.namespace,
                    name: name.to_string(),
                    declaring_types: Some(declaring_types),
                    generics,
                }
            }
            false => NameComponents {
                namespace,
                name: name.to_string(),
                declaring_types: None,
                generics,
            },
        }
    }

    // TODO: Use name components
    // fn full_name_cpp(&self, metadata: &Metadata, with_generics: bool) -> String {
    //     let namespace = config.namespace_cpp(self.namespace(metadata));
    //     let name = config.name_cpp(self.name(metadata));

    //     let mut full_name = String::new();

    //     if self.declaring_type_index != u32::MAX {
    //         let declaring_ty = metadata.runtime_metadata.metadata_registration.types
    //             [self.declaring_type_index as usize];

    //         let s = match declaring_ty.data {
    //             brocolib::runtime_metadata::TypeData::TypeDefinitionIndex(tdi) => {
    //                 let declaring_td = &metadata.global_metadata.type_definitions[tdi];
    //                 declaring_td.full_name_cpp(metadata, config, with_generics)
    //             }
    //             _ => declaring_ty.full_name(metadata),
    //         };

    //         full_name.push_str(&s);
    //         full_name.push_str("::");
    //     } else {
    //         // only write namespace if no declaring type
    //         full_name.push_str(&namespace);
    //         full_name.push_str("::");
    //     }

    //     full_name.push_str(&name);
    //     if self.generic_container_index.is_valid() && with_generics {
    //         let gc = self.generic_container(metadata);
    //         full_name.push_str(&gc.to_string(metadata));
    //     }
    //     full_name
    // }

    // separates nested types with /
    // TODO: Use name components
    // fn full_name_nested(&self, metadata: &Metadata, with_generics: bool) -> String {
    //     let namespace = config.namespace_cpp(self.namespace(metadata));
    //     let name = config.name_cpp(self.name(metadata));

    //     let mut full_name = String::new();

    //     if self.declaring_type_index != u32::MAX {
    //         let declaring_ty = metadata.runtime_metadata.metadata_registration.types
    //             [self.declaring_type_index as usize];

    //         let declaring_ty = match declaring_ty.data {
    //             brocolib::runtime_metadata::TypeData::TypeDefinitionIndex(tdi) => {
    //                 let declaring_td = &metadata.global_metadata.type_definitions[tdi];
    //                 declaring_td.full_name_nested(metadata, config, with_generics)
    //             }
    //             _ => declaring_ty.full_name(metadata),
    //         };

    //         full_name.push_str(&declaring_ty);
    //         full_name.push('/');
    //     } else {
    //         // only write namespace if no declaring type
    //         full_name.push_str(&namespace);
    //         full_name.push('.');
    //     }

    //     full_name.push_str(&name);
    //     if self.generic_container_index.is_valid() && with_generics {
    //         let gc = self.generic_container(metadata);
    //         full_name.push_str(&gc.to_string(metadata));
    //     }
    //     full_name
    // }

    fn is_reference_type(&self, metadata: &Metadata) -> bool {
        let ty =
            &metadata.runtime_metadata.metadata_registration.types[self.byval_type_index.idx()];

        (!self.is_value_type() && !self.is_enum_type()) || ty.ty == Il2CppTypeEnum::Class
    }

    fn enum_backing_type_index(&self, metadata: &Metadata) -> Option<usize> {
        if !self.is_enum_type() {
            return None;
        }

        // Newer il2cpp metadata versions dropped the `elementTypeIndex` shortcut, so instead
        // we find the backing type the same way the runtime does: an enum's only non-static
        // instance field (conventionally named `value__`) carries it.
        self.fields(metadata).iter().find_map(|f| {
            let idx = f.type_index.idx();
            let field_ty = metadata.runtime_metadata.metadata_registration.types.get(idx)?;

            (!field_ty.is_static()).then_some(idx)
        })
    }

    fn enum_backing_type<'a>(&self, metadata: &'a Metadata) -> Option<&'a Il2CppType> {
        self.enum_backing_type_index(metadata)
            .map(|idx| &metadata.runtime_metadata.metadata_registration.types[idx])
    }
}

pub trait TypeDefinitionIndexExtensions {
    fn get_type_definition<'a>(&self, metadata: &'a Metadata) -> &'a Il2CppTypeDefinition;
}

impl TypeDefinitionIndexExtensions for TypeDefinitionIndex {
    fn get_type_definition<'a>(&self, metadata: &'a Metadata) -> &'a Il2CppTypeDefinition {
        &metadata.global_metadata.type_definitions[*self]
    }
}
