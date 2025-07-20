use std::sync::Arc;

use color_eyre::{Result, eyre::ContextCompat};

use crate::generate::{
    cpp::{
        cpp_context_collection::CppContextCollection, cpp_members::CppMember, cpp_type::CppType,
    },
    cs_type_tag::CsTypeTag,
    metadata::{CordlMetadata, Il2cppFullName},
};

use log::info;

pub fn register_value_type(
    metadata: &CordlMetadata,
    cpp_context_collection: &mut CppContextCollection,
) -> Result<()> {
    info!("Registering System.ValueType handler!");
    info!("Registering System.Enum handler!");

    let value_type_tdi = metadata
        .name_to_tdi
        .get(&Il2cppFullName("System", "ValueType"))
        .expect("No System.ValueType TDI found");
    let enum_type_tdi = metadata
        .name_to_tdi
        .get(&Il2cppFullName("System", "Enum"))
        .expect("No System.ValueType TDI found");

    let value_type_tag = CsTypeTag::TypeDefinitionIndex(*value_type_tdi);
    let enum_type_tag = CsTypeTag::TypeDefinitionIndex(*enum_type_tdi);

    let value_cpp_type = cpp_context_collection
        .get_cpp_type_mut(value_type_tag)
        .wrap_err("No System.Object type found")?;

    value_type_handler(value_cpp_type);

    let enum_cpp_type = cpp_context_collection
        .get_cpp_type_mut(enum_type_tag)
        .wrap_err("No System.Object type found")?;

    enum_type_handler(enum_cpp_type);

    Ok(())
}

fn unified_type_handler(cpp_type: &mut CppType) {
    // We don't replace parent anymore
    // cpp_type.inherit = vec![base_ctor.to_string()];

    // Fixup ctor call
    cpp_type
        .implementations
        .retain_mut(|d| !matches!(d.as_ref(), CppMember::ConstructorImpl(_)));
    cpp_type
        .declarations
        .iter_mut()
        .filter(|t| matches!(t.as_ref(), CppMember::ConstructorDecl(_)))
        .for_each(|d| {
            let CppMember::ConstructorDecl(constructor) = Arc::get_mut(d).unwrap() else {
                panic!()
            };

            // We don't replace base ctor anymore
            // constructor.base_ctor = Some((base_ctor.to_string(), "".to_string()));
            constructor.body = Some(vec![]);
            constructor.is_constexpr = true;
        });

    // remove all method decl/impl
    cpp_type
        .declarations
        .retain(|t| !matches!(t.as_ref(), CppMember::MethodDecl(_)));
    // remove all method decl/impl
    cpp_type
        .implementations
        .retain(|t| !matches!(t.as_ref(), CppMember::MethodImpl(_)));

    // Remove method size structs
    cpp_type.nonmember_implementations.clear();
}
fn value_type_handler(cpp_type: &mut CppType) {
    info!("Found System.ValueType, removing inheritance!");
    unified_type_handler(cpp_type);
}
fn enum_type_handler(cpp_type: &mut CppType) {
    info!("Found System.Enum type, removing inheritance!");
    unified_type_handler(cpp_type);
}
