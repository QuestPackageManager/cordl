//! Decodes C# custom attributes (`[Foo(1, Bar = 2)]`) straight out of the raw
//! `attribute_data`/`attribute_data_range` blob brocolib exposes but does not itself parse.
//!
//! ## Where this lives in libil2cpp
//!
//! The two tables come straight from the metadata file, as declared in
//! `vm/GlobalMetadataFileInternals.h`:
//! ```c
//! // GlobalMetadataFileInternals.h:236 - one entry per member that has custom attributes
//! typedef struct Il2CppCustomAttributeDataRange
//! {
//!     uint32_t token;       // the member's token, unique only within its own image
//!     uint32_t startOffset; // offset into the attributeData blob (see below)
//! } Il2CppCustomAttributeDataRange;
//!
//! // GlobalMetadataFileInternals.h:209 - customAttributeStart/Count slice the table above,
//! // once per image; this is why token lookups have to be scoped to an image, see
//! // CordlMetadata::tdi_to_image / custom_attributes_by_image.
//! typedef struct Il2CppImageDefinition
//! {
//!     ...
//!     CustomAttributeIndex customAttributeStart;
//!     uint32_t customAttributeCount;
//! } Il2CppImageDefinition;
//! ```
//! At runtime, `vm/GlobalMetadata.cpp` (`GetCustomAttributeTypeToken`, `GetCustomAttributeDataReader`,
//! around lines 851-891) does exactly this lookup: binary-search the current image's slice of
//! `Il2CppCustomAttributeDataRange` for a matching `token`, then hand the byte range starting at
//! `startOffset` (up to the next entry's `startOffset`) to `il2cpp::metadata::CustomAttributeDataReader`
//! (`metadata/CustomAttributeDataReader.h`/`.cpp`), which is what actually decodes it - and *is*
//! vendored, so the encoding below is ported directly from its source, not reverse-engineered.
//! `CustomAttributeDataReader.cpp` even documents the wire format in a header comment (lines
//! 11-64); `il2cpp-blob.h` gives `Il2CppTypeEnum`'s exact tag values, including two used only
//! here and nowhere in an ordinary type signature: `IL2CPP_TYPE_ENUM = 0x55` and
//! `IL2CPP_TYPE_IL2CPP_TYPE_INDEX = 0xff` (a `typeof(...)`; despite the name this is *not* the
//! real ECMA-335 `TYPE` tag, 0x50 - il2cpp made up its own). Unlike the runtime reader, this one
//! doesn't need the "next entry's startOffset" end bound: the format is self-terminating (every
//! count is written before the data it counts), so it just keeps reading until it's decoded
//! everything it said it would.
//!
//! ## The blob format
//!
//! Not a real POD layout (every array is variable-length, sized by the field just before it),
//! but this is the shape of it - the byte range found via
//! [`CordlMetadata::custom_attributes_by_image`], per member:
//! ```c
//! struct AttributeDataForMember {
//!     compressed_uint32 count;             // CustomAttributeDataReader.cpp:98
//!     uint32_t ctorMethodIndex[count];      // raw Read32, NOT compressed - .cpp:143-152
//!                                            // -> Il2CppMethodDefinition::declaringType
//!                                            //    (GlobalMetadataFileInternals.h:157) names
//!                                            //    the applied attribute's own class
//!
//!     // repeated `count` times, back to back, right after ctorMethodIndex[] - NOT
//!     // interleaved with it (.cpp:243-313)
//!     struct {
//!         compressed_uint32 argumentCount;
//!         compressed_uint32 fieldCount;
//!         compressed_uint32 propertyCount;
//!
//!         AttributeValue arguments[argumentCount];
//!
//!         struct {
//!             AttributeValue value;
//!             compressed_int32 memberIndex;       // relative to declaringTypeIndex's own
//!                                                    // Il2CppFieldDefinition list, unless...
//!             compressed_uint32 declaringTypeIndex; // ...only present when memberIndex (as
//!         } fields[fieldCount];                      // decoded, see read_named_arg_owner) was
//!                                                     // negative: the field was inherited
//!         struct {                                   // from a base class of the attribute
//!             AttributeValue value;                  // (.cpp:210-224)
//!             compressed_int32 memberIndex;          // same shape, over
//!             compressed_uint32 declaringTypeIndex;  // Il2CppPropertyDefinition instead
//!         } properties[propertyCount];
//!     } attributes[count];
//! };
//!
//! // BlobReader.cpp:28-168 (values), :170-191 (the leading tag byte)
//! struct AttributeValue {
//!     uint8_t typeTag; // an Il2CppTypeEnum (il2cpp-blob.h); IL2CPP_TYPE_ENUM (0x55) is
//!                       // followed by a compressed_int32 enumTypeIndex before the payload,
//!                       // itself read back as whatever `value__`'s type actually is
//!     union {
//!         uint8_t   asBool;     // BOOLEAN/I1/U1 - 1 byte, Read8
//!         Il2CppChar asChar;    // CHAR - 2 bytes
//!         uint16_t  asI2OrU2;   // I2/U2 - 2 bytes, Read16
//!         compressed_int32  asI4; // I4
//!         compressed_uint32 asU4; // U4
//!         uint64_t  asI8OrU8;   // I8/U8 - 8 bytes, Read64 (NOT compressed)
//!         float  asR4;
//!         double asR8;
//!         struct {                  // STRING
//!             compressed_int32 length; // -1 = null
//!             char utf8[length];       // raw UTF-8 bytes, not null-terminated, not UTF-16
//!         } asString;
//!         struct {                        // SZARRAY
//!             compressed_int32 length;       // -1 = null
//!             AttributeValue sharedTag;       // a tag+payload read unconditionally - its
//!                                              // payload is only ever used as every
//!                                              // element's value when elementsDiffer == 0
//!             uint8_t elementsDiffer;
//!             AttributeValue elements[length]; // if elementsDiffer == 1: every element
//!         } asArray;                            // (i == 0 included) is its own full,
//!                                                // independent AttributeValue here; if 0,
//!                                                // there's nothing more to read - every
//!                                                // element's value is just sharedTag's
//!         struct {                    // IL2CPP_TYPE_IL2CPP_TYPE_INDEX (0xff) - typeof(...);
//!             compressed_int32 typeIndex; // NOT the real ECMA-335 TYPE tag (0x50)
//!         } asType;                       // -1 = null; else an index into
//!     };                                  // metadata_registration.types
//! };
//! ```
//!
//! This is a best-effort decoder: any inconsistency (an unrecognized element-type tag, a
//! truncated read, an out-of-range member index) aborts decoding for that member and logs a
//! warning instead of panicking or guessing - a whole codegen run should never crash over one
//! attribute, and a dropped attribute comment is far cheaper than a silently wrong one.

use std::io::{self, Cursor, Read};

use brocolib::{
    global_metadata::{MethodIndex, Token, TypeDefinitionIndex},
    runtime_metadata::{Il2CppType, Il2CppTypeEnum},
};
use byteorder::ReadBytesExt;
use itertools::Itertools;
use log::warn;

use crate::{Endian, helpers::cursor::ReadBytesExtensions};

use super::{
    cs_members::CsValue, cs_type_tag::CsTypeTag, metadata::CordlMetadata,
    type_extensions::TypeDefinitionExtensions,
};

#[derive(Debug, Clone, PartialEq)]
pub struct CsAttributeNamedArg {
    pub name: String,
    pub value: CsValue,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CsAttribute {
    pub attribute_type: CsTypeTag,
    pub arguments: Vec<CsValue>,
    pub named_arguments: Vec<CsAttributeNamedArg>,
}

impl CsAttribute {
    /// Renders as a C#-style attribute, e.g. `[Obsolete("reason", true)]`
    pub fn to_comment_string(&self, metadata: &CordlMetadata) -> String {
        let attribute_td =
            &metadata.metadata.global_metadata.type_definitions[self.attribute_type.get_tdi()];
        let full_name = attribute_td.full_name(metadata.metadata, true);

        // format short name by stripping the namespace and the "Attribute" suffix, if present
        let short_name = full_name.rsplit('.').next().unwrap_or(&full_name);
        let short_name = short_name.strip_suffix("Attribute").unwrap_or(short_name);

        let parts = self
            .arguments
            .iter()
            .map(|v| v.to_display_string(metadata))
            .chain(
                self.named_arguments
                    .iter()
                    .map(|a| format!("{} = {}", a.name, a.value.to_display_string(metadata))),
            )
            .collect_vec();

        if parts.is_empty() {
            format!("[{short_name}]")
        } else {
            format!("[{short_name}({})]", parts.join(", "))
        }
    }
}

/// Renders every attribute of a member as its own line, e.g. `"[Serializable]\n[Obsolete]"`.
/// `None` if `attrs` is empty. The caller (a `Writable` impl in the cpp backend) is responsible
/// for splitting this on `\n` and emitting each line with its own `///`/`//` prefix.
///
/// Only usable where the comment gets its own line(s) - i.e. not for a parameter's inline
/// `/* ... */` comment within a single-line signature, where a `\n` would break formatting rather
/// than aid it. Use [`format_attributes_inline`] there instead.
pub fn format_attributes(attrs: &[CsAttribute], metadata: &CordlMetadata) -> Option<String> {
    (!attrs.is_empty()).then(|| {
        attrs
            .iter()
            .map(|a| a.to_comment_string(metadata))
            .join("\n")
    })
}

/// Like [`format_attributes`], but joins multiple attributes with a space instead of a newline -
/// for a comment that must stay on one line, e.g. a parameter's inline `/* [Attr] */` comment.
pub fn format_attributes_inline(attrs: &[CsAttribute], metadata: &CordlMetadata) -> Option<String> {
    (!attrs.is_empty()).then(|| {
        attrs
            .iter()
            .map(|a| a.to_comment_string(metadata))
            .join(" ")
    })
}

/// Prepends the attribute comment lines (if any) before an existing brief comment line, e.g.
/// combining `[Obsolete]` and `"Field foo, offset 0x10"` into
/// `"[Obsolete]\nField foo, offset 0x10"`. The result may contain `\n` - the last line is always
/// the original `comment`, everything before it is one attribute per line - see
/// [`format_attributes`] and the `write_brief` helper (`cpp/cpp_members_serialize.rs`) that
/// depends on this ordering to tag only the last line with `@brief`.
pub fn prefix_with_attributes(
    attrs: &[CsAttribute],
    metadata: &CordlMetadata,
    comment: impl Into<String>,
) -> String {
    let comment = comment.into();
    match format_attributes(attrs, metadata) {
        Some(attr_comment) if comment.is_empty() => attr_comment,
        Some(attr_comment) => format!("{attr_comment}\n{comment}"),
        None => comment,
    }
}

/// Finds and decodes the custom attributes attached to `token`, a member declared on
/// `declaring_tdi` (used only to find the right image - tokens are only unique within it).
/// Returns an empty vec if the member has none, or if decoding fails.
pub fn decode_custom_attributes(
    metadata: &CordlMetadata,
    declaring_tdi: TypeDefinitionIndex,
    token: Token,
) -> Vec<CsAttribute> {
    let Some(image_index) = metadata.tdi_to_image.get(&declaring_tdi) else {
        return Vec::new();
    };
    let Some(image_attrs) = metadata
        .custom_attributes_by_image
        .get(&image_index.index())
    else {
        return Vec::new();
    };
    let Some(&start_offset) = image_attrs.get(&token.0) else {
        return Vec::new();
    };

    match decode_custom_attributes_at(metadata, start_offset) {
        Ok(attrs) => attrs,
        Err(e) => {
            warn!(
                "Failed to decode custom attribute data at offset 0x{start_offset:x} for token {:#x}: {e}",
                token.0
            );
            Vec::new()
        }
    }
}

/// Decodes the custom attributes attached to a member, given the start offset of its attribute
/// data in the `attribute_data` blob. Returns an empty vec if decoding fails.
///
/// Ports `CustomAttributeDataReader::ReadAndVisitCustomAttributeImpl`
/// (`metadata/CustomAttributeDataReader.cpp:237`), by way of its constructor
/// (same file, line 94) reading `count` and `CustomAttributeDataReader::GetDataBufferStart`
/// (line 143) placing the argument data right after it.
fn decode_custom_attributes_at(
    metadata: &CordlMetadata,
    start_offset: u32,
) -> io::Result<Vec<CsAttribute>> {
    let blob = metadata.metadata.global_metadata.attribute_data.as_vec();
    let mut cursor = Cursor::new(blob.as_slice());
    cursor.set_position(start_offset as u64);

    // CustomAttributeDataReader::CustomAttributeDataReader, CustomAttributeDataReader.cpp:94
    let count = cursor.read_compressed_u32::<Endian>()?;

    // CustomAttributeDataReader::GetDataBufferStart, CustomAttributeDataReader.cpp:143-146:
    // the ctor MethodIndex for every attribute is stored up front, one raw (uncompressed) u32
    // each - `((uint32_t*)bufferStart) + count` - followed by every attribute's argument data
    // back to back, NOT interleaved attribute-by-attribute.
    let ctor_block_start = cursor.position();
    let mut data_pos = ctor_block_start + count as u64 * 4;

    let mut attributes = Vec::with_capacity(count as usize);
    for i in 0..count {
        cursor.set_position(ctor_block_start + i as u64 * 4);
        // CustomAttributeDataReader::IterateAttributeCtorsImpl, CustomAttributeDataReader.cpp:148:
        // `MethodIndex ctorIndex = utils::Read32(ctorBuffer);` - a raw 4-byte read, not compressed.
        let ctor_index = cursor.read_u32::<Endian>()?;

        // Il2CppMethodDefinition::declaringType, GlobalMetadataFileInternals.h:157 - the
        // attribute's own class is whatever class declares the constructor that was called
        // (`Il2CppClass* attrClass = ctor->klass;`, CustomAttributeDataReader.cpp:241).
        let method_def = &metadata.metadata.global_metadata.methods[MethodIndex::new(ctor_index)];
        let attribute_tdi = method_def.declaring_type;
        let attribute_td = &metadata.metadata.global_metadata.type_definitions[attribute_tdi];

        cursor.set_position(data_pos);

        // CustomAttributeDataReader::ReadAndVisitCustomAttributeImpl, CustomAttributeDataReader.cpp:243-247
        let argument_count = cursor.read_compressed_u32::<Endian>()?;
        let field_count = cursor.read_compressed_u32::<Endian>()?;
        let property_count = cursor.read_compressed_u32::<Endian>()?;

        // CustomAttributeDataReader.cpp:267-274 (ReadAttributeDataValue per positional argument)
        let arguments = (0..argument_count)
            .map(|_| read_attribute_value(&mut cursor, metadata))
            .collect::<io::Result<Vec<_>>>()?;

        // CustomAttributeDataReader.cpp:281-296: a value, then which field it's assigned to
        let mut named_arguments = Vec::with_capacity((field_count + property_count) as usize);
        for _ in 0..field_count {
            let value = read_attribute_value(&mut cursor, metadata)?;
            let (declaring_td, index) = read_named_arg_owner(&mut cursor, metadata, attribute_td)?;
            // Il2CppFieldDefinition, GlobalMetadataFileInternals.h:113 (`klass->fields[fieldIndex]`
            // there, `declaring_td.fields(metadata)[index]` here)
            let field_def = declaring_td
                .fields(metadata.metadata)
                .get(index)
                .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "named argument field index out of range".to_string()))?;
            named_arguments.push(CsAttributeNamedArg {
                name: field_def.name(metadata.metadata).to_string(),
                value,
            });
        }
        // CustomAttributeDataReader.cpp:298-313: same shape, for properties instead of fields
        for _ in 0..property_count {
            let value = read_attribute_value(&mut cursor, metadata)?;
            let (declaring_td, index) = read_named_arg_owner(&mut cursor, metadata, attribute_td)?;
            // Il2CppPropertyDefinition, GlobalMetadataFileInternals.h:179
            let property_def = declaring_td
                .properties(metadata.metadata)
                .get(index)
                .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "named argument property index out of range".to_string()))?;
            named_arguments.push(CsAttributeNamedArg {
                name: property_def.name(metadata.metadata).to_string(),
                value,
            });
        }

        data_pos = cursor.position();

        attributes.push(CsAttribute {
            attribute_type: attribute_tdi.into(),
            arguments,
            named_arguments,
        });
    }

    Ok(attributes)
}



/// Reads the (possibly negative-encoded) field/property index of a named argument, along with
/// the type declaring it - the attribute's own type unless the member was inherited, in which
/// case a type index for the actual declaring base class follows.
///
/// Ports `ReadCustomAttributeNamedArgumentClassAndIndex`, `CustomAttributeDataReader.cpp:210`.
fn read_named_arg_owner<'m>(
    cursor: &mut Cursor<&[u8]>,
    metadata: &'m CordlMetadata,
    attribute_td: &'m brocolib::global_metadata::Il2CppTypeDefinition,
) -> io::Result<(&'m brocolib::global_metadata::Il2CppTypeDefinition, usize)> {
    let member_index = cursor.read_compressed_i32::<Endian>()?;
    if member_index >= 0 {
        return Ok((attribute_td, member_index as usize));
    }

    let member_index = -(member_index + 1);
    let type_index = cursor.read_compressed_u32::<Endian>()?;
    let declaring_td =
        &metadata.metadata.global_metadata.type_definitions[TypeDefinitionIndex::new(type_index)];

    Ok((declaring_td, member_index as usize))
}

/// `Il2CppTypeEnum` tags relevant to custom attribute values, straight out of
/// `il2cpp-blob.h` (which itself cites ECMA-335 II.23.1.16 for most of them). This intentionally
/// doesn't reuse brocolib's own [`Il2CppTypeEnum`]: `IL2CPP_TYPE_IL2CPP_TYPE_INDEX` isn't one of
/// its variants (nothing else needs it - it's not a real ECMA-335 element type, just il2cpp's
/// own made-up tag for a `typeof(...)` constant), and brocolib's `Il2CppTypeEnum::from_ty`
/// doesn't map that byte value at all - so it's handled here as [`TaggedType::Type`] instead,
/// alongside real `Il2CppTypeEnum` tags.
#[derive(Clone, Copy)]
enum TaggedType {
    Elem(Il2CppTypeEnum),
    /// `IL2CPP_TYPE_IL2CPP_TYPE_INDEX` (0xff) - `typeof(...)`. Not the real ECMA-335 `TYPE`
    /// element type (0x50); il2cpp never emits that one here.
    Type,
    /// `IL2CPP_TYPE_ENUM`; carries the index into `metadata_registration.types` naming the enum
    Enum(usize),
}

/// Mirrors the `switch` in `BlobReader::GetConstantValueFromBlob` (`vm-utils/BlobReader.cpp`),
/// restricted to the tags it (and `ReadEncodedTypeEnum`) actually handle for custom attribute
/// values. brocolib's own byte-to-`Il2CppTypeEnum` mapping (`Il2CppTypeEnum::from_ty`) isn't
/// `pub`, so this has to be its own, smaller table rather than reusing it.
fn il2cpp_type_enum_from_byte(b: u8) -> Option<Il2CppTypeEnum> {
    Some(match b {
        0x02 => Il2CppTypeEnum::Boolean,
        0x03 => Il2CppTypeEnum::Char,
        0x04 => Il2CppTypeEnum::I1,
        0x05 => Il2CppTypeEnum::U1,
        0x06 => Il2CppTypeEnum::I2,
        0x07 => Il2CppTypeEnum::U2,
        0x08 => Il2CppTypeEnum::I4,
        0x09 => Il2CppTypeEnum::U4,
        0x0a => Il2CppTypeEnum::I8,
        0x0b => Il2CppTypeEnum::U8,
        0x0c => Il2CppTypeEnum::R4,
        0x0d => Il2CppTypeEnum::R8,
        0x0e => Il2CppTypeEnum::String,
        0x1d => Il2CppTypeEnum::Szarray,
        0x55 => Il2CppTypeEnum::Enum,
        0xFF => Il2CppTypeEnum::Internal, // `IL2CPP_TYPE_IL2CPP_TYPE_INDEX` - handled separately as `TaggedType::Type`
        _ => return None,
    })
}

/// Ports `BlobReader::ReadEncodedTypeEnum`, `vm-utils/BlobReader.cpp:170`.
fn read_encoded_type_tag(cursor: &mut Cursor<&[u8]>) -> io::Result<TaggedType> {
    let tag = cursor.read_u8()?;
    
    match il2cpp_type_enum_from_byte(tag) {
        Some(Il2CppTypeEnum::Internal) => Ok(TaggedType::Type),
        Some(Il2CppTypeEnum::Enum) => {
            let enum_type_idx = cursor.read_compressed_i32::<Endian>()?;
            Ok(TaggedType::Enum(enum_type_idx as usize))
        }
        Some(other) => Ok(TaggedType::Elem(other)),
        None => Err({
            let msg: &str = &format!(
                    "unknown custom attribute element type tag 0x{tag:02x}"
                );
            io::Error::new(io::ErrorKind::InvalidData, msg.to_string())
        }),
    }
}

fn read_attribute_value(
    cursor: &mut Cursor<&[u8]>,
    metadata: &CordlMetadata,
) -> io::Result<CsValue> {
    let tagged = read_encoded_type_tag(cursor)?;
    read_value_of_tagged_type(tagged, cursor, metadata)
}

fn read_value_of_tagged_type(
    tagged: TaggedType,
    cursor: &mut Cursor<&[u8]>,
    metadata: &CordlMetadata,
) -> io::Result<CsValue> {
    match tagged {
        TaggedType::Type => {
            let idx = cursor.read_compressed_i32::<Endian>()?;
            if idx < 0 {
                Ok(CsValue::Null)
            } else {
                let ty = metadata
                    .metadata_registration
                    .types
                    .get(idx as usize)
                    .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "typeof() type index out of range".to_string()))?;
                Ok(CsValue::Type(CsTypeTag::from_type_data(
                    ty.data,
                    metadata.metadata,
                )))
            }
        }
        TaggedType::Enum(enum_type_idx) => {
            let enum_ty = metadata
                .metadata_registration
                .types
                .get(enum_type_idx)
                .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "enum type index out of range".to_string()))?;
            let tag: CsTypeTag = CsTypeTag::from_type_data(enum_ty.data, metadata.metadata);
            let backing = enum_backing_element_type(metadata, enum_ty);
            let value = read_primitive_value(backing, cursor, metadata)?;
            Ok(CsValue::Enum(tag, Box::new(value)))
        }
        TaggedType::Elem(elem) => read_primitive_value(elem, cursor, metadata),
    }
}

/// Ports `BlobReader::GetConstantValueFromBlob`, `vm-utils/BlobReader.cpp:28`, for the
/// non-enum tags - `TaggedType::Enum` is unwrapped by [`read_value_of_tagged_type`] first,
/// matching `ReadEncodedTypeEnum` resolving `IL2CPP_TYPE_ENUM` to its backing type up front
/// (`vm-utils/BlobReader.cpp:170`) before `GetConstantValueFromBlob` ever sees it. `elem_ty` is
/// never `Il2CppTypeEnum::Enum` here for that same reason, nor anything [`TaggedType::Type`]'s
/// separate 0xff sentinel would produce - both are handled one level up, in
/// [`read_value_of_tagged_type`].
fn read_primitive_value(
    elem_ty: Il2CppTypeEnum,
    cursor: &mut Cursor<&[u8]>,
    metadata: &CordlMetadata,
) -> io::Result<CsValue> {
    match elem_ty {
        Il2CppTypeEnum::String => {
            let len = cursor.read_compressed_i32::<Endian>()?;
            if len < 0 {
                Ok(CsValue::Null)
            } else {
                let mut buf = vec![0u8; len as usize];
                cursor.read_exact(&mut buf)?;
                // matches CsType::default_value_blob's own String handling
                Ok(CsValue::String(
                    String::from_utf8_lossy(&buf).escape_default().to_string(),
                ))
            }
        }
        Il2CppTypeEnum::Szarray => {
            let len = cursor.read_compressed_i32::<Endian>()?;
            if len < 0 {
                Ok(CsValue::Null)
            } else {
                // BlobReader.cpp:97-99: a tag is always read right here, unconditionally -
                // even when every element carries its own (`elementsAreDifferent == 1`). In
                // that case this one is a dummy: its bytes still have to be consumed to stay
                // in sync with the writer, but `elementType = arrayElementType` (line 111) is
                // then immediately overwritten for every single element, i == 0 included
                // (line 112-113), so its decoded value here must never be reused.
                let shared_tag = read_encoded_type_tag(cursor)?;
                let elements_differ = cursor.read_u8()?;

                let mut items = Vec::with_capacity(len as usize);
                for _ in 0..len {
                    let value = if elements_differ == 1 {
                        read_attribute_value(cursor, metadata)?
                    } else {
                        read_value_of_tagged_type(shared_tag, cursor, metadata)?
                    };
                    items.push(value);
                }
                Ok(CsValue::Array(items))
            }
        }
        // everything else is shared with field/parameter default value decoding
        _ => CsValue::read_primitive(elem_ty, cursor),
    }
}

/// Finds an enum's backing integer type ([`TypeDefinitionExtensions::enum_backing_type`]).
/// Falls back to `I4` (the overwhelmingly common case, and what a bare `enum Foo { ... }`
/// backs to) if it isn't on record.
fn enum_backing_element_type(metadata: &CordlMetadata, enum_ty: &Il2CppType) -> Il2CppTypeEnum {
    let brocolib::runtime_metadata::TypeData::TypeDefinitionIndex(tdi) = enum_ty.data else {
        unreachable!("enum type is not a TypeDefinitionIndex: {enum_ty:?}");
    };
    let enum_td = &metadata.metadata.global_metadata.type_definitions[tdi];

    enum_td
        .enum_backing_type(metadata.metadata)
        .map_or(Il2CppTypeEnum::I4, |backing_ty| backing_ty.ty)
}
