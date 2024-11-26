use std::{collections::HashMap, path::{self, PathBuf}};

use crate::generate::cs_type_tag::CsTypeTag;

use super::rust_type::RustType;

pub struct RustContext {
    // combined header
    pub fundamental_path: PathBuf,

    // Types to write, typedef
    pub typedef_types: HashMap<CsTypeTag, RustType>,
}

impl RustContext {
    pub(crate) fn make(
        tag: crate::generate::cs_type_tag::CsTypeTag,
        context: &crate::generate::context::TypeContext,
        metadata: &crate::generate::metadata::CordlMetadata<'_>,
        config: &super::config::RustGenerationConfig,
    ) -> RustContext {
        todo!()
    }

    /// Returns an immutable reference to the map of C++ types.
    pub fn get_types(&self) -> &HashMap<CsTypeTag, RustType> {
        &self.typedef_types
    }

    /// Returns a mutable reference to the map of C++ types.
    pub fn get_types_mut(&mut self) -> &mut HashMap<CsTypeTag, RustType> {
        &mut self.typedef_types
    }
    
    pub(crate) fn write(&self, config: &super::config::RustGenerationConfig) -> Result<(), color_eyre::eyre::Error> {
        todo!()
    }
    
    pub(crate) fn insert_rust_type(&self, new_cpp_ty: RustType) {
        todo!()
    }

    pub fn get_module_path(&self, config: &super::config::RustGenerationConfig) -> String {
        let relative_path = pathdiff::diff_paths(&self.fundamental_path, &config.source_path).unwrap();


        let module_path = relative_path.file_stem().unwrap().to_str().unwrap().replace(path::MAIN_SEPARATOR, "::");
        format!("crate::{module_path}")
    }
}
