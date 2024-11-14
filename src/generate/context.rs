use std::collections::HashMap;

use brocolib::global_metadata::TypeDefinitionIndex;
use log::info;

use super::{
    config::GenerationConfig, cs_type::CsType, cs_type_tag::CsTypeTag, metadata::Metadata,
};

// Holds the contextual information for creating a C++ file
// Will hold various metadata, such as includes, type definitions, and extraneous writes
#[derive(Debug, Clone)]
pub struct TypeContext {
    // Types to write, typedef
    pub typedef_types: HashMap<CsTypeTag, CsType>,
}

impl TypeContext {
    pub fn get_type_recursive_mut(
        &mut self,
        root_tag: CsTypeTag,
        child_tag: CsTypeTag,
    ) -> Option<&mut CsType> {
        let ty = self.typedef_types.get_mut(&root_tag);
        if root_tag == child_tag {
            return ty;
        }

        ty.and_then(|ty| ty.get_nested_type_mut(child_tag))
    }
    pub fn get_cpp_type_recursive(
        &self,
        root_tag: CsTypeTag,
        child_tag: CsTypeTag,
    ) -> Option<&CsType> {
        let ty = self.typedef_types.get(&root_tag);
        // if a root type
        if root_tag == child_tag {
            return ty;
        }

        ty.and_then(|ty| ty.get_nested_type(child_tag))
    }

    pub fn get_types(&self) -> &HashMap<CsTypeTag, CsType> {
        &self.typedef_types
    }

    // TODO: Move out, this is CSContext
    pub fn make(
        metadata: &Metadata,
        config: &GenerationConfig,
        tdi: TypeDefinitionIndex,
        tag: CsTypeTag,
        generic_inst: Option<&Vec<usize>>,
    ) -> TypeContext {
        let t = &metadata.metadata.global_metadata.type_definitions[tdi];

        let components = t.get_name_components(metadata.metadata);

        let ns = &components.namespace.unwrap_or_default();
        let name = &components.name;

        let cpp_namespace = config.namespace_cpp(ns);
        let cpp_name = config.namespace_cpp(name);

        let ns_path = config.namespace_path(ns);
        let path = if ns_path.is_empty() {
            "GlobalNamespace/".to_string()
        } else {
            ns_path + "/"
        };
        let path_name = match t.declaring_type_index != u32::MAX {
            true => {
                let name = config.path_name(name);
                let base_name = components.declaring_types.unwrap_or_default().join("_");

                format!("{base_name}_{name}")
            }
            false => config.path_name(name),
        };

        let mut x = TypeContext {
            typedef_types: Default::default(),
        };

        match CsType::make_cpp_type(metadata, config, tdi, tag, generic_inst) {
            Some(cpptype) => {
                x.insert_cpp_type(cpptype);
            }
            None => {
                info!(
                    "Unable to create valid CppContext for type: {}!",
                    t.full_name(metadata.metadata, true)
                );
            }
        }

        x
    }

    pub fn insert_cpp_type(&mut self, cpp_type: CsType) {
        if cpp_type.nested {
            panic!(
                "Cannot have a root type as a nested type! {}",
                &cpp_type.cpp_name_components.combine_all()
            );
        }
        self.typedef_types.insert(cpp_type.self_tag, cpp_type);
    }
}
