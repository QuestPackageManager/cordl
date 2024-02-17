use color_eyre::Result;
use log::info;
use std::rc::Rc;

use crate::generate::{
    context_collection::CppContextCollection, cs_context_collection::CsContextCollection,
    members::CppMember,
};

pub fn remove_coments(context_collection: &mut CppContextCollection) -> Result<()> {
    info!("Removing comments");

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
                        CppMember::MethodDecl(cpp_method_decl) => {
                            cpp_method_decl.brief = None;
                        }
                        CppMember::ConstructorDecl(cpp_constructor_decl) => {
                            cpp_constructor_decl.brief = None;
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
