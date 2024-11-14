use std::collections::HashMap;

use brocolib::global_metadata::TypeDefinitionIndex;
use log::info;

use super::{
    cs_type::CsType, cs_type_tag::CsTypeTag, metadata::Metadata,
    type_extensions::TypeDefinitionExtensions,
};

// Holds the contextual information for creating a C++ file
// Will hold various metadata, such as includes, type definitions, and extraneous writes
#[derive(Debug, Clone)]
pub struct TypeContext {
    // Types to write, typedef
    pub typedef_types: HashMap<CsTypeTag, CsType>,
}

impl TypeContext {
    pub fn get_types(&self) -> &HashMap<CsTypeTag, CsType> {
        &self.typedef_types
    }
    pub fn get_types_mut(&mut self) -> &mut HashMap<CsTypeTag, CsType> {
        &mut self.typedef_types
    }

    // TODO: Move out, this is CSContext
    pub fn make(
        metadata: &Metadata,
        tdi: TypeDefinitionIndex,
        tag: CsTypeTag,
        generic_inst: Option<&Vec<usize>>,
    ) -> TypeContext {
        let t = &metadata.metadata.global_metadata.type_definitions[tdi];

        let components = t.get_name_components(metadata.metadata);

        let _ns = &components.namespace.unwrap_or_default();
        let _name = &components.name;

        let mut x = TypeContext {
            typedef_types: Default::default(),
        };

        match CsType::make_cs_type(metadata, tdi, tag, generic_inst) {
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
                &cpp_type.cs_name_components.combine_all()
            );
        }
        self.typedef_types.insert(cpp_type.self_tag, cpp_type);
    }
}
