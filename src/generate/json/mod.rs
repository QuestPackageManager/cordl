use std::{
    collections::HashMap,
    fs::{self, File},
    io::BufWriter,
    path::{Path, PathBuf},
};

use itertools::Itertools;
use json_gen::{JsonTable, JsonType, make_type};

use super::{
    cs_context_collection::TypeContextCollection,
    cs_type::CsType,
    metadata::CordlMetadata,
    type_extensions::{TypeDefinitionExtensions, TypeDefinitionIndexExtensions},
};

mod json_data;
mod json_gen;
mod json_name_resolver;

type Result<T> = std::result::Result<T, color_eyre::eyre::Report>;

pub fn make_json(
    metadata: &CordlMetadata,
    collection: &TypeContextCollection,
    file: &Path,
    format: bool,
) -> Result<()> {
    // we could use a map here but sorting
    // wouldn't be guaranteed
    // we want sorting so diffs are more readable
    let json_objects: HashMap<usize, JsonType> = collection
        .get()
        .values()
        .flat_map(|c| c.get_types().values())
        // skip compiler generated types
        .filter(|t| is_real_declaring_type(t, metadata))
        .enumerate()
        .map(|(i, td)| (i, make_type(td, metadata, collection)))
        .sorted_by(|a, b| a.1.full_name.cmp(&b.1.full_name))
        .collect();

    let table = JsonTable {
        types_table: json_objects
            .iter()
            .sorted_by(|a, b| a.0.cmp(b.0))
            .map(|t| t.1.tag.clone())
            .collect(),
        types: json_objects,
    };

    let file = File::create(file)?;
    let mut buf_writer = BufWriter::new(file);

    match format {
        true => serde_json::to_writer_pretty(&mut buf_writer, &table)?,
        false => serde_json::to_writer(&mut buf_writer, &table)?,
    };

    Ok(())
}

pub fn make_json_folder(
    metadata: &CordlMetadata,
    collection: &TypeContextCollection,
    folder: &Path,
) -> Result<()> {
    // we could use a map here but sorting
    // wouldn't be guaranteed
    // we want sorting so diffs are more readable
    collection
        .get()
        .values()
        .flat_map(|c| c.get_types().values())
        // skip compiler generated types
        .filter(|t| is_real_declaring_type(t, metadata))
        .map(|td| make_type(td, metadata, collection))
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

///
/// Essentially check if the type is compiler generated or
/// not useful to emit
///
pub fn is_real_declaring_type(ty: &CsType, metadata: &CordlMetadata) -> bool {
    let condition1 = !ty.name().contains("<>c__") && !ty.name().contains(">d__");
    let condition2 = !ty
        .cs_name_components
        .combine_all()
        .contains("<PrivateImplementationDetails>");
    let condition3 = ty.parent.is_some()
        || ty.is_interface
        || ty.cs_name_components.combine_all() == "System.Object";
    let condition4 = !ty.namespace().contains("$$struct");

    // -1 if no declaring type, meaning root
    let is_declaring_type = ty.declaring_ty.is_none();

    let tdi = ty.self_tag.get_tdi();
    let td = tdi.get_type_definition(metadata.metadata);

    let is_compiler_generated = td.is_compiler_generated(metadata.metadata);

    !is_compiler_generated
        && is_declaring_type
        && condition1
        && condition2
        && condition3
        && condition4
}
