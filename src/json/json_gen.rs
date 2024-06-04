use std::{fs::File, io::BufWriter, path::PathBuf};

use brocolib::{
    global_metadata::{
        Il2CppFieldDefinition, Il2CppMethodDefinition, Il2CppParameterDefinition,
        Il2CppPropertyDefinition, Il2CppTypeDefinition, TypeDefinitionIndex,
    },
    Metadata,
};
use color_eyre::eyre::Result;
use itertools::Itertools;
use serde::{Deserialize, Serialize};

use crate::generate::{
    config::GenerationConfig,
    type_extensions::{ParameterDefinitionExtensions, TypeDefinitionExtensions, TypeExtentions},
};

#[derive(Debug, Clone, Serialize, Deserialize)]
enum JsonFieldRef {
    In,
    Out,
    Ref,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
struct JsonType {
    pub name: String,
    pub value_type: bool,
    pub fields: Vec<JsonField>,
    pub properties: Vec<JsonProperty>,
    pub methods: Vec<JsonMethod>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
struct JsonField {
    pub name: String,
    pub ty_name: String,
}
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
struct JsonProperty {
    pub name: String,
    pub ty_name: String,
    pub has_getter: bool,
    pub has_setter: bool,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
struct JsonMethod {
    pub name: String,
    pub ret: String,
    pub parameters: Vec<JsonParam>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
struct JsonParam {
    pub name: String,
    pub ty: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub ref_mode: Option<JsonFieldRef>,
}

fn make_field(
    field: &Il2CppFieldDefinition,
    td: &Il2CppTypeDefinition,
    tdi: TypeDefinitionIndex,
    metadata: &Metadata,
) -> JsonField {
    let ty = metadata.runtime_metadata.metadata_registration.types[field.type_index as usize];

    JsonField {
        name: field.name(metadata).to_string(),
        ty_name: ty.full_name(metadata),
    }
}
fn make_property(
    property: &Il2CppPropertyDefinition,
    td: &Il2CppTypeDefinition,
    tdi: TypeDefinitionIndex,
    metadata: &Metadata,
) -> JsonProperty {
    let p_setter = (property.set != u32::MAX).then(|| property.set_method(td, metadata));
    let p_getter = (property.get != u32::MAX).then(|| property.get_method(td, metadata));

    let p_type_index = match p_getter {
        Some(g) => g.return_type as usize,
        None => p_setter.unwrap().parameters(metadata)[0].type_index as usize,
    };

    let p_type = metadata
        .runtime_metadata
        .metadata_registration
        .types
        .get(p_type_index)
        .unwrap();

    JsonProperty {
        name: property.name(metadata).to_string(),
        ty_name: p_type.full_name(metadata),
        has_getter: property.get != u32::MAX,
        has_setter: property.set != u32::MAX,
    }
}
fn make_param(
    param: &Il2CppParameterDefinition,
    td: &Il2CppTypeDefinition,
    tdi: TypeDefinitionIndex,
    metadata: &Metadata,
) -> JsonParam {
    let param_type =
        metadata.runtime_metadata.metadata_registration.types[param.type_index as usize];

    let ref_mode = if param_type.is_param_in() {
        Some(JsonFieldRef::In)
    } else if param_type.is_param_out() {
        Some(JsonFieldRef::Out)
    } else if param_type.is_byref() {
        Some(JsonFieldRef::Ref)
    } else {
        None
    };

    JsonParam {
        name: param.name(metadata).to_string(),
        ty: param_type.full_name(metadata),
        ref_mode,
    }
}
fn make_method(
    method: &Il2CppMethodDefinition,
    td: &Il2CppTypeDefinition,
    tdi: TypeDefinitionIndex,
    metadata: &Metadata,
) -> JsonMethod {
    let ret_ty = metadata.runtime_metadata.metadata_registration.types[method.return_type as usize];

    let params = method
        .parameters(metadata)
        .iter()
        .map(|p| make_param(p, td, tdi, metadata))
        .collect_vec();

    JsonMethod {
        name: method.name(metadata).to_string(),
        parameters: params,
        ret: ret_ty.full_name(metadata),
    }
}

fn make_type(td: &Il2CppTypeDefinition, tdi: TypeDefinitionIndex, metadata: &Metadata) -> JsonType {
    let fields = td
        .fields(metadata)
        .iter()
        .map(|f| make_field(f, td, tdi, metadata))
        .collect_vec();
    let properties = td
        .properties(metadata)
        .iter()
        .map(|f| make_property(f, td, tdi, metadata))
        .collect_vec();
    let methods = td
        .methods(metadata)
        .iter()
        .map(|f| make_method(f, td, tdi, metadata))
        .sorted_by(|a, b| a.name.cmp(&b.name))
        .collect_vec();

    JsonType {
        name: td.full_name(metadata, true),
        value_type: td.is_value_type(),
        fields,
        properties,
        methods,
    }
}

pub fn make_json(metadata: &Metadata, _config: &GenerationConfig, file: PathBuf) -> Result<()> {
    let json_objects = metadata
        .global_metadata
        .type_definitions
        .as_vec()
        .iter()
        .enumerate()
        .map(|(i, t)| (TypeDefinitionIndex::new(i as u32), t))
        .filter(|(_, t)| !t.name(metadata).contains("<>c__"))
        .map(|(tdi, td)| make_type(td, tdi, metadata))
        .sorted_by(|a, b| a.name.cmp(&b.name))
        .collect_vec();

    let file = File::open(file)?;
    let mut buf_writer = BufWriter::new(file);

    serde_json::to_writer_pretty(&mut buf_writer, &json_objects)?;

    Ok(())
}
