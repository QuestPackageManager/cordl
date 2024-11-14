use color_eyre::Result;
use log::info;
use std::rc::Rc;

use crate::generate::{
    cs_context_collection::TypeContextCollection, cs_context_collection::CsContextCollection,
    cs_members::CsMember,
};

pub fn remove_coments(context_collection: &mut TypeContextCollection) -> Result<()> {
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
                        CsMember::FieldDecl(cpp_field_decl) => {
                            cpp_field_decl.brief_comment = None;
                        }
                        CsMember::Property(cpp_property_decl) => {
                            cpp_property_decl.brief_comment = None;
                        }
                        CsMember::MethodDecl(cpp_method_decl) => {
                            cpp_method_decl.brief = None;
                        }
                        CsMember::ConstructorDecl(cpp_constructor_decl) => {
                            cpp_constructor_decl.brief = None;
                        }
                        _ => {
                            return Ok(());
                        }
                    };
                    Ok(())
                })?;

            cpp_type
                .nonmember_declarations
                .iter_mut()
                .try_for_each(|d| -> Result<()> {
                    match Rc::make_mut(d) {
                        CppNonMember::CppStaticAssert(static_asert) => {
                            static_asert.condition = "".to_string();
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
