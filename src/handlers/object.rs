use color_eyre::Result;
use log::info;

use crate::generate::{
    cs_type::CsType,
    cs_type::IL2CPP_OBJECT_TYPE,
    cs_members::CsMember,
    metadata::{Il2cppFullName, Metadata},
};

pub fn register_system(metadata: &mut Metadata) -> Result<()> {
    info!("Registering system handler!");
    register_system_object_type_handler(metadata)?;

    Ok(())
}

fn register_system_object_type_handler(metadata: &mut Metadata) -> Result<()> {
    info!("Registering System.Object handler!");

    let system_object_tdi = metadata
        .name_to_tdi
        .get(&Il2cppFullName("System", "Object"))
        .expect("No System.Object TDI found");

    metadata
        .custom_type_handler
        .insert(*system_object_tdi, Box::new(system_object_handler));

    Ok(())
}

fn system_object_handler(cpp_type: &mut CsType) {
    info!("Found System.Object type, adding systemW!");
    // clear inherit so that bs hook can dof include order shenanigans
    cpp_type.requirements.need_wrapper();
    cpp_type.inherit = vec![IL2CPP_OBJECT_TYPE.to_string()];

    // Remove field because it does not size properly and is not necessary
    cpp_type
        .members
        .retain(|t| !matches!(t.as_ref(), CsMember::FieldDecl(f) if f.instance));

    // remove size assert too because System::Object will be wrong due to include ordering
    // cpp_type
    //     .nonmember_declarations
    //     .retain(|t| !matches!(t.as_ref(), CppNonMember::CppStaticAssert(_)));
}
