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

use std::fmt;

impl fmt::Display for JsonTypeEnum {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            JsonTypeEnum::End => "End",
            JsonTypeEnum::Void => "Void",
            JsonTypeEnum::Boolean => "Boolean",
            JsonTypeEnum::Char => "Char",
            JsonTypeEnum::I1 => "I1",
            JsonTypeEnum::U1 => "U1",
            JsonTypeEnum::I2 => "I2",
            JsonTypeEnum::U2 => "U2",
            JsonTypeEnum::I4 => "I4",
            JsonTypeEnum::U4 => "U4",
            JsonTypeEnum::I8 => "I8",
            JsonTypeEnum::U8 => "U8",
            JsonTypeEnum::R4 => "R4",
            JsonTypeEnum::R8 => "R8",
            JsonTypeEnum::String => "String",
            JsonTypeEnum::Ptr => "Ptr",
            JsonTypeEnum::Byref => "Byref",
            JsonTypeEnum::Valuetype => "Valuetype",
            JsonTypeEnum::Class => "Class",
            JsonTypeEnum::Var => "Var",
            JsonTypeEnum::Array => "Array",
            JsonTypeEnum::Genericinst => "Genericinst",
            JsonTypeEnum::Typedbyref => "Typedbyref",
            JsonTypeEnum::I => "I",
            JsonTypeEnum::U => "U",
            JsonTypeEnum::Fnptr => "Fnptr",
            JsonTypeEnum::Object => "Object",
            JsonTypeEnum::Szarray => "Szarray",
            JsonTypeEnum::Mvar => "Mvar",
            JsonTypeEnum::CmodReqd => "CmodReqd",
            JsonTypeEnum::CmodOpt => "CmodOpt",
            JsonTypeEnum::Internal => "Internal",
            JsonTypeEnum::Modifier => "Modifier",
            JsonTypeEnum::Sentinel => "Sentinel",
            JsonTypeEnum::Pinned => "Pinned",
            JsonTypeEnum::Enum => "Enum",
        };
        write!(f, "{name}")
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
