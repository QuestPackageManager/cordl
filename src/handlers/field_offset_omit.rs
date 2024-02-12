use brocolib::{global_metadata::TypeDefinitionIndex, runtime_metadata::Il2CppType};
use color_eyre::Result;
use log::info;
use std::{path::PathBuf, rc::Rc};

use crate::{
    data::name_components::NameComponents,
    generate::{
        context_collection::{self, CppContextCollection},
        cpp_type::{self, CppType},
        cs_context_collection::CsContextCollection,
        members::{CppInclude, CppMember},
        metadata::{Il2cppFullName, Metadata, TypeUsage},
        type_extensions::TypeDefinitionExtensions,
    },
};

pub fn remove_field_coments(context_collection: &mut CppContextCollection) -> Result<()> {
    info!("Removing field comments");

    context_collection
        .get_mut_cpp_context_collection()
        .get_mut()
        .values_mut()
        .flat_map(|cpp_context| cpp_context.typedef_types.values_mut())
        .try_for_each(|cpp_type| -> Result<()> {
            cpp_type
                .declarations
                .iter_mut()
                .try_for_each(|d| -> Result<()> {
                    match Rc::make_mut(d) {
                        CppMember::FieldDecl(cpp_field_decl) => {
                            cpp_field_decl.brief_comment = None;
                        }
                        _ => {
                            return Ok(());
                        }
                    };
                    Ok(())
                })
        })?;

    Ok(())
}
