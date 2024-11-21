use std::{
    collections::{HashMap, HashSet},
    fs::File,
    io::Write,
};

use itertools::Itertools;
use log::{info, trace};
use pathdiff::diff_paths;

use crate::generate::{
    cpp::config::STATIC_CONFIG, cs_context_collection::TypeContextCollection, cs_type::CsType,
    cs_type_tag::CsTypeTag, metadata::CordlMetadata,
};

use super::{
    config::CppGenerationConfig, cpp_context::CppContext, cpp_name_resolver::CppNameResolver,
    cpp_type::CppType,
};

#[derive(Default)]
pub struct CppContextCollection {
    // Should always be a TypeDefinitionIndex
    all_contexts: HashMap<CsTypeTag, CppContext>,
    pub alias_context: HashMap<CsTypeTag, CsTypeTag>,
    filled_types: HashSet<CsTypeTag>,
    filling_types: HashSet<CsTypeTag>,
    borrowing_types: HashSet<CsTypeTag>,
}

impl CppContextCollection {
    pub fn from_cs_collection(
        collection: TypeContextCollection,
        metadata: &CordlMetadata,
        config: &CppGenerationConfig,
    ) -> CppContextCollection {
        let mut cpp_collection = CppContextCollection::default();

        info!("Making CppContextCollection from TypeContextCollection");
        for (tag, context) in collection.get() {
            cpp_collection
                .all_contexts
                .insert(*tag, CppContext::make(*tag, context, metadata, config));
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

    fn do_fill_cpp_type(
        &mut self,
        cpp_type: &mut CppType,
        cs_type: CsType,
        metadata: &CordlMetadata,
        config: &CppGenerationConfig,
    ) {
        let tag = cpp_type.self_tag;

        if self.filled_types.contains(&tag) {
            return;
        }
        if self.filling_types.contains(&tag) {
            panic!("Currently filling type {tag:?}, cannot fill")
        }

        // Move ownership to local
        self.filling_types.insert(tag);

        let name_resolver = CppNameResolver {
            cordl_metadata: metadata,
            collection: self,
        };

        cpp_type.fill(cs_type, &name_resolver, config);

        self.filled_types.insert(tag);
        self.filling_types.remove(&tag.clone());
    }

    pub fn fill(
        &mut self,
        type_tag: CsTypeTag,
        cs_type: CsType,
        metadata: &CordlMetadata,
        config: &CppGenerationConfig,
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
            self.do_fill_cpp_type(&mut cpp_type, cs_type, metadata, config);

            // Move ownership back up
            self.all_contexts
                .get_mut(&context_tag)
                .expect("No cpp context")
                .insert_cpp_type(cpp_type);
        }
    }

    ///
    /// By default will only look for nested types of the context, ignoring other CppTypes
    ///
    pub fn get_cpp_type(&self, ty: CsTypeTag) -> Option<&CppType> {
        let context_root_tag = self.get_context_root_tag(ty);

        self.get_context(context_root_tag)
            .and_then(|c| c.get_types().get(&ty))
    }

    ///
    /// By default will only look for nested types of the context, ignoring other CppTypes
    ///
    pub fn get_cpp_type_mut(&mut self, ty: CsTypeTag) -> Option<&mut CppType> {
        let context_root_tag = self.get_context_root_tag(ty);

        self.get_context_mut(context_root_tag)
            .and_then(|c| c.get_types_mut().get_mut(&ty))
    }

    pub fn borrow_cpp_type<F>(&mut self, ty: CsTypeTag, func: F)
    where
        F: Fn(&mut Self, CppType) -> CppType,
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

        context.insert_cpp_type(new_cpp_ty);

        self.borrowing_types.remove(&context_ty);
    }

    pub fn get_context(&self, type_tag: CsTypeTag) -> Option<&CppContext> {
        let context_tag = self.get_context_root_tag(type_tag);
        if self.borrowing_types.contains(&context_tag) {
            panic!("Borrowing this context! {context_tag:?}");
        }
        self.all_contexts.get(&context_tag)
    }
    pub fn get_context_mut(&mut self, type_tag: CsTypeTag) -> Option<&mut CppContext> {
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

    pub(crate) fn get(&self) -> &HashMap<CsTypeTag, CppContext> {
        &self.all_contexts
    }

    pub fn write_all(&self, config: &CppGenerationConfig) -> color_eyre::Result<()> {
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

    pub fn write_namespace_headers(&self) -> color_eyre::Result<()> {
        self.all_contexts
            .iter()
            .into_group_map_by(|(_, c)| c.fundamental_path.parent())
            .into_iter()
            .try_for_each(|(dir, contexts)| -> color_eyre::Result<()> {
                let namespace = if dir.unwrap() == STATIC_CONFIG.header_path {
                    "GlobalNamespace"
                } else {
                    dir.unwrap().file_name().unwrap().to_str().unwrap()
                };

                let str = contexts
                    .iter()
                    // ignore empty contexts
                    .filter(|(_, c)| !c.typedef_types.is_empty())
                    // ignore weird named types
                    .filter(|(_, c)| {
                        !c.fundamental_path
                            .file_name()
                            .unwrap()
                            .to_str()
                            .unwrap()
                            .starts_with('_')
                    })
                    // add includes
                    .map(|(_, c)| {
                        let stripped_path =
                            diff_paths(&c.fundamental_path, &STATIC_CONFIG.header_path).unwrap();

                        let stripped_path_friendly = if cfg!(windows) {
                            stripped_path.to_string_lossy().replace('\\', "/")
                        } else {
                            stripped_path.to_string_lossy().to_string()
                        };
                        // replace \\ to / on Windows
                        format!("#include \"{stripped_path_friendly}\"")
                    })
                    .sorted()
                    .unique()
                    .join("\n");

                let path = dir.unwrap().join(namespace).with_extension("hpp");

                info!(
                    "Creating namespace glob include {path:?} for {} files",
                    contexts.len()
                );

                let mut file = File::create(path)?;

                writeln!(
                    file,
                    "#ifdef __cpp_modules
                    module;
                    #endif
                "
                )?;
                writeln!(file, "#pragma once")?;
                file.write_all(str.as_bytes())?;

                writeln!(file)?;
                writeln!(
                    file,
                    "#ifdef __cpp_modules
                    export module {namespace};
                    #endif
                "
                )?;

                Ok(())
            })?;
        Ok(())
    }
}
