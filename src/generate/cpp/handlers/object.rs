use color_eyre::{eyre::ContextCompat, Result};
use log::info;

use crate::generate::{
    cpp::{
        cpp_context_collection::CppContextCollection, cpp_members::CppMember,
        cpp_name_resolver::IL2CPP_OBJECT_TYPE, cpp_type::CppType,
    },
    cs_type_tag::CsTypeTag,
    metadata::{CordlMetadata, Il2cppFullName},
};

pub fn register_system(
    metadata: &CordlMetadata,
    cpp_context_collection: &mut CppContextCollection,
) -> Result<()> {
    info!("Registering System.Object handler!");

    let system_object_tdi = metadata
        .name_to_tdi
        .get(&Il2cppFullName("System", "Object"))
        .expect("No System.Object TDI found");

    let tag = CsTypeTag::TypeDefinitionIndex(*system_object_tdi);

    let cpp_type = cpp_context_collection
        .get_cpp_type_mut(tag)
        .wrap_err("No System.Object type found")?;
    system_object_handler(cpp_type);

    Ok(())
}

fn system_object_handler(cpp_type: &mut CppType) {
    info!("Found System.Object type, adding systemW!");
    // clear inherit so that bs hook can dof include order shenanigans
    cpp_type.requirements.need_wrapper();
    cpp_type.parent = Some(IL2CPP_OBJECT_TYPE.to_string());

    // Remove field because it does not size properly and is not necessary
    cpp_type
        .declarations
        .retain(|t| !matches!(t.as_ref(), CppMember::FieldDecl(f) if f.instance));

    // remove size assert too because System::Object will be wrong due to include ordering
    // cpp_type
    //     .nonmember_declarations
    //     .retain(|t| !matches!(t.as_ref(), CppNonMember::CppStaticAssert(_)));
}
