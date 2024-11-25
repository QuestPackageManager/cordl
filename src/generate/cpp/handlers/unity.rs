use color_eyre::{eyre::ContextCompat, Result};
use log::info;
use std::{path::PathBuf, sync::Arc};

use crate::{
    data::{name_components::NameComponents, type_resolver::TypeUsage},
    generate::{
        cpp::{
            cpp_context_collection::CppContextCollection,
            cpp_members::{CppInclude, CppMember},
            cpp_type::CppType,
        },
        cs_type_tag::CsTypeTag,
        metadata::{CordlMetadata, Il2cppFullName},
        type_extensions::TypeDefinitionExtensions,
    },
};

pub fn register_unity(
    metadata: &CordlMetadata,
    cpp_context_collection: &mut CppContextCollection,
) -> Result<()> {
    info!("Registering UnityEngine.Object resolve handler!");

    let unity_object_tdi = *metadata
        .name_to_tdi
        .get(&Il2cppFullName("UnityEngine", "Object"))
        .expect("No UnityEngine.Object TDI found");

    let tag = CsTypeTag::TypeDefinitionIndex(unity_object_tdi);

    let _cpp_type = cpp_context_collection
        .get_cpp_type_mut(tag)
        .wrap_err("No System.Object type found")?;
    // unity_object_handler(cpp_type);

    Ok(())
}

pub fn unity_object_resolve_handler(
    original: NameComponents,
    cpp_type: &CppType,
    metadata: &CordlMetadata,
    typ_usage: TypeUsage,
) -> NameComponents {
    if !matches!(
        typ_usage,
        TypeUsage::Field | TypeUsage::Property | TypeUsage::GenericArg | TypeUsage::ReturnType
    ) {
        return original;
    }

    let tdi = cpp_type.self_tag.get_tdi();
    let td = &metadata.metadata.global_metadata.type_definitions[tdi];

    let unity_td = &metadata.metadata.global_metadata.type_definitions[metadata.unity_object_tdi];

    if !td.is_assignable_to(unity_td, metadata.metadata) {
        return original;
    }

    NameComponents {
        namespace: Some("".to_string()),
        declaring_types: None,
        name: "UnityW".to_string(),
        generics: Some(vec![original.remove_pointer().combine_all()]),
        is_pointer: false,
    }
}

fn unity_object_handler(cpp_type: &mut CppType) {
    info!("Found UnityEngine.Object type, adding UnityW!");
    cpp_type.parent = Some("bs_hook::UnityW".to_string());

    let path = PathBuf::from(r"beatsaber-hook/shared/utils/unityw.hpp");

    cpp_type
        .requirements
        .add_def_include(None, CppInclude::new_exact(path));

    // Fixup ctor call declarations
    cpp_type
        .declarations
        .iter_mut()
        .filter(|t| matches!(t.as_ref(), CppMember::ConstructorDecl(_)))
        .for_each(|d| {
            let CppMember::ConstructorDecl(constructor) = Arc::get_mut(d).unwrap() else {
                panic!()
            };

            if let Some(base_ctor) = &mut constructor.base_ctor {
                base_ctor.0 = "UnityW".to_string();
            }
        });
    // Fixup ctor call implementations
    cpp_type
        .implementations
        .iter_mut()
        .filter(|t| matches!(t.as_ref(), CppMember::ConstructorImpl(_)))
        .for_each(|d| {
            let CppMember::ConstructorImpl(constructor) = Arc::get_mut(d).unwrap() else {
                panic!()
            };

            if let Some(base_ctor) = &mut constructor.base_ctor {
                base_ctor.0 = "UnityW".to_string();
            }
        });
}
