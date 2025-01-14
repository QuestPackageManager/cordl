use std::{
    collections::{HashMap, HashSet},
    ffi::OsStr,
    fs::File,
    io::{BufWriter, Write},
};

use rayon::prelude::*;

use itertools::Itertools;
use log::{info, trace};
use rayon::iter::ParallelIterator;
use walkdir::WalkDir;

use crate::generate::{
    cs_context_collection::TypeContextCollection, cs_type::CsType, cs_type_tag::CsTypeTag,
    metadata::CordlMetadata,
};

use super::{
    config::RustGenerationConfig, rust_context::RustContext,
    rust_name_resolver::RustNameResolver, rust_type::RustType,
};

#[derive(Default)]
pub struct RustContextCollection {
    // Should always be a TypeDefinitionIndex
    all_contexts: HashMap<CsTypeTag, RustContext>,
    pub alias_context: HashMap<CsTypeTag, CsTypeTag>,
    filled_types: HashSet<CsTypeTag>,
    filling_types: HashSet<CsTypeTag>,
    borrowing_types: HashSet<CsTypeTag>,
}

impl RustContextCollection {
    pub fn from_cs_collection(
        collection: TypeContextCollection,
        metadata: &CordlMetadata,
        config: &RustGenerationConfig,
    ) -> RustContextCollection {
        let mut cpp_collection = RustContextCollection::default();

        info!("Making CppContextCollection from TypeContextCollection");
        for (tag, context) in collection.get() {
            cpp_collection
                .all_contexts
                .insert(*tag, RustContext::make(*tag, context, metadata, config));
        }
        cpp_collection.alias_context = collection.alias_context;

        info!("Filling typedefs in CppContextCollection");
        for (_, context) in collection.all_contexts {
            for (tag, cs_type) in context.typedef_types {
                cpp_collection.fill(tag, cs_type, metadata, config);
            }
        }

        cpp_collection
    }

    fn do_fill_rust_type(
        &mut self,
        rs_type: &mut RustType,
        cs_type: CsType,
        metadata: &CordlMetadata,
        config: &RustGenerationConfig,
    ) {
        let tag = rs_type.self_tag;

        if self.filled_types.contains(&tag) {
            return;
        }
        if self.filling_types.contains(&tag) {
            panic!("Currently filling type {tag:?}, cannot fill")
        }

        // Move ownership to local
        self.filling_types.insert(tag);

        let name_resolver = RustNameResolver {
            cordl_metadata: metadata,
            collection: self,
            config,
        };

        rs_type.fill(cs_type, &name_resolver, config);

        self.filled_types.insert(tag);
        self.filling_types.remove(&tag.clone());
    }

    pub fn fill(
        &mut self,
        type_tag: CsTypeTag,
        cs_type: CsType,
        metadata: &CordlMetadata,
        config: &RustGenerationConfig,
    ) {
        let context_tag = self.get_context_root_tag(type_tag);

        if self.filled_types.contains(&type_tag) {
            return;
        }

        if self.borrowing_types.contains(&context_tag) {
            panic!("Borrowing context {context_tag:?}");
        }

        // Move ownership to local
        let cpp_type_entry = self
            .all_contexts
            .get_mut(&context_tag)
            .expect("No cpp context")
            .typedef_types
            .remove_entry(&type_tag);

        // In some occasions, the CppContext can be empty
        if let Some((_t, mut cpp_type)) = cpp_type_entry {
            self.do_fill_rust_type(&mut cpp_type, cs_type, metadata, config);

            // Move ownership back up
            self.all_contexts
                .get_mut(&context_tag)
                .expect("No cpp context")
                .insert_rust_type(cpp_type);
        }
    }

    ///
    /// By default will only look for nested types of the context, ignoring other CppTypes
    ///
    pub fn get_rust_type(&self, ty: CsTypeTag) -> Option<&RustType> {
        let context_root_tag = self.get_context_root_tag(ty);

        self.get_context(context_root_tag)
            .and_then(|c| c.get_types().get(&ty))
    }

    ///
    /// By default will only look for nested types of the context, ignoring other CppTypes
    ///
    pub fn get_cpp_type_mut(&mut self, ty: CsTypeTag) -> Option<&mut RustType> {
        let context_root_tag = self.get_context_root_tag(ty);

        self.get_context_mut(context_root_tag)
            .and_then(|c| c.get_types_mut().get_mut(&ty))
    }

    pub fn borrow_cpp_type<F>(&mut self, ty: CsTypeTag, func: F)
    where
        F: Fn(&mut Self, RustType) -> RustType,
    {
        let context_ty = self.get_context_root_tag(ty);
        if self.borrowing_types.contains(&context_ty) {
            panic!("Already borrowing this context!");
        }

        let context = self.all_contexts.get_mut(&context_ty).unwrap();

        // TODO: Needed?
        // self.borrowing_types.insert(context_ty);

        // search in root
        // clone to avoid failing il2cpp_name
        let Some(declaring_cpp_type) = context.typedef_types.get(&ty).cloned() else {
            panic!("No type {context_ty:#?} found!")
        };
        let _old_tag = declaring_cpp_type.self_tag;
        let new_cpp_ty = func(self, declaring_cpp_type);

        let context = self.all_contexts.get_mut(&context_ty).unwrap();

        context.insert_rust_type(new_cpp_ty);

        self.borrowing_types.remove(&context_ty);
    }

    pub fn get_context(&self, type_tag: CsTypeTag) -> Option<&RustContext> {
        let context_tag = self.get_context_root_tag(type_tag);
        if self.borrowing_types.contains(&context_tag) {
            panic!("Borrowing this context! {context_tag:?}");
        }
        self.all_contexts.get(&context_tag)
    }
    pub fn get_context_mut(&mut self, type_tag: CsTypeTag) -> Option<&mut RustContext> {
        let context_tag = self.get_context_root_tag(type_tag);
        if self.borrowing_types.contains(&context_tag) {
            panic!("Borrowing this context! {context_tag:?}");
        }
        self.all_contexts
            .get_mut(&self.get_context_root_tag(context_tag))
    }

    pub fn get_context_root_tag(&self, ty: CsTypeTag) -> CsTypeTag {
        self.alias_context
            .get(&ty)
            .cloned()
            // .map(|t| self.get_context_root_tag(*t))
            .unwrap_or(ty)
    }

    pub(crate) fn get(&self) -> &HashMap<CsTypeTag, RustContext> {
        &self.all_contexts
    }
    pub(crate) fn get_mut(&mut self) -> &mut HashMap<CsTypeTag, RustContext> {
        &mut self.all_contexts
    }

    pub fn write_all(&self, config: &RustGenerationConfig) -> color_eyre::Result<()> {
        let amount = self.all_contexts.len() as f64;
        self.all_contexts
            .iter()
            .enumerate()
            .try_for_each(|(i, (_, c))| {
                trace!(
                    "Writing {:.4}% ({}/{}) {}",
                    (i as f64 / amount * 100.0),
                    i,
                    amount,
                    c.fundamental_path.display(),
                );
                c.write(config)
            })
    }

    pub fn write_feature_block(&self, config: &RustGenerationConfig) -> color_eyre::Result<()> {
        let dependency_graph: Vec<(&RustType, Vec<_>)> = self
            .all_contexts
            .values()
            .flat_map(|c| c.typedef_types.values())
            .filter(|t| t.self_feature.is_some())
            .map(|t| {
                let dependencies = t
                    .requirements
                    .get_dependencies()
                    .iter()
                    .filter(|o| **o != t.self_tag)
                    .filter_map(|o| self.get_rust_type(*o))
                    .filter_map(|o| o.self_feature.as_ref())
                    .collect_vec();
                (t, dependencies)
            })
            .collect();

        let feature_block = dependency_graph
            // combine all features with same name that somehow exist
            .into_iter()
            .into_group_map_by(|(t, _)| t.self_feature.as_ref().unwrap().name.clone())
            .into_iter()
            .map(|(feature_name, features)| {
                (
                    feature_name,
                    features.into_iter().fold(Vec::new(), |mut a, b| {
                        a.extend(b.1);
                        a
                    }),
                )
            })
            // make feature block
            .map(|(feature_name, features)| {
                let dependencies = features
                    .iter()
                    .map(|s| format!("\"{}\"", s.name))
                    // Sort so things don't break git diffs
                    .sorted()
                    .join(", ");

                let feature = format!("\"{feature_name}\" = [{dependencies}]",);

                feature
            })
            .unique()
            // Sort so things don't break git diffs
            .sorted()
            .join("\n");

        let mut cargo_config =
            match std::fs::read_to_string("./cordl_internals_rs/Cargo_template.toml") {
                Ok(content) => content,
                Err(_) => {
                    eprintln!("Failed to load file `./cordl_internals_rs/Cargo_template.toml`");
                    return Err(color_eyre::eyre::eyre!("Failed to load Cargo template"));
                }
            };

        cargo_config = cargo_config.replace("#cordl_features", &feature_block);

        let mut file = File::create(&config.cargo_config)?;
        file.write_all(cargo_config.as_bytes())?;

        Ok(())
    }

    pub fn write_namespace_modules(&self, config: &RustGenerationConfig) -> color_eyre::Result<()> {
        info!("Writing namespace modules!");
        fn make_mod_dir(dir: &std::path::Path, name: &str) -> Result<(), color_eyre::eyre::Error> {
            if !dir.exists() {
                return Ok(());
            }

            let mut modules_paths = WalkDir::new(dir)
                .max_depth(1)
                .min_depth(1)
                .into_iter()
                .map(|c| c.map(|entry| entry.into_path()))
                .collect::<walkdir::Result<Vec<_>>>()?;

            // Sort so things don't break git diffs
            modules_paths.sort();

            if modules_paths.is_empty() {
                return Ok(());
            }

            let mod_path = dir.join(name).with_extension("rs");
            let mod_file = File::options()
                .truncate(false)
                .append(true)
                .create(true)
                .open(&mod_path)?;
            let mut buf_writer = BufWriter::new(mod_file);

            for module in &modules_paths {
                if module == dir || *module == mod_path {
                    continue;
                }

                if !module.exists() {
                    continue;
                }

                let file_stem = module.file_stem().unwrap().to_string_lossy();

                if module.is_dir() {
                    make_mod_dir(module, "mod.rs")?;
                    writeln!(buf_writer, "// namespace {};", file_stem)?;
                    writeln!(buf_writer, "pub mod {};", file_stem)?;
                } else if module.extension() == Some(OsStr::new("rs")) {
                    writeln!(buf_writer, "// class {}; export all", file_stem)?;
                    writeln!(buf_writer, "mod {};", file_stem)?;
                    writeln!(buf_writer, "pub use {}::*;", file_stem)?;
                }
            }

            buf_writer.flush()?;

            Ok(())
        }

        let mod_file = File::options()
            .create(true)
            .append(true)
            .truncate(false)
            .open(config.source_path.join("lib.rs"))?;
        let mut buf_writer = BufWriter::new(mod_file);
        writeln!(
            buf_writer,
            "
        #![feature(inherent_associated_types)]  

        #![allow(clippy::all)]
        #![allow(unused)]
        #![allow(non_snake_case)]
        #![allow(non_camel_case_types)]
        #![allow(non_upper_case_globals)]
        #![allow(non_ascii_idents)]
        #![allow(bad_style)]
        #![allow(clippy::module_name_repetitions)]
        #![allow(clippy::similar_names)]
        #![allow(clippy::case_sensitive_file_name)]
        #![allow(clippy::enum_variant_names)]
        #![allow(clippy::large_enum_variant)]
        "
        )?;
        buf_writer.flush()?;

        make_mod_dir(&config.source_path, "lib.rs")?;

        Ok(())
    }
}
