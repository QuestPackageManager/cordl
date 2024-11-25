use brocolib::{global_metadata::TypeDefinitionIndex, runtime_metadata::Il2CppTypeEnum};
use serde::{Deserialize, Serialize};

use crate::{
    data::type_resolver::{ResolvedType, ResolvedTypeData},
    generate::cs_type_tag::CsTypeTag,
};

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, PartialOrd, Ord, Eq, Hash)]
pub enum JsonTypeTag {
    TypeDefinition(u32),
    GenericInstantiation { type_definition: u32, inst: usize },
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum JsonResolvedTypeData {
    Array(Box<JsonResolvedTypeData>),
    GenericInst(Box<JsonResolvedTypeData>, Vec<(JsonResolvedTypeData, bool)>),
    GenericArg(u32, u16),            // points to class generic
    GenericMethodArg(u32, u32, u16), // points to method generic
    Ptr(Box<JsonResolvedTypeData>),
    Type(JsonTypeTag),
    Primitive(JsonTypeEnum),
    Blacklisted(JsonTypeTag),
    ByRef(Box<JsonResolvedTypeData>),
    ByRefConst(Box<JsonResolvedTypeData>),
}

impl From<ResolvedType> for JsonResolvedTypeData {
    fn from(value: ResolvedType) -> Self {
        match value.data {
            ResolvedTypeData::Array(inner) => {
                JsonResolvedTypeData::Array(Box::new((*inner).into()))
            }
            ResolvedTypeData::GenericInst(inner, args) => JsonResolvedTypeData::GenericInst(
                Box::new((*inner).into()),
                args.into_iter().map(|(arg, b)| (arg.into(), b)).collect(),
            ),
            ResolvedTypeData::GenericArg(a, b) => JsonResolvedTypeData::GenericArg(a.index(), b),
            ResolvedTypeData::GenericMethodArg(a, b, c) => {
                JsonResolvedTypeData::GenericMethodArg(a.index(), b.index(), c)
            }
            ResolvedTypeData::Ptr(inner) => JsonResolvedTypeData::Ptr(Box::new((*inner).into())),
            ResolvedTypeData::Type(tag) => JsonResolvedTypeData::Type(tag.into()),
            ResolvedTypeData::Primitive(prim) => JsonResolvedTypeData::Primitive(prim.into()),
            ResolvedTypeData::Blacklisted(tag) => JsonResolvedTypeData::Blacklisted(tag.into()),
            ResolvedTypeData::ByRef(inner) => {
                JsonResolvedTypeData::ByRef(Box::new((*inner).into()))
            }
            ResolvedTypeData::ByRefConst(inner) => {
                JsonResolvedTypeData::ByRefConst(Box::new((*inner).into()))
            }
        }
    }
}

/// Corresponds to element type signatures.
/// See ECMA-335, II.23.1.16
///
/// Defined at `il2cpp-blob.h:6`
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
pub enum JsonTypeEnum {
    /// End of list
    End,
    /// System.Void (void)
    Void,
    /// System.Boolean (bool)
    Boolean,
    /// System.Char (char)
    Char,
    /// System.SByte (sbyte)
    I1,
    /// System.Byte (byte)
    U1,
    /// System.Int16 (short)
    I2,
    /// System.UInt16 (ushort)
    U2,
    /// System.Int32 (int)
    I4,
    /// System.UInt32 (uint)
    U4,
    /// System.Int64 (long)
    I8,
    /// System.UInt64 (ulong)
    U8,
    /// System.Single (float)
    R4,
    /// System.Double (double)
    R8,
    /// System.String (string)
    String,
    Ptr,
    Byref,
    Valuetype,
    Class,
    /// Class generic parameter
    Var,
    Array,
    Genericinst,
    /// System.TypedReference
    Typedbyref,
    /// System.IntPtr
    I,
    /// System.UIntPtr
    U,
    Fnptr,
    /// System.Object (object)
    Object,
    /// Single-dimensioned zero-based array type
    Szarray,
    /// Method generic parameter
    Mvar,
    /// Required modifier
    CmodReqd,
    /// Optional modifier
    CmodOpt,
    Internal,
    Modifier,
    /// Sentinel for vararg method signature
    Sentinel,
    /// Denotes a local variable points to a pinned object
    Pinned,
    /// Used in custom attributes to specify an enum
    Enum,
}

impl ToString for JsonTypeEnum {
    fn to_string(&self) -> String {
        match self {
            JsonTypeEnum::End => "End".to_string(),
            JsonTypeEnum::Void => "Void".to_string(),
            JsonTypeEnum::Boolean => "Boolean".to_string(),
            JsonTypeEnum::Char => "Char".to_string(),
            JsonTypeEnum::I1 => "I1".to_string(),
            JsonTypeEnum::U1 => "U1".to_string(),
            JsonTypeEnum::I2 => "I2".to_string(),
            JsonTypeEnum::U2 => "U2".to_string(),
            JsonTypeEnum::I4 => "I4".to_string(),
            JsonTypeEnum::U4 => "U4".to_string(),
            JsonTypeEnum::I8 => "I8".to_string(),
            JsonTypeEnum::U8 => "U8".to_string(),
            JsonTypeEnum::R4 => "R4".to_string(),
            JsonTypeEnum::R8 => "R8".to_string(),
            JsonTypeEnum::String => "String".to_string(),
            JsonTypeEnum::Ptr => "Ptr".to_string(),
            JsonTypeEnum::Byref => "Byref".to_string(),
            JsonTypeEnum::Valuetype => "Valuetype".to_string(),
            JsonTypeEnum::Class => "Class".to_string(),
            JsonTypeEnum::Var => "Var".to_string(),
            JsonTypeEnum::Array => "Array".to_string(),
            JsonTypeEnum::Genericinst => "Genericinst".to_string(),
            JsonTypeEnum::Typedbyref => "Typedbyref".to_string(),
            JsonTypeEnum::I => "I".to_string(),
            JsonTypeEnum::U => "U".to_string(),
            JsonTypeEnum::Fnptr => "Fnptr".to_string(),
            JsonTypeEnum::Object => "Object".to_string(),
            JsonTypeEnum::Szarray => "Szarray".to_string(),
            JsonTypeEnum::Mvar => "Mvar".to_string(),
            JsonTypeEnum::CmodReqd => "CmodReqd".to_string(),
            JsonTypeEnum::CmodOpt => "CmodOpt".to_string(),
            JsonTypeEnum::Internal => "Internal".to_string(),
            JsonTypeEnum::Modifier => "Modifier".to_string(),
            JsonTypeEnum::Sentinel => "Sentinel".to_string(),
            JsonTypeEnum::Pinned => "Pinned".to_string(),
            JsonTypeEnum::Enum => "Enum".to_string(),
        }
    }
}

impl From<Il2CppTypeEnum> for JsonTypeEnum {
    fn from(value: Il2CppTypeEnum) -> Self {
        match value {
            Il2CppTypeEnum::End => JsonTypeEnum::End,
            Il2CppTypeEnum::Void => JsonTypeEnum::Void,
            Il2CppTypeEnum::Boolean => JsonTypeEnum::Boolean,
            Il2CppTypeEnum::Char => JsonTypeEnum::Char,
            Il2CppTypeEnum::I1 => JsonTypeEnum::I1,
            Il2CppTypeEnum::U1 => JsonTypeEnum::U1,
            Il2CppTypeEnum::I2 => JsonTypeEnum::I2,
            Il2CppTypeEnum::U2 => JsonTypeEnum::U2,
            Il2CppTypeEnum::I4 => JsonTypeEnum::I4,
            Il2CppTypeEnum::U4 => JsonTypeEnum::U4,
            Il2CppTypeEnum::I8 => JsonTypeEnum::I8,
            Il2CppTypeEnum::U8 => JsonTypeEnum::U8,
            Il2CppTypeEnum::R4 => JsonTypeEnum::R4,
            Il2CppTypeEnum::R8 => JsonTypeEnum::R8,
            Il2CppTypeEnum::String => JsonTypeEnum::String,
            Il2CppTypeEnum::Ptr => JsonTypeEnum::Ptr,
            Il2CppTypeEnum::Byref => JsonTypeEnum::Byref,
            Il2CppTypeEnum::Valuetype => JsonTypeEnum::Valuetype,
            Il2CppTypeEnum::Class => JsonTypeEnum::Class,
            Il2CppTypeEnum::Var => JsonTypeEnum::Var,
            Il2CppTypeEnum::Array => JsonTypeEnum::Array,
            Il2CppTypeEnum::Genericinst => JsonTypeEnum::Genericinst,
            Il2CppTypeEnum::Typedbyref => JsonTypeEnum::Typedbyref,
            Il2CppTypeEnum::I => JsonTypeEnum::I,
            Il2CppTypeEnum::U => JsonTypeEnum::U,
            Il2CppTypeEnum::Fnptr => JsonTypeEnum::Fnptr,
            Il2CppTypeEnum::Object => JsonTypeEnum::Object,
            Il2CppTypeEnum::Szarray => JsonTypeEnum::Szarray,
            Il2CppTypeEnum::Mvar => JsonTypeEnum::Mvar,
            Il2CppTypeEnum::CmodReqd => JsonTypeEnum::CmodReqd,
            Il2CppTypeEnum::CmodOpt => JsonTypeEnum::CmodOpt,
            Il2CppTypeEnum::Internal => JsonTypeEnum::Internal,
            Il2CppTypeEnum::Modifier => JsonTypeEnum::Modifier,
            Il2CppTypeEnum::Sentinel => JsonTypeEnum::Sentinel,
            Il2CppTypeEnum::Pinned => JsonTypeEnum::Pinned,
            Il2CppTypeEnum::Enum => JsonTypeEnum::Enum,
        }
    }
}

impl From<CsTypeTag> for JsonTypeTag {
    fn from(value: CsTypeTag) -> Self {
        match value {
            CsTypeTag::TypeDefinitionIndex(type_definition_index) => {
                JsonTypeTag::TypeDefinition(type_definition_index.index())
            }
            CsTypeTag::GenericInstantiation(generic_instantiation) => {
                JsonTypeTag::GenericInstantiation {
                    type_definition: generic_instantiation.tdi.index(),
                    inst: generic_instantiation.inst,
                }
            }
        }
    }
}

impl From<TypeDefinitionIndex> for JsonTypeTag {
    fn from(value: TypeDefinitionIndex) -> Self {
        JsonTypeTag::TypeDefinition(value.index())
    }
}
