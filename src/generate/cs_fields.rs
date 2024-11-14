use std::collections::HashMap;

use crate::generate::cs_members::CsNestedStruct;
use crate::generate::cs_members::CsNestedUnion;
use crate::generate::cs_type::CsType;

use brocolib::global_metadata::Il2CppFieldDefinition;
use brocolib::runtime_metadata::Il2CppType;
use itertools::Itertools;
use log::warn;

use brocolib::global_metadata::TypeDefinitionIndex;

use super::cs_context_collection::TypeContextCollection;
use super::cs_members::CsField;
use super::cs_members::CsFieldLayout;

use super::cs_members::CsMember;

use super::cs_type_tag::CsTypeTag;
use super::metadata::Metadata;
use super::type_extensions::Il2CppTypeEnumExtensions;
use super::type_extensions::TypeDefinitionExtensions;
use super::type_extensions::TypeExtentions;

#[derive(Clone, Debug)]
pub struct FieldInfo<'a> {
    pub cs_field: CsField,
    pub field: &'a Il2CppFieldDefinition,
    pub field_type: &'a Il2CppType,
    pub is_constant: bool,
    pub is_static: bool,
    pub is_pointer: bool,

    pub offset: Option<u32>,
    pub size: usize,
}

pub struct FieldInfoSet<'a> {
    fields: Vec<Vec<FieldInfo<'a>>>,
    size: u32,
    offset: u32,
}

impl<'a> FieldInfoSet<'a> {
    fn max(&self) -> u32 {
        self.size + self.offset
    }
}

pub(crate) fn handle_const_fields(
    cpp_type: &mut CsType,
    fields: &[FieldInfo],
    metadata: &Metadata,
    tdi: TypeDefinitionIndex,
) {
    let t = CsType::get_type_definition(metadata, tdi);

    // if no fields, skip
    if t.field_count == 0 {
        return;
    }

    let _declaring_cpp_template = if cpp_type
        .generic_template
        .as_ref()
        .is_some_and(|t| !t.names.is_empty())
    {
        cpp_type.generic_template.clone()
    } else {
        None
    };

    for field_info in fields.iter().filter(|f| f.is_constant) {
        let f_type = field_info.field_type;
        let f_name = field_info.field.name(metadata.metadata);
        let f_offset = field_info.offset.unwrap_or(u32::MAX);
        let f_size = field_info.size;

        let def_value = field_info.cs_field.value.as_ref();

        let def_value = def_value.expect("Constant with no default value?");

        match f_type.ty.is_primitive_builtin() {
            false => {
                // other type
                let field_decl = CsField {
                    instance: false,
                    readonly: f_type.is_constant(),
                    value: None,
                    const_expr: false,
                    brief_comment: Some(format!("Field {f_name} value: {def_value:#?}")),
                    ..field_info.cs_field.clone()
                };

                cpp_type
                    .members
                    .push(CsMember::FieldDecl(field_decl).into());
            }
            true => {
                // primitive type
                let field_decl = CsField {
                    instance: false,
                    const_expr: true,
                    readonly: f_type.is_constant(),

                    brief_comment: Some(format!(
                        "Field {f_name} offset 0x{f_offset:x} size 0x{f_size:x}"
                    )),
                    value: Some(def_value.clone()),
                    ..field_info.cs_field.clone()
                };

                cpp_type
                    .members
                    .push(CsMember::FieldDecl(field_decl).into());
            }
        }
    }
}

pub(crate) fn handle_instance_fields(
    cpp_type: &mut CsType,
    fields: &[FieldInfo],
    metadata: &Metadata,
    tdi: TypeDefinitionIndex,
) {
    let t = CsType::get_type_definition(metadata, tdi);

    // if no fields, skip
    if t.field_count == 0 {
        return;
    }

    let instance_field_decls = fields
        .iter()
        .filter(|f| f.offset.is_some() && !f.is_static && !f.is_constant)
        .cloned()
        .collect_vec();

    // explicit layout types are packed into single unions
    if t.is_explicit_layout() {
        // oh no! the fields are unionizing! don't tell elon musk!
        let u = pack_fields_into_single_union(instance_field_decls);
        cpp_type.members.push(CsMember::NestedUnion(u).into());
    } else {
        instance_field_decls
            .into_iter()
            .map(|member| CsMember::FieldDecl(member.cs_field))
            .for_each(|member| cpp_type.members.push(member.into()));
    };
}

pub(crate) fn handle_valuetype_fields(
    cpp_type: &mut CsType,
    fields: &[FieldInfo],
    metadata: &Metadata,
    tdi: TypeDefinitionIndex,
) {
    // Value types only need getter fixes for explicit layout types
    let t = CsType::get_type_definition(metadata, tdi);

    // if no fields, skip
    if t.field_count == 0 {
        return;
    }

    // instance fields for explicit layout value types are special
    if t.is_explicit_layout() {
        handle_instance_fields(cpp_type, fields, metadata, tdi);
    } else {
        handle_instance_fields(cpp_type, fields, metadata, tdi);
    }
}

pub(crate) fn handle_referencetype_fields(
    cpp_type: &mut CsType,
    fields: &[FieldInfo],
    metadata: &Metadata,
    tdi: TypeDefinitionIndex,
) {
    let t = CsType::get_type_definition(metadata, tdi);

    if t.is_explicit_layout() {
        warn!(
            "Reference type with explicit layout: {}",
            cpp_type.cs_name_components.combine_all()
        );
    }

    // if no fields, skip
    if t.field_count == 0 {
        return;
    }

    handle_instance_fields(cpp_type, &fields, metadata, tdi);
}

pub fn handle_static_fields(
    cpp_type: &mut CsType,
    fields: &[FieldInfo],
    metadata: &Metadata,
    tdi: TypeDefinitionIndex,
) {
    let t = CsType::get_type_definition(metadata, tdi);

    // if no fields, skip
    if t.field_count == 0 {
        return;
    }

    // we want only static fields
    // we ignore constants
    for field_info in fields.iter().filter(|f| f.is_static && !f.is_constant) {
        let f_type = field_info.field_type;
        let f_name = field_info.field.name(metadata.metadata);
        let _f_offset = field_info.offset.unwrap_or(u32::MAX);
        let _f_size = field_info.size;
        let _field_ty_cpp_name = &field_info.cs_field.field_ty;
        let f_tag = CsTypeTag::from_type_data(f_type.data, metadata.metadata);

        cpp_type.members.push(
            CsMember::FieldDecl(CsField {
                name: f_name.to_string(),
                field_ty: f_tag,
                instance: false,
                readonly: false,
                const_expr: true,
                offset: None,
                value: None,
                brief_comment: None,
            })
            .into(),
        );
    }
}

pub(crate) fn field_collision_check(instance_fields: &[FieldInfo]) -> bool {
    let mut next_offset = 0;
    return instance_fields
        .iter()
        .sorted_by(|a, b| a.offset.cmp(&b.offset))
        .any(|field| {
            let offset = field.offset.unwrap_or(u32::MAX);
            if offset < next_offset {
                true
            } else {
                next_offset = offset + field.size as u32;
                false
            }
        });
}

// inspired by what il2cpp does for explicitly laid out types
pub(crate) fn pack_fields_into_single_union(fields: Vec<FieldInfo>) -> CsNestedUnion {
    // get the min offset to use as a base for the packed structs
    let min_offset = fields.iter().map(|f| f.offset.unwrap()).min().unwrap_or(0);

    let packed_structs = fields
        .into_iter()
        .map(|field| {
            let layout = field_into_offset_structs(min_offset, field);

            layout
        })
        .collect_vec();

    let declarations = packed_structs
        .into_iter()
        .map(|s| CsMember::FieldLayout(s).into())
        .collect_vec();

    CsNestedUnion {
        brief_comment: Some("Explicitly laid out type with union based offsets".into()),
        declarations,
        offset: min_offset,
    }
}

pub(crate) fn field_into_offset_structs(_min_offset: u32, field: FieldInfo) -> CsFieldLayout {
    // il2cpp basically turns each field into 2 structs within a union:
    // 1 which is packed with size 1, and padded with offset to fit to the end
    // the other which has the same padding and layout, except this one is for alignment so it's just packed as the parent struct demands

    let Some(actual_offset) = &field.offset else {
        panic!("don't call field_into_offset_structs with non instance fields!")
    };

    let padding = actual_offset;

    CsFieldLayout {
        // #pragma pack(push, tp, 1)
        field: field.cs_field,
        padding: *padding,         // create field with size padding
        alignment: *actual_offset, // create field with size padding
    }
}

/// generates the fields for the value type or reference type\
/// handles unions
pub(crate) fn make_or_unionize_fields(instance_fields: &[FieldInfo]) -> Vec<CsMember> {
    // make all fields like usual
    if !field_collision_check(instance_fields) {
        return instance_fields
            .iter()
            .map(|d| CsMember::FieldDecl(d.cs_field.clone()))
            .collect_vec();
    }
    // we have a collision, investigate and handle

    let mut offset_map = HashMap::new();

    fn accumulated_size(fields: &[FieldInfo]) -> u32 {
        fields.iter().map(|f| f.size as u32).sum()
    }

    let mut current_max: u32 = 0;
    let mut current_offset: u32 = 0;

    // TODO: Field padding for exact offsets (explicit layouts?)

    // you can't sort instance fields on offset/size because it will throw off the unionization process
    instance_fields
        .iter()
        .sorted_by(|a, b| a.size.cmp(&b.size))
        .rev()
        .sorted_by(|a, b| a.offset.cmp(&b.offset))
        .for_each(|field| {
            let offset = field.offset.unwrap_or(u32::MAX);
            let size = field.size as u32;
            let max = offset + size;

            if max > current_max {
                current_offset = offset;
                current_max = max;
            }

            let current_set = offset_map
                .entry(current_offset)
                .or_insert_with(|| FieldInfoSet {
                    fields: vec![],
                    offset: current_offset,
                    size,
                });

            if current_max > current_set.max() {
                current_set.size = size
            }

            // if we have a last vector & the size of its fields + current_offset is smaller than current max add to that list
            if let Some(last) = current_set.fields.last_mut()
                && current_offset + accumulated_size(last) == offset
            {
                last.push(field.clone());
            } else {
                current_set.fields.push(vec![field.clone()]);
            }
        });

    offset_map
        .into_values()
        .map(|field_set| {
            // if we only have one list, just emit it as a set of fields
            if field_set.fields.len() == 1 {
                return field_set
                    .fields
                    .into_iter()
                    .flat_map(|v| v.into_iter())
                    .map(|d| CsMember::FieldDecl(d.cs_field))
                    .collect_vec();
            }
            // we had more than 1 list, so we have unions to emit
            let declarations = field_set
                .fields
                .into_iter()
                .map(|struct_contents| {
                    if struct_contents.len() == 1 {
                        // emit a struct with only 1 field as just a field
                        return struct_contents
                            .into_iter()
                            .map(|d| CsMember::FieldDecl(d.cs_field))
                            .collect_vec();
                    }
                    vec![
                        // if we have more than 1 field, emit a nested struct
                        CsMember::NestedStruct(CsNestedStruct {
                            is_enum: false,
                            is_class: false,
                            declarations: struct_contents
                                .into_iter()
                                .map(|d| CsMember::FieldDecl(d.cs_field).into())
                                .collect_vec(),
                            brief_comment: Some(format!(
                                "Anonymous struct offset 0x{:x}, size 0x{:x}",
                                field_set.offset, field_set.size
                            )),
                            packing: None,
                            name: "".into(),
                        }),
                    ]
                })
                .flat_map(|v| v.into_iter())
                .collect_vec();

            // wrap our set into a union
            vec![CsMember::NestedUnion(CsNestedUnion {
                brief_comment: Some(format!(
                    "Anonymous union offset 0x{:x}, size 0x{:x}",
                    field_set.offset, field_set.size
                )),
                declarations: declarations.into_iter().map(|d| d.into()).collect_vec(),
                offset: field_set.offset,
            })]
        })
        .flat_map(|v| v.into_iter())
        .collect_vec()
}
