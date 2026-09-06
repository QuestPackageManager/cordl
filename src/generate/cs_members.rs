use bitflags::bitflags;
use brocolib::{global_metadata::MethodIndex, runtime_metadata::Il2CppTypeEnum};
use byteorder::ReadBytesExt;
use itertools::Itertools;

use crate::Endian;
use crate::data::type_resolver::ResolvedType;
use crate::generate::metadata::CordlMetadata;
use crate::helpers::cursor::ReadBytesExtensions;

use std::hash::Hash;
use std::io::{self, Cursor};

use super::cs_attributes::CsAttribute;
use super::cs_type_tag::CsTypeTag;

#[derive(Debug, Eq, Hash, PartialEq, Clone, Default)]
pub struct CsGenericContainer {
    pub args: Vec<CsGenericArg>,
}

#[derive(Debug, Eq, Hash, PartialEq, Clone, Default)]
pub struct CsGenericArg {
    /// Types this parameter must be convertible to, e.g. `where T : Foo, IBar<T>`
    pub constraints: Vec<CsGenericConstraint>,
    pub name: String,
    pub index: u16,
}

#[derive(Debug, Eq, Hash, PartialEq, Clone)]
pub enum CsGenericConstraint {
    /// A type constraint, e.g. `where T : Foo`
    Resolved(ResolvedType),

    // Constraints that aren't expressed as a type, e.g. `where T : struct, new()`
    /// A constraint that the type must be a value type (struct) e.g where T: struct
    Struct,
    /// A constraint that the type must be a reference type (class) e.g where T: class
    Class,
    /// A constraint that the type must have a default constructor e.g where T: new()
    DefaultConstructor,
    /// A constraint that the type must be covariant e.g where T: out T
    Covariant,
    /// A constraint that the type must be contravariant e.g where T: in T
    Contravariant,
}

impl CsGenericContainer {
    pub fn just_names(&self) -> impl Iterator<Item = &String> {
        self.args.iter().map(|t| &t.name)
    }
}

#[derive(Clone, Debug, Default, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct CsMethodData {
    pub estimated_size: Option<usize>,
    pub addrs: Option<u64>,
    pub slot: Option<u16>,
}

#[derive(Clone, Debug, PartialEq, PartialOrd)]
pub enum CsValue {
    String(String),
    Char(String),
    Bool(bool),

    U8(u8),
    U16(u16),
    U32(u32),
    U64(u64),

    I8(i8),
    I16(i16),
    I32(i32),
    I64(i64),

    F32(f32),
    F64(f64),

    /// `typeof(...)`
    Type(CsTypeTag),
    /// An SZARRAY constant, e.g. `new[] { 1, 2, 3 }`. Only ever produced by custom attribute
    /// arguments - field/parameter default values never carry array constants.
    Array(Vec<CsValue>),
    /// An enum constant, together with its type, e.g. `(MyEnum)5`. Only ever produced by
    /// custom attribute arguments - see [`Array`](CsValue::Array).
    Enum(CsTypeTag, Box<CsValue>),

    Null,
}

impl CsValue {
    /// Renders as C# source syntax, e.g. `"reason"`, `(MyEnum)5`, `new[] { 1, 2 }` - for use in
    /// a custom attribute comment. Unlike [`std::fmt::Display`] (a *compilable* C++ literal,
    /// e.g. `static_cast<uint8_t>(0x5u)`, for field/parameter default values - see
    /// `cpp/cpp_type.rs`), this takes a [`CordlMetadata`] so it can resolve the [`CsTypeTag`]s
    /// [`CsValue::Type`] and [`CsValue::Enum`] carry into actual names.
    pub fn to_display_string(&self, metadata: &CordlMetadata) -> String {
        match self {
            CsValue::String(v) => format!("\"{v}\""),
            CsValue::Char(v) => format!("'{v}'"),
            CsValue::Bool(v) => v.to_string(),
            CsValue::U8(v) => v.to_string(),
            CsValue::U16(v) => v.to_string(),
            CsValue::U32(v) => v.to_string(),
            CsValue::U64(v) => v.to_string(),
            CsValue::I8(v) => v.to_string(),
            CsValue::I16(v) => v.to_string(),
            CsValue::I32(v) => v.to_string(),
            CsValue::I64(v) => v.to_string(),
            CsValue::F32(v) => v.to_string(),
            CsValue::F64(v) => v.to_string(),
            CsValue::Null => "null".to_string(),
            CsValue::Array(items) => format!(
                "new[] {{ {} }}",
                items
                    .iter()
                    .map(|v| v.to_display_string(metadata))
                    .join(", ")
            ),
            CsValue::Type(tag) => {
                let td = &metadata.metadata.global_metadata.type_definitions[tag.get_tdi()];
                format!("typeof({})", td.full_name(metadata.metadata, true))
            }
            CsValue::Enum(tag, value) => {
                let td = &metadata.metadata.global_metadata.type_definitions[tag.get_tdi()];
                format!(
                    "({}){}",
                    td.full_name(metadata.metadata, true),
                    value.to_display_string(metadata)
                )
            }
        }
    }

    /// Reads a single value of type `ty` from `cursor`, in the binary encoding il2cpp shares
    /// between field/parameter default values (`CsType::default_value_blob`) and custom
    /// attribute arguments (`cs_attributes.rs`) - the two places that call this. Doesn't handle
    /// `String`: its "this value is null" convention differs between those two blob formats
    /// (default values write an empty string; attribute arguments use a dedicated -1 length),
    /// so each caller decodes it itself.
    pub fn read_primitive(ty: Il2CppTypeEnum, cursor: &mut Cursor<&[u8]>) -> io::Result<CsValue> {
        Ok(match ty {
            Il2CppTypeEnum::Boolean => CsValue::Bool(cursor.read_u8()? != 0),
            Il2CppTypeEnum::I1 => CsValue::I8(cursor.read_i8()?),
            Il2CppTypeEnum::I2 => CsValue::I16(cursor.read_i16::<Endian>()?),
            Il2CppTypeEnum::I4 => CsValue::I32(cursor.read_compressed_i32::<Endian>()?),
            // TODO: We assume 64 bit
            Il2CppTypeEnum::I | Il2CppTypeEnum::I8 => CsValue::I64(cursor.read_i64::<Endian>()?),
            Il2CppTypeEnum::U1 => CsValue::U8(cursor.read_u8()?),
            Il2CppTypeEnum::U2 => CsValue::U16(cursor.read_u16::<Endian>()?),
            Il2CppTypeEnum::U4 => CsValue::U32(cursor.read_compressed_u32::<Endian>()?),
            // TODO: We assume 64 bit
            Il2CppTypeEnum::U | Il2CppTypeEnum::U8 => CsValue::U64(cursor.read_u64::<Endian>()?),
            Il2CppTypeEnum::R4 => CsValue::F32(cursor.read_f32::<Endian>()?),
            Il2CppTypeEnum::R8 => CsValue::F64(cursor.read_f64::<Endian>()?),
            Il2CppTypeEnum::Char => {
                let v = cursor.read_u16::<Endian>()?;
                CsValue::Char(String::from_utf16_lossy(&[v]).escape_default().to_string())
            }
            _ => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("CsValue::read_primitive: unsupported type {ty:?}"),
                ));
            }
        })
    }
}

/// Explicit layout
/// il2cpp basically turns each field into 2 structs within a union:
/// 1 which is packed with size 1, and padded with offset to fit to the end
/// the other which has the same padding and layout, except this one is for alignment so it's just packed as the parent struct demands
/// union {
///      [[pack(1)]]
///      struct {
///          byte __field_padding = size(offset)
///          T field
///      }
///      [[pack(default)]]
///      struct {
///          byte __field_padding_forAlignment = size(offset)
///          T __field_forAlignment
///      }... per field
/// }
///
#[derive(Clone, Debug, PartialEq)]
pub struct CsField {
    pub name: String,
    pub field_ty: ResolvedType,
    pub instance: bool,
    pub readonly: bool,
    // is C# const (constant evaluated)
    // could be assumed from value though
    pub is_const: bool,

    pub offset: Option<u32>,
    pub size: usize,

    pub value: Option<CsValue>,
    pub brief_comment: Option<String>,
    pub attributes: Vec<CsAttribute>,
}

// not Hash/Eq: CsAttributeValue can hold floats
#[derive(Clone, Debug, PartialEq)]
pub struct CsProperty {
    pub name: String,
    pub prop_ty: ResolvedType,
    pub instance: bool,
    pub getter: Option<(MethodIndex, String)>,
    pub setter: Option<(MethodIndex, String)>,
    /// Whether this property is one that's indexable (accessor methods take an index argument)
    pub indexable: bool,
    pub brief_comment: Option<String>,
    pub attributes: Vec<CsAttribute>,
}

bitflags! {
    #[derive(Debug, Clone, Hash, PartialEq, PartialOrd, Eq, Ord)]
    pub struct CsParamFlags: u8 {
        const REF = 1;
        const IN = 1 << 1;
        const OUT = 1 << 2;
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct CsParam {
    pub name: String,
    pub il2cpp_ty: ResolvedType,
    // TODO: Use bitflags to indicate these attributes
    // May hold:
    // const
    // May hold one of:
    // *
    // &
    // &&
    pub modifiers: CsParamFlags,
    pub def_value: Option<CsValue>,
    pub attributes: Vec<CsAttribute>,
}

bitflags! {
    #[derive(Clone, Debug, PartialEq)]
    pub struct CSMethodFlags: u32 {
        const STATIC = 0b00000001;
        const VIRTUAL = 0b00000010;
        const OPERATOR = 0b00000100;
        const ABSTRACT = 0b00001000;
        const OVERRIDE = 0b00010000;
        const FINAL = 0b00100000;
        const SPECIAL_NAME = 0b01000000;
        const UNSAFE = 0b10000000;
    }
}

// TODO: Generics
#[derive(Clone, Debug, PartialEq)]
pub struct CsMethod {
    pub name: String,
    pub method_index: MethodIndex,
    pub return_type: ResolvedType,
    pub parameters: Vec<CsParam>,
    pub instance: bool,
    pub generic_container: Option<CsGenericContainer>,
    pub brief: Option<String>,

    pub declaring_type: CsTypeTag,

    pub method_data: CsMethodData,
    pub method_flags: CSMethodFlags,
    /// if this method is a generic instantiation, the types used to instantiate it
    /// are stored here
    pub generic_instatiation: Option<Vec<ResolvedType>>,
    pub attributes: Vec<CsAttribute>,
}

// TODO: Generics
#[derive(Clone, Debug)]
pub struct CsConstructor {
    pub name: String,
    pub parameters: Vec<CsParam>,
    pub generic_container: Option<CsGenericContainer>,
    pub attributes: Vec<CsAttribute>,
}

impl PartialEq for CsConstructor {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name
            && self.parameters == other.parameters
            && self.generic_container == other.generic_container
    }
}
