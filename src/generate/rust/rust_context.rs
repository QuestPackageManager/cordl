use std::{
    collections::{HashMap, HashSet},
    fs::{create_dir_all, File},
    io::BufWriter,
    path::{self, PathBuf},
};

use color_eyre::eyre::ContextCompat;
use itertools::Itertools;
use log::{trace, warn};
use std::io::Write;

use crate::generate::{
    cs_type_tag::CsTypeTag, type_extensions::TypeDefinitionExtensions, writer::Writer,
};

use super::rust_type::RustType;

pub struct RustContext {
    // combined header
    pub fundamental_path: PathBuf,

    // Types to write, typedef
    pub typedef_types: HashMap<CsTypeTag, RustType>,

    // Name -> alias
    pub typealias_types: HashSet<(String, String)>,
}

impl RustContext {
    pub(crate) fn make(
        context_tag: crate::generate::cs_type_tag::CsTypeTag,
        context: &crate::generate::context::TypeContext,
        metadata: &crate::generate::metadata::CordlMetadata<'_>,
        config: &super::config::RustGenerationConfig,
    ) -> RustContext {
        let tdi = context_tag.get_tdi();
        let t = &metadata.metadata.global_metadata.type_definitions[tdi];

        let components = t.get_name_components(metadata.metadata);

        let ns = &components.namespace.as_deref().unwrap_or("GlobalNamespace");
        let name = &components.name;

        let path = PathBuf::from(config.namespace_path(ns));

        let path_name = match t.declaring_type_index != u32::MAX {
            true => {
                let name = config.name_rs(name);
                let base_name = components.declaring_types.unwrap_or_default().join("_");

                format!("{base_name}_{name}")
            }
            false => config.name_rs(name),
        };

        let fundamental_path = config
            .source_path
            .join(path.join(format!("{path_name}_mod")).with_extension("rs"));

        let mut x: RustContext = RustContext {
            fundamental_path,
            typedef_types: Default::default(),
            typealias_types: Default::default(),
        };

        for (tag, ty) in &context.typedef_types {
            let mut rs_ty = RustType::make_rust_type(*tag, ty, config);
            rs_ty.nested_fixup(&context_tag, ty, metadata, config);
            rs_ty.enum_fixup(ty);

            // TODO: Implement blacklist
            // let tdi = tag.get_tdi();
            // if metadata.blacklisted_types.contains(&tdi) {
            //     let result = match t.is_value_type() {
            //         true => format!(
            //             "{VALUE_WRAPPER_TYPE}<{:x}>",
            //             ty.size_info.as_ref().unwrap().instance_size
            //         ),
            //         false => IL2CPP_OBJECT_TYPE.to_string(),
            //     };

            //     if !t.is_value_type() {
            //         x.typealias_types.insert((
            //             rs_ty.cpp_namespace(),
            //             CppUsingAlias {
            //                 alias: rs_ty.name().to_string(),
            //                 result,
            //                 template: Default::default(),
            //             },
            //         ));
            //         continue;
            //     }
            // }

            x.typedef_types.insert(*tag, rs_ty);
        }

        x
    }

    /// Returns an immutable reference to the map of C++ types.
    pub fn get_types(&self) -> &HashMap<CsTypeTag, RustType> {
        &self.typedef_types
    }

    /// Returns a mutable reference to the map of C++ types.
    pub fn get_types_mut(&mut self) -> &mut HashMap<CsTypeTag, RustType> {
        &mut self.typedef_types
    }

    pub(crate) fn write(
        &self,
        config: &super::config::RustGenerationConfig,
    ) -> Result<(), color_eyre::eyre::Error> {
        let _base_path = &config.source_path;

        if !self
            .fundamental_path
            .parent()
            .context("parent is not a directory!")?
            .is_dir()
        {
            // Assume it's never a file
            create_dir_all(
                self.fundamental_path
                    .parent()
                    .context("Failed to create all directories!")?,
            )?;
        }

        trace!("Writing {:?}", self.fundamental_path.as_path());
        let mut typedef_writer = Writer {
            stream: BufWriter::new(File::create(self.fundamental_path.as_path())?),
            indent: 0,
            newline: true,
        };

        let modules: HashSet<&String> = self
            .typedef_types
            .values()
            .flat_map(|t| t.requirements.get_modules().iter())
            .sorted()
            .collect();

        for m in modules {
            writeln!(typedef_writer, "use {m};")?;
        }

        for t in self
            .typedef_types
            .values()
            .sorted_by(|a, b| a.rs_name().cmp(b.rs_name()))
        {
            if t.is_compiler_generated {
                warn!("Skipping compiler generated type: {}", t.name());
                continue;
            }

            t.write(&mut typedef_writer, config)?;
        }

        Ok(())
    }

    pub(crate) fn insert_rust_type(&mut self, new_rs_ty: RustType) {
        self.typedef_types.insert(new_rs_ty.self_tag, new_rs_ty);
    }

    pub fn get_module_path(&self, config: &super::config::RustGenerationConfig) -> String {
        let relative_path =
            pathdiff::diff_paths(&self.fundamental_path, &config.source_path).unwrap();

        let module_name = relative_path.file_stem().unwrap().to_string_lossy();

        let module_path = relative_path
            .parent()
            .unwrap()
            .to_string_lossy()
            .replace(path::MAIN_SEPARATOR, "::");

        format!("crate::{module_path}::{module_name}")
    }
}
