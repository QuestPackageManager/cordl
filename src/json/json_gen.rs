use std::{
    fs::{self, File},
    io::BufWriter,
    path::{Path, PathBuf},
};

use brocolib::global_metadata::{
    Il2CppFieldDefinition, Il2CppMethodDefinition, Il2CppParameterDefinition,
    Il2CppPropertyDefinition, Il2CppTypeDefinition, TypeDefinitionIndex,
};
use color_eyre::eyre::Result;
use itertools::Itertools;

use serde::{Deserialize, Serialize};

use crate::generate::{
    metadata::CordlMetadata,
    offsets,
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
    pub full_name: String,
    pub name: String,
    pub namespace: String,
    pub value_type: bool,
    pub fields: Vec<JsonField>,
    pub properties: Vec<JsonProperty>,
    pub methods: Vec<JsonMethod>,
    pub children: Vec<JsonType>,

    pub size: u32,
    pub packing: Option<u8>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
struct JsonField {
    pub name: String,
    pub ty_name: String,
    pub offset: Option<u32>,
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
    field_index: usize,
    _td: &Il2CppTypeDefinition,
    tdi: TypeDefinitionIndex,
    metadata: &CordlMetadata,
) -> JsonField {
    let ty = metadata
        .metadata
        .runtime_metadata
        .metadata_registration
        .types[field.type_index as usize];

    let offset = metadata
        .metadata
        .runtime_metadata
        .metadata_registration
        .field_offsets
        .as_ref()
        .and_then(|f| f[tdi.index() as usize].get(field_index))
        .copied();

    JsonField {
        name: field.name(metadata.metadata).to_string(),
        ty_name: ty.full_name(metadata.metadata),
        offset,
    }
}
fn make_property(
    property: &Il2CppPropertyDefinition,
    td: &Il2CppTypeDefinition,
    _tdi: TypeDefinitionIndex,
    metadata: &CordlMetadata,
) -> JsonProperty {
    let p_setter = (property.set != u32::MAX).then(|| property.set_method(td, metadata.metadata));
    let p_getter = (property.get != u32::MAX).then(|| property.get_method(td, metadata.metadata));

    let p_type_index = match p_getter {
        Some(g) => g.return_type as usize,
        None => p_setter.unwrap().parameters(metadata.metadata)[0].type_index as usize,
    };

    let p_type = metadata
        .metadata
        .runtime_metadata
        .metadata_registration
        .types
        .get(p_type_index)
        .unwrap();

    JsonProperty {
        name: property.name(metadata.metadata).to_string(),
        ty_name: p_type.full_name(metadata.metadata),
        has_getter: property.get != u32::MAX,
        has_setter: property.set != u32::MAX,
    }
}
fn make_param(
    param: &Il2CppParameterDefinition,
    _td: &Il2CppTypeDefinition,
    _tdi: TypeDefinitionIndex,
    metadata: &CordlMetadata,
) -> JsonParam {
    let param_type = metadata
        .metadata
        .runtime_metadata
        .metadata_registration
        .types[param.type_index as usize];

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
        name: param.name(metadata.metadata).to_string(),
        ty: param_type.full_name(metadata.metadata),
        ref_mode,
    }
}
fn make_method(
    method: &Il2CppMethodDefinition,
    td: &Il2CppTypeDefinition,
    tdi: TypeDefinitionIndex,
    metadata: &CordlMetadata,
) -> JsonMethod {
    let ret_ty = metadata
        .metadata
        .runtime_metadata
        .metadata_registration
        .types[method.return_type as usize];

    let params = method
        .parameters(metadata.metadata)
        .iter()
        .map(|p| make_param(p, td, tdi, metadata))
        .collect_vec();

    JsonMethod {
        name: method.name(metadata.metadata).to_string(),
        parameters: params,
        ret: ret_ty.full_name(metadata.metadata),
    }
}

fn make_type(td: &Il2CppTypeDefinition, tdi: TypeDefinitionIndex, metadata: &CordlMetadata) -> JsonType {
    let fields = td
        .fields(metadata.metadata)
        .iter()
        .enumerate()
        .map(|(i, f)| make_field(f, i, td, tdi, metadata))
        .collect_vec();
    let properties = td
        .properties(metadata.metadata)
        .iter()
        .map(|f| make_property(f, td, tdi, metadata))
        .sorted_by(|a, b| a.name.cmp(&b.name))
        .collect_vec();
    let methods = td
        .methods(metadata.metadata)
        .iter()
        .map(|f| make_method(f, td, tdi, metadata))
        .sorted_by(|a, b| a.name.cmp(&b.name))
        .collect_vec();

    let children = td
        .nested_types(metadata.metadata)
        .iter()
        .map(|nested_tdi: &TypeDefinitionIndex| {
            let nested_td = &metadata.metadata.global_metadata.type_definitions[*nested_tdi];

            make_type(nested_td, tdi, metadata)
        })
        .sorted_by(|a, b| a.name.cmp(&b.name))
        .collect_vec();

    let namespace = td.namespace(metadata.metadata).to_string();
    let name = td.name(metadata.metadata).to_string();

    let (size, packing) = offsets::get_size_and_packing(td, tdi, None, metadata);

    JsonType {
        full_name: td.full_name(metadata.metadata, true),
        namespace,
        name,
        value_type: td.is_value_type(),
        fields,
        properties,
        methods,
        children,
        packing,
        size,
    }
}

///
/// Essentially check if the type is compiler generated or
/// not useful to emit
///
pub fn is_real_declaring_type(td: &Il2CppTypeDefinition, metadata: &CordlMetadata) -> bool {
    let condition1 = !td.name(metadata.metadata).contains("<>c__")
        && !td.name(metadata.metadata).contains(">d__");
    let condition2 = !td
        .full_name(metadata.metadata, true)
        .contains("<PrivateImplementationDetails>");
    let condition3 = td.parent_index != u32::MAX
        || td.is_interface()
        || td.full_name(metadata.metadata, true) == "System.Object";
    let condition4 = !td.namespace(metadata.metadata).contains("$$struct");

    // -1 if no declaring type, meaning root
    let is_declaring_type = td.declaring_type_index == u32::MAX;

    let is_compiler_generated = td.is_compiler_generated();

    !is_compiler_generated
        && is_declaring_type
        && condition1
        && condition2
        && condition3
        && condition4
}

pub fn make_json(metadata: &CordlMetadata, file: &Path) -> Result<()> {
    // we could use a map here but sorting
    // wouldn't be guaranteed
    // we want sorting so diffs are more readable
    let json_objects = metadata
        .metadata
        .global_metadata
        .type_definitions
        .as_vec()
        .iter()
        .enumerate()
        .map(|(i, t)| (TypeDefinitionIndex::new(i as u32), t))
        // skip compiler generated types
        .filter(|(_, t)| is_real_declaring_type(t, metadata))
        .map(|(tdi, td)| make_type(td, tdi, metadata))
        .sorted_by(|a, b| a.full_name.cmp(&b.full_name))
        .collect_vec();

    let file = File::create(file)?;
    let mut buf_writer = BufWriter::new(file);

    serde_json::to_writer_pretty(&mut buf_writer, &json_objects)?;

    Ok(())
}

pub fn make_json_folder(metadata: &CordlMetadata, folder: &Path) -> Result<()> {
    // we could use a map here but sorting
    // wouldn't be guaranteed
    // we want sorting so diffs are more readable
    metadata
        .metadata
        .global_metadata
        .type_definitions
        .as_vec()
        .iter()
        .enumerate()
        .map(|(i, t)| (TypeDefinitionIndex::new(i as u32), t))
        // skip compiler generated types
        .filter(|(_, t)| is_real_declaring_type(t, metadata))
        .map(|(tdi, td)| make_type(td, tdi, metadata))
        .sorted_by(|a, b| a.full_name.cmp(&b.full_name))
        .try_for_each(|t| -> Result<()> {
            let mut namespace = t.namespace.clone();
            let name = t.name.clone();

            if namespace.is_empty() {
                namespace = "GlobalNamespace".to_string();
            }

            let file: PathBuf = folder.join(namespace).join(name).with_extension("json");

            fs::create_dir_all(file.parent().unwrap())?;

            let file = File::create(file)?;
            let mut buf_writer = BufWriter::new(file);

            serde_json::to_writer_pretty(&mut buf_writer, &t)?;

            Ok(())
        })?;

    Ok(())
}
