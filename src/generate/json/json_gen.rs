use std::collections::HashMap;

use itertools::Itertools;

use serde::{Deserialize, Serialize};

use crate::generate::{
    cs_attributes::CsAttribute,
    cs_context_collection::TypeContextCollection,
    cs_members::{CsField, CsGenericContainer, CsMethod, CsParam, CsParamFlags, CsProperty, CsValue},
    cs_type::CsType,
    cs_type_tag::CsTypeTag,
    metadata::CordlMetadata,
};

use super::{
    json_data::{JsonAttribute, JsonGenericConstraint, JsonNamedArgument, JsonResolvedTypeData, JsonTypeTag, JsonValue},
    json_name_resolver::JsonNameResolver,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonTable {
    /// associated with types by their index in types_table
    pub types: HashMap<usize, JsonType>,
    pub types_table: Vec<JsonTypeTag>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum JsonFieldRef {
    In,
    Out,
    Ref,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonType {
    pub full_name: String,
    pub name: String,
    pub namespace: String,
    pub value_type: bool,
    pub fields: Vec<JsonField>,
    pub properties: Vec<JsonProperty>,
    pub methods: Vec<JsonMethod>,
    pub children: Vec<JsonType>,
    pub tag: JsonTypeTag,
    pub parent: Option<JsonResolvedTypeData>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub generic_container: Option<JsonGenericContainer>,

    /// Generic instatiation types if this is a generic instance type
    #[serde(skip_serializing_if = "Option::is_none")]
    pub generic_instatiation: Option<Vec<JsonResolvedTypeData>>,

    pub size: u32,
    pub packing: Option<u8>,

    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub attributes: Vec<JsonAttribute>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonField {
    pub name: String,
    pub ty_name: String,
    pub ty_tag: JsonResolvedTypeData,
    pub instance: bool,
    pub is_const: bool,
    pub readonly: bool,
    pub offset: Option<u32>,

    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub attributes: Vec<JsonAttribute>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonProperty {
    pub name: String,
    pub ty_name: String,
    pub ty_tag: JsonResolvedTypeData,
    pub instance: bool,
    pub indexable: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub getter: Option<(u32, String)>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub setter: Option<(u32, String)>,

    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub attributes: Vec<JsonAttribute>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonGenericArgument {
    pub constraints: Vec<JsonGenericConstraint>,
    pub index: u16, // generic argument index
    pub name: String,
}

type JsonGenericContainer = Vec<JsonGenericArgument>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonMethod {
    pub name: String,
    pub ret: String,
    pub ret_ty_tag: JsonResolvedTypeData,
    pub parameters: Vec<JsonParam>,
    pub instance: bool,
    pub method_info: JsonMethodInfo,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub generic_container: Option<JsonGenericContainer>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub generic_instatiation: Option<Vec<JsonResolvedTypeData>>,

    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub attributes: Vec<JsonAttribute>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonMethodInfo {
    pub estimated_size: Option<usize>,
    pub addrs: Option<u64>,
    pub slot: Option<u16>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonParam {
    pub name: String,
    pub ty: String,
    pub ty_tag: JsonResolvedTypeData,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub ref_mode: Option<JsonFieldRef>,

    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub attributes: Vec<JsonAttribute>,
}

/// Resolves the full C# name of a custom attribute's type, e.g. `System.ObsoleteAttribute` -
/// looked up directly by [`TypeDefinitionIndex`](brocolib::global_metadata::TypeDefinitionIndex)
/// rather than through [`JsonNameResolver::resolve_name`], since a [`CsTypeTag`] on its own
/// (unlike a [`crate::data::type_resolver::ResolvedType`]) carries no generic/pointer/array shape
/// to resolve.
fn attribute_type_name(tag: CsTypeTag, metadata: &CordlMetadata) -> String {
    metadata.metadata.global_metadata.type_definitions[tag.get_tdi()]
        .full_name(metadata.metadata, true)
}

fn make_value(value: &CsValue, metadata: &CordlMetadata) -> JsonValue {
    match value {
        CsValue::String(v) => JsonValue::String(v.clone()),
        CsValue::Char(v) => JsonValue::Char(v.clone()),
        CsValue::Bool(v) => JsonValue::Bool(*v),
        CsValue::U8(v) => JsonValue::U8(*v),
        CsValue::U16(v) => JsonValue::U16(*v),
        CsValue::U32(v) => JsonValue::U32(*v),
        CsValue::U64(v) => JsonValue::U64(*v),
        CsValue::I8(v) => JsonValue::I8(*v),
        CsValue::I16(v) => JsonValue::I16(*v),
        CsValue::I32(v) => JsonValue::I32(*v),
        CsValue::I64(v) => JsonValue::I64(*v),
        CsValue::F32(v) => JsonValue::F32(*v),
        CsValue::F64(v) => JsonValue::F64(*v),
        CsValue::Null => JsonValue::Null,
        CsValue::Array(items) => {
            JsonValue::Array(items.iter().map(|v| make_value(v, metadata)).collect_vec())
        }
        CsValue::Type(tag) => JsonValue::Type {
            tag: (*tag).into(),
            name: attribute_type_name(*tag, metadata),
        },
        CsValue::Enum(tag, value) => JsonValue::Enum {
            tag: (*tag).into(),
            name: attribute_type_name(*tag, metadata),
            value: Box::new(make_value(value, metadata)),
        },
    }
}

fn make_attribute(attr: &CsAttribute, metadata: &CordlMetadata) -> JsonAttribute {
    JsonAttribute {
        attribute_type: attr.attribute_type.into(),
        attribute_type_name: attribute_type_name(attr.attribute_type, metadata),
        arguments: attr
            .arguments
            .iter()
            .map(|v| make_value(v, metadata))
            .collect_vec(),
        named_arguments: attr
            .named_arguments
            .iter()
            .map(|a| JsonNamedArgument {
                name: a.name.clone(),
                value: make_value(&a.value, metadata),
            })
            .collect_vec(),
    }
}

fn make_attributes(attrs: &[CsAttribute], metadata: &CordlMetadata) -> Vec<JsonAttribute> {
    attrs.iter().map(|a| make_attribute(a, metadata)).collect_vec()
}

fn make_field(field: &CsField, name_resolver: &JsonNameResolver) -> JsonField {
    let ty: JsonResolvedTypeData = field.field_ty.clone().into();
    let ty_name = name_resolver.resolve_name(&field.field_ty).combine_all();
    let offset = field.offset;

    JsonField {
        name: field.name.to_string(),
        ty_name,
        offset,
        ty_tag: ty,
        instance: field.instance,
        is_const: field.is_const,
        readonly: field.readonly,
        attributes: make_attributes(&field.attributes, name_resolver.cordl_metadata),
    }
}
fn make_property(property: &CsProperty, name_resolver: &JsonNameResolver) -> JsonProperty {
    let p_setter = property
        .setter
        .as_ref()
        .map(|(i, s)| (i.index(), s.to_string()));
    let p_getter = property
        .getter
        .as_ref()
        .map(|(i, s)| (i.index(), s.to_string()));

    let p_type: JsonResolvedTypeData = property.prop_ty.clone().into();
    let ty_name = name_resolver.resolve_name(&property.prop_ty).combine_all();

    JsonProperty {
        name: property.name.to_string(),
        ty_tag: p_type,
        ty_name,
        instance: property.instance,
        indexable: property.indexable,
        setter: p_setter,
        getter: p_getter,
        attributes: make_attributes(&property.attributes, name_resolver.cordl_metadata),
    }
}
fn make_param(param: &CsParam, name_resolver: &JsonNameResolver) -> JsonParam {
    let param_type: JsonResolvedTypeData = param.il2cpp_ty.clone().into();
    let ty_name = name_resolver.resolve_name(&param.il2cpp_ty).combine_all();

    let ref_mode = if param.modifiers.contains(CsParamFlags::IN) {
        Some(JsonFieldRef::In)
    } else if param.modifiers.contains(CsParamFlags::OUT) {
        Some(JsonFieldRef::Out)
    } else if param.modifiers.contains(CsParamFlags::REF) {
        Some(JsonFieldRef::Ref)
    } else {
        None
    };

    JsonParam {
        name: param.name.to_string(),
        ty: ty_name,
        ty_tag: param_type,
        ref_mode,
        attributes: make_attributes(&param.attributes, name_resolver.cordl_metadata),
    }
}

fn make_generic(generic_container: &CsGenericContainer) -> JsonGenericContainer {
    generic_container
        .args
        .iter()
        .map(|arg| JsonGenericArgument {
            name: arg.name.to_string(),
            index: arg.index,
            constraints: arg
                .constraints
                .iter()
                .cloned()
                .map(|c| c.into())
                .collect_vec(),
        })
        .collect_vec()
}

fn make_method(method: &CsMethod, name_resolver: &JsonNameResolver) -> JsonMethod {
    let ret_ty_name = name_resolver
        .resolve_name(&method.return_type)
        .combine_all();
    let ret_ty: JsonResolvedTypeData = method.return_type.clone().into();

    let params = method
        .parameters
        .iter()
        .map(|p| make_param(p, name_resolver))
        .collect_vec();

    let json_method_info = JsonMethodInfo {
        addrs: method.method_data.addrs,
        estimated_size: method.method_data.estimated_size,
        slot: method.method_data.slot,
    };

    let generic_instatiation = method
        .generic_instatiation
        .as_ref()
        .map(|inst_types| inst_types.iter().map(|ty| ty.clone().into()).collect_vec());

    JsonMethod {
        name: method.name.to_string(),
        parameters: params,
        instance: method.instance,
        ret: ret_ty_name,
        ret_ty_tag: ret_ty,
        method_info: json_method_info,
        generic_container: method.generic_container.as_ref().map(make_generic),
        generic_instatiation,
        attributes: make_attributes(&method.attributes, name_resolver.cordl_metadata),
    }
}

pub fn make_type(
    td: &CsType,
    metadata: &CordlMetadata,
    collection: &TypeContextCollection,
) -> JsonType {
    let name_resolver = JsonNameResolver {
        cordl_metadata: metadata,
        collection,
    };

    let parent: Option<JsonResolvedTypeData> = td.parent.clone().map(|p| p.into());

    let fields = td
        .fields
        .iter()
        .map(|f| make_field(f, &name_resolver))
        .collect_vec();
    let properties = td
        .properties
        .iter()
        .map(|f| make_property(f, &name_resolver))
        .sorted_by(|a, b| a.name.cmp(&b.name))
        .collect_vec();
    let methods = td
        .methods
        .iter()
        .map(|f| make_method(f, &name_resolver))
        .sorted_by(|a, b| a.name.cmp(&b.name))
        .collect_vec();

    let children = td
        .nested_types
        .iter()
        .map(|nested_tag| {
            let nested_td = collection.get_cs_type(*nested_tag).unwrap();

            make_type(nested_td, metadata, collection)
        })
        .sorted_by(|a, b| a.name.cmp(&b.name))
        .collect_vec();

    let namespace = td.namespace().to_string();
    let name = td.name().to_string();

    let size = td.size_info.as_ref().unwrap().instance_size;
    let packing = td.packing;

    let generic_instatiation = td
        .generic_instantiations_args_types
        .as_ref()
        .map(|inst_types| inst_types.iter().map(|ty| ty.clone().into()).collect_vec());

    JsonType {
        full_name: td.cs_name_components.combine_all(),
        namespace,
        name,
        value_type: td.is_value_type,
        fields,
        properties,
        methods,
        children,
        generic_container: td.generic_container.as_ref().map(make_generic),
        packing,
        size,
        tag: td.self_tag.into(),
        parent,
        generic_instatiation,
        attributes: make_attributes(&td.attributes, metadata),
    }
}
