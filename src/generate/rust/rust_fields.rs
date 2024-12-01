use brocolib::{global_metadata::TypeDefinitionIndex, runtime_metadata::Il2CppTypeEnum};
use itertools::Itertools;
use log::warn;
use quote::ToTokens;
use rayon::vec;

use crate::{
    data::type_resolver::{ResolvedTypeData, TypeUsage},
    generate::{
        cs_members::CsField,
        cs_type_tag::CsTypeTag,
        metadata::CordlMetadata,
        type_extensions::{TypeDefinitionExtensions, TypeDefinitionIndexExtensions},
    },
};

/*
/// @brief Explicitly laid out type with union based offsets
union {


#pragma pack(push, tp, 1)
struct  {
/// @brief Padding field 0x0
 uint8_t  ___U0_padding[0x0];
/// @brief Field U0, offset: 0x0, size: 0x4, def value: None
 uint32_t  ___U0;
};
#pragma pack(pop, tp)
struct  {
/// @brief Padding field 0x0 for alignment
 uint8_t  ___U0_padding_forAlignment[0x0];
/// @brief Field U0, offset: 0x0, size: 0x4, def value: None
 uint32_t  ___U0_forAlignment;
};

#pragma pack(push, tp, 1)
struct  {
/// @brief Padding field 0x4
 uint8_t  ___U1_padding[0x4];
/// @brief Field U1, offset: 0x4, size: 0x4, def value: None
 uint32_t  ___U1;
};
#pragma pack(pop, tp)
struct  {
/// @brief Padding field 0x4 for alignment
 uint8_t  ___U1_padding_forAlignment[0x4];
/// @brief Field U1, offset: 0x4, size: 0x4, def value: None
 uint32_t  ___U1_forAlignment;
};
#pragma pack(push, tp, 1)
struct  {
/// @brief Padding field 0x8
 uint8_t  ___U2_padding[0x8];
/// @brief Field U2, offset: 0x8, size: 0x4, def value: None
 uint32_t  ___U2;
};
#pragma pack(pop, tp)
struct  {
/// @brief Padding field 0x8 for alignment
 uint8_t  ___U2_padding_forAlignment[0x8];
/// @brief Field U2, offset: 0x8, size: 0x4, def value: None
 uint32_t  ___U2_forAlignment;
};
#pragma pack(push, tp, 1)
struct  {
/// @brief Padding field 0x0
 uint8_t  ___ulo64LE_padding[0x0];
/// @brief Field ulo64LE, offset: 0x0, size: 0x8, def value: None
 uint64_t  ___ulo64LE;
};
#pragma pack(pop, tp)
struct  {
/// @brief Padding field 0x0 for alignment
 uint8_t  ___ulo64LE_padding_forAlignment[0x0];
/// @brief Field ulo64LE, offset: 0x0, size: 0x8, def value: None
 uint64_t  ___ulo64LE_forAlignment;
};
#pragma pack(push, tp, 1)
struct  {
/// @brief Padding field 0x8
 uint8_t  ___uhigh64LE_padding[0x8];
/// @brief Field uhigh64LE, offset: 0x8, size: 0x8, def value: None
 uint64_t  ___uhigh64LE;
};
#pragma pack(pop, tp)
struct  {
/// @brief Padding field 0x8 for alignment
 uint8_t  ___uhigh64LE_padding_forAlignment[0x8];
/// @brief Field uhigh64LE, offset: 0x8, size: 0x8, def value: None
 uint64_t  ___uhigh64LE_forAlignment;
};
};
*/

use super::{
    config::RustGenerationConfig,
    rust_members::{
        RustField, RustFunction, RustItem, RustParam, RustStruct, RustUnion, Visibility,
    },
    rust_name_resolver::RustNameResolver,
    rust_type::RustType,
};

pub(crate) fn handle_valuetype_fields(
    cpp_type: &mut RustType,
    fields: &[CsField],
    name_resolver: &RustNameResolver,
    config: &RustGenerationConfig,
) {
    let metadata = name_resolver.cordl_metadata;
    let tdi = cpp_type.self_tag.get_tdi();
    // Value types only need getter fixes for explicit layout types
    let t = tdi.get_type_definition(metadata.metadata);

    // if no fields, skip
    if t.field_count == 0 {
        return;
    }

    // instance fields for explicit layout value types are special
    if t.is_explicit_layout() {
        // TODO: Figure out layouts for explicit layout types
        // let backing_fields = fields
        //     .iter()
        //     .map(|f| make_rust_field(cpp_type, &f, name_resolver, config))
        //     // .map(|mut f| {
        //     //     f.name = fixup_backing_field(&f.cpp_name);
        //     //     f
        //     // })
        //     .collect_vec();

        // handle_instance_fields(cpp_type, &backing_fields, metadata, tdi);
    }
    let backing_fields = fields
        .iter()
        .map(|f| make_rust_field(cpp_type, f, name_resolver, config))
        .collect_vec();

    handle_instance_fields(cpp_type, &backing_fields, fields, metadata, tdi);
}

pub(crate) fn handle_referencetype_fields(
    cpp_type: &mut RustType,
    fields: &[CsField],
    name_resolver: &RustNameResolver,
    config: &RustGenerationConfig,
) {
    let metadata = name_resolver.cordl_metadata;
    let tdi = cpp_type.self_tag.get_tdi();
    let t = tdi.get_type_definition(metadata.metadata);

    if t.is_explicit_layout() {
        warn!(
            "Reference type with explicit layout: {}",
            cpp_type.rs_name_components.combine_all()
        );
    }

    // if no fields, skip
    if t.field_count == 0 {
        return;
    }

    let backing_fields = fields
        .iter()
        .filter(|f| f.instance && !f.is_const)
        .map(|f| make_rust_field(cpp_type, f, name_resolver, config))
        // .map(|mut f| {
        //     f.cpp_name = fixup_backing_field(&f.cpp_name);
        //     f
        // })
        .collect_vec();

    handle_instance_fields(cpp_type, &backing_fields, fields, metadata, tdi);
}

pub fn handle_static_fields(
    cpp_type: &mut RustType,
    fields: &[CsField],
    name_resolver: &RustNameResolver,
    config: &RustGenerationConfig,
) {
    let metadata = name_resolver.cordl_metadata;

    let tdi = cpp_type.self_tag.get_tdi();
    let t = tdi.get_type_definition(metadata.metadata);

    // if no fields, skip
    if t.field_count == 0 {
        return;
    }

    // we want only static fields
    // we ignore constants
    for field_info in fields.iter().filter(|f| !f.instance && !f.is_const) {
        let f_name = &field_info.name;

        let field_ty_cpp_name = name_resolver
            .resolve_name(cpp_type, &field_info.field_ty, TypeUsage::Field, true)
            .combine_all();

        // non const field
        // instance field access on ref types is special
        // ref type instance fields are specially named because the field getters are supposed to be used
        let f_cpp_name = config.name_rs(f_name);

        let klass_resolver = cpp_type.classof_name();

        let getter_call = format!(
            "return getStaticField<{field_ty_cpp_name}, \"{f_name}\", {klass_resolver}>();"
        );

        let setter_var_name = "value";
        let setter_call =
                format!("setStaticField<{field_ty_cpp_name}, \"{f_name}\", {klass_resolver}>(std::forward<{field_ty_cpp_name}>({setter_var_name}));");

        let getter_name = format!("getStaticF_{}", f_cpp_name);
        let setter_name = format!("setStaticF_{}", f_cpp_name);

        let get_return_type = field_ty_cpp_name.clone();

        let getter_decl = RustFunction {
            name: getter_name.clone(),
            is_ref: false,
            is_mut: false,
            is_self: false,

            return_type: Some(get_return_type),
            params: vec![],
            visibility: Some(Visibility::Public),
            body: Some(getter_call.to_string()),
        };

        let setter_decl = RustFunction {
            name: setter_name,

            is_ref: false,
            is_mut: false,
            is_self: false,

            return_type: None,
            params: vec![RustParam {
                name: setter_var_name.to_string(),
                param_type: field_ty_cpp_name,
            }],
            visibility: Some(Visibility::Public),
            body: Some(setter_call.to_string()),
        };

        // only push accessors if declaring ref type, or if static field
        cpp_type.methods.push(getter_decl);
        cpp_type.methods.push(setter_decl);
    }
}

pub(crate) fn handle_const_fields(
    cpp_type: &mut RustType,
    fields: &[CsField],
    name_resolver: &RustNameResolver,
    config: &RustGenerationConfig,
) {
    let metadata = name_resolver.cordl_metadata;

    // if no fields, skip
    if fields.is_empty() {
        return;
    }

    return;
    todo!();

    for field_info in fields.iter().filter(|f| f.is_const) {
        let cpp_field_template = make_rust_field(cpp_type, field_info, name_resolver, config);
        let f_resolved_type = &field_info.field_ty;
        let f_type = field_info.field_ty.get_type(metadata);
        let f_name = &field_info.name;
        let f_offset = field_info.offset.unwrap_or(u32::MAX);
        let f_size = field_info.size;

        let def_value = field_info.value.as_ref();

        let def_value = def_value.expect("Constant with no default value?");

        match f_resolved_type.data {
            ResolvedTypeData::Primitive(_) => {
                // primitive type
                let field_decl = RustField {
                    ..cpp_field_template
                };

                cpp_type.fields.push(field_decl);
            }
            _ => {
                // other type
                let field_decl = RustField {
                    ..cpp_field_template.clone()
                };
            }
        }
    }
}

fn handle_instance_fields(
    cpp_type: &mut RustType,
    fields: &[RustField],
    cs_fields: &[CsField],
    metadata: &CordlMetadata,
    tdi: TypeDefinitionIndex,
) {
    let t = tdi.get_type_definition(metadata.metadata);

    // if no fields, skip
    if t.field_count == 0 {
        return;
    }

    // let property_exists = |to_find: &str| cpp_type.fields.iter().any(|d| d.name == to_find);

    // explicit layout types are packed into single unions
    if t.is_explicit_layout() {
        // oh no! the fields are unionizing! don't tell elon musk!
        let last_field = cs_fields
            .iter()
            .filter(|t| t.offset.is_some())
            .max_by(|a, b| {
                let offset = a.offset.cmp(&b.offset);
                let size = a.size.cmp(&b.size);

                offset.then(size)
            });

        if let Some(last_field) = last_field {
            // make the type as big as it needs to be to match ABI
            let size = last_field.offset.unwrap() as usize + last_field.size;

            let size_field = RustField {
                name: "padding".to_owned(),
                field_type: (format!("[u8; 0x{size:x}]")).to_token_stream(),
                visibility: Visibility::Private,
                offset: 0,
            };

            cpp_type.fields.push(size_field);
        }

        // let u = pack_fields_into_single_union(fields);
        // cpp_type.fields.push(RustField {
        //     name: "explicit_layout".to_string(),
        //     field_type: RustItem::Union(u),
        //     visibility: Visibility::Private,
        //     offset: 0,
        // });
    } else {
        fields
            .iter()
            .cloned()
            .for_each(|member| cpp_type.fields.push(member));
    };
}

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

    todo!()
    // let fields = packed_structs
    //     .into_iter()
    //     .enumerate()
    //     .map(|(i, struc)| RustField {
    //         name: format!("struct{}", i),
    //         field_type: RustItem::Struct(struc),
    //         visibility: Visibility::Private,
    //         offset: 0,
    //     })
    //     .collect_vec();

    // RustUnion { fields }
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
        field_type: format!("[u8; 0x{padding:x}]").to_token_stream(),
        visibility: Visibility::Private,
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
        field_type: format!("[u8; 0x{padding:x}]").to_token_stream(),
        visibility: Visibility::Private,
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
        visibility: Visibility::Private,
        ..field.clone()
    };

    let packed_field = RustField {
        visibility: Visibility::Public,
        ..field
    };

    let packed_struct = RustStruct {
        fields: vec![packed_padding_field.clone(), packed_field.clone()],

        packing: Some(1),
    };

    let alignment_struct = RustStruct {
        fields: vec![(alignment_padding_field), (alignment_field)],

        packing: None,
    };

    (packed_struct, alignment_struct)
}

fn make_rust_field(
    cpp_type: &mut RustType,
    f: &CsField,
    name_resolver: &RustNameResolver<'_, '_>,
    config: &RustGenerationConfig,
) -> RustField {
    let field_type = name_resolver.resolve_name(cpp_type, &f.field_ty, TypeUsage::Field, true);

    assert!(f.instance && !f.is_const, "Static field not allowed!");

    RustField {
        name: config.name_rs(&f.name),
        field_type: field_type.to_combined_ident(),
        visibility: Visibility::Public,
        offset: f.offset.unwrap_or_default(),
    }
}
