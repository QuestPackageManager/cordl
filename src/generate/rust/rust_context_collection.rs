use std::{
    collections::{HashMap, HashSet},
    ffi::OsStr,
    fs::File,
    io::{BufWriter, Write},
};

use itertools::Itertools;
use log::{info, trace};
use walkdir::WalkDir;

use crate::generate::{
    cs_context_collection::TypeContextCollection,
    cs_type::CsType,
    cs_type_tag::CsTypeTag,
    metadata::CordlMetadata,
    rust::{
        rust_members::RustFeature,
        rust_type::{CustomArc, RustTypeRequirement},
    },
};

use super::{
    config::RustGenerationConfig, rust_context::RustContext, rust_name_resolver::RustNameResolver,
    rust_type::RustType,
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
        let mut rs_collection = RustContextCollection::default();

        info!("Making RustContextCollection from TypeContextCollection");
        for (tag, context) in collection.get() {
            rs_collection
                .all_contexts
                .insert(*tag, RustContext::make(*tag, context, metadata, config));
        }
        rs_collection.alias_context = collection.alias_context;

        info!("Filling typedefs in RustContextCollection");
        for (_, context) in collection.all_contexts {
            for (tag, cs_type) in context.typedef_types {
                rs_collection.fill(tag, cs_type, metadata, config);
            }
        }

        rs_collection
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
        let rs_type_entry = self
            .all_contexts
            .get_mut(&context_tag)
            .expect("No rs context")
            .typedef_types
            .remove_entry(&type_tag);

        // In some occasions, the rsContext can be empty
        if let Some((_t, mut rs_type)) = rs_type_entry {
            self.do_fill_rust_type(&mut rs_type, cs_type, metadata, config);

            // Move ownership back up
            self.all_contexts
                .get_mut(&context_tag)
                .expect("No rs context")
                .insert_rust_type(rs_type);
        }
    }

    ///
    /// By default will only look for nested types of the context, ignoring other rsTypes
    ///
    pub fn get_rust_type(&self, ty: CsTypeTag) -> Option<&RustType> {
        let context_root_tag = self.get_context_root_tag(ty);

        self.get_context(context_root_tag)
            .and_then(|c| c.get_types().get(&ty))
    }

    ///
    /// By default will only look for nested types of the context, ignoring other rsTypes
    ///
    pub fn get_rs_type_mut(&mut self, ty: CsTypeTag) -> Option<&mut RustType> {
        let context_root_tag = self.get_context_root_tag(ty);

        self.get_context_mut(context_root_tag)
            .and_then(|c| c.get_types_mut().get_mut(&ty))
    }

    pub fn borrow_rs_type<F>(&mut self, ty: CsTypeTag, func: F)
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
        // clone to avoid failing il2rs_name
        let Some(declaring_rs_type) = context.typedef_types.get(&ty).cloned() else {
            panic!("No type {context_ty:#?} found!")
        };
        let _old_tag = declaring_rs_type.self_tag;
        let new_rs_ty = func(self, declaring_rs_type);

        let context = self.all_contexts.get_mut(&context_ty).unwrap();

        context.insert_rust_type(new_rs_ty);

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

    /// Writes the Cargo.toml file with all features
    pub fn write_feature_block(&self, config: &RustGenerationConfig) -> color_eyre::Result<()> {
        fn get_dependencies<'a>(
            iter: impl Iterator<Item = &'a RustTypeRequirement>,
            this: &RustContextCollection,
            t_self_tag: &CsTypeTag,
        ) -> Vec<CustomArc<RustFeature>> {
            iter.filter(|o| ***o != *t_self_tag)
                .filter_map(|req| {
                    let t = this.get_rust_type(**req)?;

                    // Skip compiler generated types
                    if t.is_compiler_generated {
                        return None;
                    }
                    
                    match req {
                        RustTypeRequirement::Definition(_) => t.self_def_feature.clone(),
                        RustTypeRequirement::Implementation(_) => t.self_impl_feature.clone(),
                    }
                })
                .collect_vec()
        }

        // Group each rust type to its list of dependencies
        let dependency_graph: Vec<(&RustType, (Vec<_>, Vec<_>))> = self
            .all_contexts
            .values()
            // each context has types
            .flat_map(|c| c.typedef_types.values())
            // filter out types that have no feature
            .filter(|t| t.self_def_feature.is_some() && t.self_impl_feature.is_some())
            .filter(|t| !t.is_compiler_generated)
            .map(|t| {
                let def_dependencies = get_dependencies(
                    t.requirements.get_def_dependencies().iter(),
                    self,
                    &t.self_tag,
                );
                let impl_dependencies = get_dependencies(
                    t.requirements.get_impl_dependencies().iter(),
                    self,
                    &t.self_tag,
                );

                (t, (def_dependencies, impl_dependencies))
            })
            .collect();

        // get def features
        // def features depend on def requirements
        let def_feature_block = dependency_graph
            // combine all features with same name that somehow exist
            .iter()
            .map(|(t, (def_features, _))| {
                let feature_name = t.self_def_feature.as_ref().unwrap().name.clone();
                (feature_name, def_features.clone())
            })
            .collect_vec();

        // make each impl feature depend on its def feature and its impl features
        // impl features depend on def of itself and its impl requirements
        let impl_feature_block = dependency_graph
            .into_iter()
            .filter_map(|(t, (_, impl_features))| -> Option<_> {
                // add the def feature as a dependency
                let impl_features = std::iter::once(t.self_def_feature.clone()?)
                    .chain(impl_features)
                    .collect_vec();

                let name = t.self_impl_feature.as_ref()?.name.clone();

                Some((name, impl_features))
            })
            .collect_vec();

        // combine
        let feature_blocks = def_feature_block
            .into_iter()
            .chain(impl_feature_block)
            .unique_by(|(name, _)| name.clone())
            .map(|(name, dependencies)| {
                let formatted_dependencies = dependencies
                    .into_iter()
                    .map(|s| format!("\"{}\"", s.name))
                    .unique()
                    // Sort so things don't break git diffs
                    .sorted()
                    .join(", ");
                format!("\"{}\" = [{}]", name, formatted_dependencies)
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

        cargo_config = cargo_config.replace("#cordl_features", &feature_blocks);

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
