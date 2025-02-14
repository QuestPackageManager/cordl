use std::collections::HashMap;

use itertools::Itertools;

use serde::{Deserialize, Serialize};

use crate::generate::{
    cs_context_collection::TypeContextCollection,
    cs_members::{CsField, CsMethod, CsParam, CsParamFlags, CsProperty, CsGenericTemplateType, CsGenericTemplate},
    cs_type::CsType,
    metadata::CordlMetadata,
    type_extensions::TypeDefinitionExtensions,
};

use super::{
    json_data::{JsonResolvedTypeData, JsonTypeTag},
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
    pub template: Option<JsonTemplate>,

    pub size: u32,
    pub packing: Option<u8>,
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
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonProperty {
    pub name: String,
    pub ty_name: String,
    pub ty_tag: JsonResolvedTypeData,
    pub instance: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub getter: Option<(u32, String)>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub setter: Option<(u32, String)>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum JsonGenericArgumentType {
    AnyType,
    ReferenceType
}

type JsonTemplate = Vec<(JsonGenericArgumentType, String)>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonMethod {
    pub name: String,
    pub ret: String,
    pub ret_ty_tag: JsonResolvedTypeData,
    pub parameters: Vec<JsonParam>,
    pub instance: bool,
    pub method_info: JsonMethodInfo,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub template: Option<JsonTemplate>
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
        readonly: field.readonly
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
        setter: p_setter,
        getter: p_getter,
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
    }
}

fn make_template(template: &CsGenericTemplate) -> JsonTemplate {
    // note: I'm not good at rust so there may be a better way to do this - zip
    // (feel free to remove this comment if this is fine)
    return template.names
        .iter()
        .map(|p| (match(p.0) {
            CsGenericTemplateType::AnyType => JsonGenericArgumentType::AnyType,
            CsGenericTemplateType::ReferenceType => JsonGenericArgumentType::ReferenceType
        }, p.1.clone())).collect_vec();
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

    JsonMethod {
        name: method.name.to_string(),
        parameters: params,
        instance: method.instance,
        ret: ret_ty_name,
        ret_ty_tag: ret_ty,
        method_info: json_method_info,
        template: match&(method.template) {
            Some(t) => Some(make_template(t)),
            None => None
        }
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

    JsonType {
        full_name: td.cs_name_components.combine_all(),
        namespace,
        name,
        value_type: td.is_value_type,
        fields,
        properties,
        methods,
        children,
        template: match&(td.generic_template) {
            Some(t) => Some(make_template(t)),
            None => None
        },
        packing,
        size,
        tag: td.self_tag.into(),
        parent,
    }
}
