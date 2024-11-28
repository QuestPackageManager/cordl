use itertools::Itertools;

use crate::generate::cs_members::CsField;

use super::{
    config::RustGenerationConfig,
    rust_members::{RustField, RustItem, RustStruct, RustUnion, Visibility},
    rust_name_resolver::RustNameResolver,
};

// inspired by what il2cpp does for explicitly laid out types
pub(crate) fn pack_fields_into_single_union(fields: &[RustField]) -> RustUnion {
    // get the min offset to use as a base for the packed structs
    let min_offset = fields.iter().map(|f| f.offset).min().unwrap_or(0);

    let packed_structs = fields
        .iter()
        .cloned()
        .flat_map(|field| {
            let structs = field_into_offset_structs(min_offset, field);

            vec![structs.0, structs.1]
        })
        .collect_vec();

    let fields = packed_structs
        .into_iter()
        .enumerate()
        .map(|(i, struc)| RustField {
            name: format!("struct{}", i),
            field_type: RustItem::Struct(struc),
            visibility: Visibility::Private,
            offset: 0,
        })
        .collect_vec();

    RustUnion {
        name: "packed_union".to_owned(),
        fields,
        visibility: Visibility::Private,
    }
}

pub(crate) fn field_into_offset_structs(
    _min_offset: u32,
    field: RustField,
) -> (RustStruct, RustStruct) {
    // il2cpp basically turns each field into 2 structs within a union:
    // 1 which is packed with size 1, and padded with offset to fit to the end
    // the other which has the same padding and layout, except this one is for alignment so it's just packed as the parent struct demands

    let actual_offset = field.offset;
    let padding = field.offset;

    let f_name = &field.name;

    let packed_padding_cpp_name = format!("{f_name}_padding");
    let alignment_padding_cpp_name = format!("{f_name}_padding_forAlignment");
    let alignment_cpp_name = format!("{f_name}_forAlignment");

    let packed_padding_field = RustField {
        name: packed_padding_cpp_name,
        field_type: RustItem::NamedType(format!("vec![0x{padding:x}; u8]")),
        visibility: Visibility::Public,
        offset: actual_offset,
        // brief_comment: Some(format!("Padding field 0x{padding:x}")),
        // const_expr: false,
        // cpp_name: packed_padding_cpp_name,
        // field_ty: "uint8_t".into(),
        // offset: Some(*actual_offset),
        // instance: true,
        // is_private: false,
        // readonly: false,
        // value: None,
    };

    let alignment_padding_field = RustField {
        name: alignment_padding_cpp_name,
        field_type: RustItem::NamedType(format!("vec![0x{padding:x}; u8]")),
        visibility: Visibility::Public,
        offset: actual_offset,
        // brief_comment: Some(format!("Padding field 0x{padding:x} for alignment")),
        // const_expr: false,
        // cpp_name: alignment_padding_cpp_name,
        // field_ty: "uint8_t".into(),
        // offset: Some(*actual_offset),
        // instance: true,
        // is_private: false,
        // readonly: false,
        // value: None,
    };

    let alignment_field = RustField {
        name: alignment_cpp_name,
        ..field.clone()
    };

    let packed_field = RustField {
        visibility: Visibility::Private,
        ..field
    };

    let packed_struct = RustStruct {
        fields: vec![packed_padding_field.clone(), packed_field.clone()],
        name: "packed_struct".to_owned(),
        visibility: Visibility::Private,

        packing: Some(1),
    };

    let alignment_struct = RustStruct {
        fields: vec![(alignment_padding_field), (alignment_field)],
        name: "alignment_struct".to_owned(),

        packing: None,
        visibility: Visibility::Private,
    };

    (packed_struct, alignment_struct)
}
