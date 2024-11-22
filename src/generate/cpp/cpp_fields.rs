use crate::data::type_resolver::{ResolvedTypeData, TypeUsage};
use crate::generate::cpp::cpp_type::CORDL_ACCESSOR_FIELD_PREFIX;

use crate::generate::cs_members::CsField;
use crate::generate::cs_type_tag::CsTypeTag;
use crate::generate::metadata::CordlMetadata;
use crate::generate::type_extensions::{
    TypeDefinitionExtensions, TypeDefinitionIndexExtensions, TypeExtentions,
};
use crate::generate::writer::CppWritable;

use itertools::Itertools;
use log::warn;

use std::sync::Arc;

use brocolib::runtime_metadata::Il2CppTypeEnum;

use brocolib::global_metadata::TypeDefinitionIndex;

use super::config::CppGenerationConfig;
use super::cpp_members::{
    CppFieldDecl, CppFieldImpl, CppInclude, CppNestedStruct, CppNestedUnion, CppNonMember,
    CppStaticAssert, CppTemplate,
};
use super::cpp_members::{
    CppLine, CppMember, CppMethodDecl, CppMethodImpl, CppParam, CppPropertyDecl,
};
use super::cpp_name_resolver::CppNameResolver;
use super::cpp_type::{CppType, CORDL_METHOD_HELPER_NAMESPACE};

pub fn handle_static_fields(
    cpp_type: &mut CppType,
    fields: &[CsField],
    name_resolver: &CppNameResolver,
    config: &CppGenerationConfig,
) {
    let metadata = name_resolver.cordl_metadata;

    let tdi = cpp_type.self_tag.get_tdi();
    let t = tdi.get_type_definition(metadata.metadata);

    // if no fields, skip
    if t.field_count == 0 {
        return;
    }

    // we want only static fields
    // we ignore constants
    for field_info in fields.iter().filter(|f| !f.instance && !f.is_const) {
        let _f_type = &field_info.field_ty;
        let f_name = &field_info.name;
        let f_offset = field_info.offset.unwrap_or(u32::MAX);
        let f_size = field_info.size;

        let f_cpp_decl = make_cpp_field_decl(cpp_type, field_info, name_resolver, config);

        let field_ty_cpp_name = &f_cpp_decl.field_ty;

        // non const field
        // instance field access on ref types is special
        // ref type instance fields are specially named because the field getters are supposed to be used
        let f_cpp_name = f_cpp_decl.cpp_name.clone();

        let klass_resolver = cpp_type.classof_cpp_name();

        let getter_call =
                format!("return {CORDL_METHOD_HELPER_NAMESPACE}::getStaticField<{field_ty_cpp_name}, \"{f_name}\", {klass_resolver}>();");

        let setter_var_name = "value";
        let setter_call =
                format!("{CORDL_METHOD_HELPER_NAMESPACE}::setStaticField<{field_ty_cpp_name}, \"{f_name}\", {klass_resolver}>(std::forward<{field_ty_cpp_name}>({setter_var_name}));");

        // don't get a template that has no names
        let useful_template =
            cpp_type
                .cpp_template
                .clone()
                .and_then(|t| match t.names.is_empty() {
                    true => None,
                    false => Some(t),
                });

        let getter_name = format!("getStaticF_{}", f_cpp_name);
        let setter_name = format!("setStaticF_{}", f_cpp_name);

        let get_return_type = field_ty_cpp_name.clone();

        let getter_decl = CppMethodDecl {
            cpp_name: getter_name.clone(),
            instance: false,
            return_type: get_return_type,

            brief: None,
            body: None, // TODO:
            // Const if instance for now
            is_const: false,
            is_constexpr: field_info.instance || field_info.is_const,
            is_inline: true,
            is_virtual: false,
            is_implicit_operator: false,
            is_explicit_operator: false,
            is_no_except: false, // TODO:
            parameters: vec![],
            prefix_modifiers: vec![],
            suffix_modifiers: vec![],
            template: None,
        };

        let setter_decl = CppMethodDecl {
            cpp_name: setter_name,
            instance: false,
            return_type: "void".to_string(),

            brief: None,
            body: None,      //TODO:
            is_const: false, // TODO: readonly fields?
            is_constexpr: field_info.instance || field_info.is_const,
            is_inline: true,
            is_virtual: false,
            is_implicit_operator: false,
            is_explicit_operator: false,
            is_no_except: false, // TODO:
            parameters: vec![CppParam {
                def_value: None,
                modifiers: "".to_string(),
                name: setter_var_name.to_string(),
                ty: field_ty_cpp_name.clone(),
            }],
            prefix_modifiers: vec![],
            suffix_modifiers: vec![],
            template: None,
        };

        let getter_impl = CppMethodImpl {
            body: vec![Arc::new(CppLine::make(getter_call.clone()))],
            declaring_cpp_full_name: cpp_type.cpp_name_components.remove_pointer().combine_all(),
            template: useful_template.clone(),

            ..getter_decl.clone().into()
        };

        let setter_impl = CppMethodImpl {
            body: vec![Arc::new(CppLine::make(setter_call))],
            declaring_cpp_full_name: cpp_type.cpp_name_components.remove_pointer().combine_all(),
            template: useful_template.clone(),

            ..setter_decl.clone().into()
        };

        // instance fields on a ref type should declare a cpp property

        let prop_decl = CppPropertyDecl {
            cpp_name: f_cpp_name,
            prop_ty: field_ty_cpp_name.clone(),
            instance: field_info.instance,
            getter: getter_decl.cpp_name.clone().into(),
            setter: setter_decl.cpp_name.clone().into(),
            indexable: false,
            brief_comment: Some(format!(
                "Field {f_name}, offset 0x{f_offset:x}, size 0x{f_size:x} "
            )),
        };

        // only push accessors if declaring ref type, or if static field
        cpp_type
            .declarations
            .push(CppMember::Property(prop_decl).into());

        // decl
        cpp_type
            .declarations
            .push(CppMember::MethodDecl(setter_decl).into());

        cpp_type
            .declarations
            .push(CppMember::MethodDecl(getter_decl).into());

        // impl
        cpp_type
            .implementations
            .push(CppMember::MethodImpl(setter_impl).into());

        cpp_type
            .implementations
            .push(CppMember::MethodImpl(getter_impl).into());
    }
}

pub(crate) fn handle_const_fields(
    cpp_type: &mut CppType,
    fields: &[CsField],
    name_resolver: &CppNameResolver,
    config: &CppGenerationConfig,
) {
    let metadata = name_resolver.cordl_metadata;

    // if no fields, skip
    if fields.is_empty() {
        return;
    }

    let declaring_cpp_template = if cpp_type
        .cpp_template
        .as_ref()
        .is_some_and(|t| !t.names.is_empty())
    {
        cpp_type.cpp_template.clone()
    } else {
        None
    };

    for field_info in fields.iter().filter(|f| f.is_const) {
        let cpp_field_template = make_cpp_field_decl(cpp_type, field_info, name_resolver, config);
        let f_resolved_type = &field_info.field_ty;
        let f_type = field_info.field_ty.get_type(metadata);
        let f_name = &field_info.name;
        let f_offset = field_info.offset.unwrap_or(u32::MAX);
        let f_size = field_info.size;

        let def_value = field_info.value.as_ref();

        let def_value = def_value.expect("Constant with no default value?");

        match f_resolved_type.data {
            ResolvedTypeData::Primitive(_) => {
                // primitive type
                let field_decl = CppFieldDecl {
                    instance: false,
                    const_expr: true,
                    readonly: field_info.readonly,

                    brief_comment: Some(format!(
                        "Field {f_name} offset 0x{f_offset:x} size 0x{f_size:x}"
                    )),
                    value: Some(def_value.to_string()),
                    ..cpp_field_template
                };

                cpp_type
                    .declarations
                    .push(CppMember::FieldDecl(field_decl).into());
            }
            _ => {
                // other type
                let field_decl = CppFieldDecl {
                    instance: false,
                    readonly: field_info.readonly,
                    value: None,
                    const_expr: false,
                    brief_comment: Some(format!("Field {f_name} value: {def_value:?}")),
                    ..cpp_field_template.clone()
                };
                let field_impl = CppFieldImpl {
                    value: def_value.to_string(),
                    const_expr: true,
                    declaring_type: cpp_type.cpp_name_components.remove_pointer().combine_all(),
                    declaring_type_template: declaring_cpp_template.clone(),
                    ..cpp_field_template.clone().into()
                };

                // get enum type to include impl
                // this is needed since the enum constructor is not defined
                // in the declaration
                // TODO: Make enum ctors inline defined
                if f_type.valuetype && f_type.ty == Il2CppTypeEnum::Valuetype {
                    let field_cpp_tag: CsTypeTag =
                        CsTypeTag::from_type_data(f_type.data, metadata.metadata);
                    let field_cpp_td_tag: CsTypeTag = field_cpp_tag.get_tdi().into();
                    let field_cpp_type = name_resolver.collection.get_cpp_type(field_cpp_td_tag);

                    if field_cpp_type.is_some_and(|f| f.is_enum_type) {
                        let field_cpp_context = name_resolver
                            .collection
                            .get_context(field_cpp_td_tag)
                            .expect("No context for cpp enum type");

                        cpp_type.requirements.add_impl_include(
                            field_cpp_type,
                            CppInclude::new_context_typeimpl(field_cpp_context),
                        );
                    }
                }

                cpp_type
                    .declarations
                    .push(CppMember::FieldDecl(field_decl).into());
                cpp_type
                    .implementations
                    .push(CppMember::FieldImpl(field_impl).into());
            }
        }
    }
}

pub(crate) fn handle_instance_fields(
    cpp_type: &mut CppType,
    fields: &[CppFieldDecl],
    metadata: &CordlMetadata,
    tdi: TypeDefinitionIndex,
) {
    let t = tdi.get_type_definition(metadata.metadata);

    // if no fields, skip
    if t.field_count == 0 {
        return;
    }

    let instance_field_decls = fields
        .iter()
        .filter(|f| f.offset.is_some() && f.instance)
        .cloned()
        .collect_vec();

    let property_exists = |to_find: &str| {
        cpp_type.declarations.iter().any(|d| match d.as_ref() {
            CppMember::Property(p) => p.cpp_name == to_find,
            _ => false,
        })
    };

    let resulting_fields = instance_field_decls
        .into_iter()
        .map(|d| {
            let mut f = d;
            if property_exists(&f.cpp_name) {
                f.cpp_name = format!("_cordl_{}", &f.cpp_name);

                // make private if a property with this name exists
                f.is_private = true;
            }

            f
        })
        .collect_vec();

    // explicit layout types are packed into single unions
    if t.is_explicit_layout() {
        // oh no! the fields are unionizing! don't tell elon musk!
        let u = pack_fields_into_single_union(&resulting_fields);
        cpp_type.declarations.push(CppMember::NestedUnion(u).into());
    } else {
        // TODO: Make field offset asserts for explicit layouts!
        add_field_offset_asserts(cpp_type, &resulting_fields);

        resulting_fields
            .into_iter()
            .map(|member| CppMember::FieldDecl(member))
            .for_each(|member| cpp_type.declarations.push(member.into()));
    };
}

fn add_field_offset_asserts(cpp_type: &mut CppType, fields: &[CppFieldDecl]) {
    // let cpp_name = if let Some(cpp_template) = &cpp_type.cpp_template {
    //     // We don't handle generic instantiations since we can't tell if a ge
    //     let mut name_components = cpp_type.cpp_name_components.clone();

    //     name_components.generics = name_components.generics.map(|generics| {
    //         generics
    //             .into_iter()
    //             .map(
    //                 |generic| match cpp_template.names.iter().any(|(ty, s)| &generic == s) {
    //                     true => "void*".to_string(),
    //                     false => generic,
    //                 },
    //             )
    //             .collect_vec()
    //     });

    //     name_components.remove_pointer().combine_all()
    // } else {
    //     cpp_type.cpp_name_components.remove_pointer().combine_all()
    // };

    // Skip generics for now
    if cpp_type.cpp_template.is_some() {
        return;
    }

    let cpp_name = cpp_type.cpp_name_components.remove_pointer().combine_all();
    for field in fields {
        let field_name = &field.cpp_name;
        let offset = field.offset.unwrap_or(u32::MAX);

        let assert = CppStaticAssert {
            condition: format!("offsetof({cpp_name}, {field_name}) == 0x{offset:x}"),
            message: Some("Offset mismatch!".to_string()),
        };
        // cpp_type
        //     .declarations
        //     .push(CppMember::CppStaticAssert(assert).into());

        cpp_type
            .nonmember_declarations
            .push(CppNonMember::CppStaticAssert(assert).into())
    }
}

pub(crate) fn fixup_backing_field(fieldname: &str) -> String {
    format!("{CORDL_ACCESSOR_FIELD_PREFIX}{fieldname}")
}

pub(crate) fn handle_valuetype_fields(
    cpp_type: &mut CppType,
    fields: &[CsField],
    name_resolver: &CppNameResolver,
    config: &CppGenerationConfig,
) {
    let metadata = name_resolver.cordl_metadata;
    let tdi = cpp_type.self_tag.get_tdi();
    // Value types only need getter fixes for explicit layout types
    let t = tdi.get_type_definition(metadata.metadata);

    // if no fields, skip
    if t.field_count == 0 {
        return;
    }

    // instance fields for explicit layout value types are special
    if t.is_explicit_layout() {
        for field_info in fields.iter().filter(|f| f.instance) {
            // don't get a template that has no names
            let template = cpp_type
                .cpp_template
                .clone()
                .filter(|t| !t.names.is_empty());

            let cpp_field_decl = make_cpp_field_decl(cpp_type, field_info, name_resolver, config);

            let prop = prop_decl_from_fieldinfo(metadata, field_info, &cpp_field_decl);
            let (accessor_decls, accessor_impls) =
                prop_methods_from_fieldinfo(field_info, template, cpp_type, name_resolver, config);

            cpp_type.declarations.push(CppMember::Property(prop).into());

            accessor_decls.into_iter().for_each(|method| {
                cpp_type
                    .declarations
                    .push(CppMember::MethodDecl(method).into());
            });

            accessor_impls.into_iter().for_each(|method| {
                cpp_type
                    .implementations
                    .push(CppMember::MethodImpl(method).into());
            });
        }

        let backing_fields = fields
            .iter()
            .map(|f| make_cpp_field_decl(cpp_type, &f, name_resolver, config))
            .map(|mut f| {
                f.cpp_name = fixup_backing_field(&f.cpp_name);
                f
            })
            .collect_vec();

        handle_instance_fields(cpp_type, &backing_fields, metadata, tdi);
    } else {
        let backing_fields = fields
            .iter()
            .map(|f| make_cpp_field_decl(cpp_type, &f, name_resolver, config))
            .collect_vec();

        handle_instance_fields(cpp_type, &backing_fields, metadata, tdi);
    }
}

// create prop and field declaration from passed field info
pub(crate) fn prop_decl_from_fieldinfo(
    _metadata: &CordlMetadata,
    cs_field: &CsField,
    cpp_field: &CppFieldDecl,
) -> CppPropertyDecl {
    if !cs_field.instance {
        panic!("Can't turn static fields into declspec properties!");
    }

    let f_name = &cs_field.name;
    let f_cpp_name = &cpp_field.cpp_name;
    let f_offset = cs_field.offset.unwrap_or(u32::MAX);
    let f_size = cs_field.size;
    let _field_ty_cpp_name = &cs_field.field_ty;

    let (getter_name, setter_name) = method_names_from_fieldinfo(f_cpp_name);

    CppPropertyDecl {
        cpp_name: f_cpp_name.clone(),
        prop_ty: cpp_field.field_ty.clone(),
        instance: cs_field.instance,
        getter: Some(getter_name),
        setter: Some(setter_name),
        indexable: false,
        brief_comment: Some(format!(
            "Field {f_name}, offset 0x{f_offset:x}, size 0x{f_size:x} "
        )),
    }
}

fn method_names_from_fieldinfo(f_cpp_name: &str) -> (String, String) {
    let getter_name = format!("__cordl_internal_get_{}", f_cpp_name);
    let setter_name = format!("__cordl_internal_set_{}", f_cpp_name);

    (getter_name, setter_name)
}

pub(crate) fn prop_methods_from_fieldinfo(
    field: &CsField,
    field_template: Option<CppTemplate>,
    cpp_type: &mut CppType,

    name_resolver: &CppNameResolver,
    config: &CppGenerationConfig,
) -> (Vec<CppMethodDecl>, Vec<CppMethodImpl>) {
    let metadata = name_resolver.cordl_metadata;
    let f_type = field.field_ty.get_type(metadata);

    let cpp_field = make_cpp_field_decl(cpp_type, field, name_resolver, config);
    let field_ty_cpp_name = &cpp_field.field_ty;
    let f_cpp_name = &cpp_field.cpp_name;

    let cordl_field_name = fixup_backing_field(f_cpp_name);
    let field_access = format!("this->{cordl_field_name}");

    let (getter_name, setter_name) = method_names_from_fieldinfo(f_cpp_name);

    // let (get_return_type, const_get_return_type) = match !f_type.valuetype {
    //     // Var types are default pointers
    //     true => (
    //         field_ty_cpp_name.clone(),
    //         format!("::cordl_internals::to_const_pointer<{field_ty_cpp_name}> const",),
    //     ),
    //     false => (
    //         field_ty_cpp_name.clone(),
    //         format!("{field_ty_cpp_name} const"),
    //     ),
    // };

    // field accessors emit as ref because they are fields, you should be able to access them the same
    let get_return_type = format!("{field_ty_cpp_name}&");
    let const_get_return_type = format!("{field_ty_cpp_name} const&");

    let declaring_is_ref = cpp_type.is_reference_type;

    // for ref types we emit an instance null check that is dependent on a compile time define,
    // that way we can prevent nullptr access and instead throw, if the user wants this
    // technically "this" should never ever be null, but in native modding this can happen
    let instance_null_check = match declaring_is_ref {
        true => Some("CORDL_FIELD_NULL_CHECK(static_cast<void const*>(this));"),
        false => None,
    };

    let getter_call = format!("return {field_access};");
    let setter_var_name = "value";
    // if the declaring type is a value type, we should not use wbarrier
    let setter_call = match !f_type.valuetype && declaring_is_ref {
        // setter for generic type
        true if field_template.as_ref().is_some_and(|s| !s.names.is_empty()) => {
            format!(
                "::cordl_internals::setInstanceField(this, &{field_access}, {setter_var_name});"
            )
        }
        // ref type field write on a ref type
        true => {
            format!("il2cpp_functions::gc_wbarrier_set_field(this, static_cast<void**>(static_cast<void*>(&{field_access})), cordl_internals::convert(std::forward<decltype({setter_var_name})>({setter_var_name})));")
        }
        false => {
            format!("{field_access} = {setter_var_name};")
        }
    };

    let getter_decl = CppMethodDecl {
        cpp_name: getter_name.clone(),
        instance: true,
        return_type: get_return_type,

        brief: None,
        body: None, // TODO:
        // Const if instance for now
        is_const: false,
        is_constexpr: !f_type.is_static() || f_type.is_constant(),
        is_inline: true,
        is_virtual: false,
        is_implicit_operator: false,
        is_explicit_operator: false,

        is_no_except: false, // TODO:
        parameters: vec![],
        prefix_modifiers: vec![],
        suffix_modifiers: vec![],
        template: None,
    };

    let const_getter_decl = CppMethodDecl {
        cpp_name: getter_name,
        instance: true,
        return_type: const_get_return_type,

        brief: None,
        body: None, // TODO:
        // Const if instance for now
        is_const: true,
        is_constexpr: !f_type.is_static() || f_type.is_constant(),
        is_inline: true,
        is_virtual: false,
        is_implicit_operator: false,
        is_explicit_operator: false,

        is_no_except: false, // TODO:
        parameters: vec![],
        prefix_modifiers: vec![],
        suffix_modifiers: vec![],
        template: None,
    };

    let setter_decl = CppMethodDecl {
        cpp_name: setter_name,
        instance: true,
        return_type: "void".to_string(),

        brief: None,
        body: None,      //TODO:
        is_const: false, // TODO: readonly fields?
        is_constexpr: !f_type.is_static() || f_type.is_constant(),
        is_inline: true,
        is_virtual: false,
        is_implicit_operator: false,
        is_explicit_operator: false,

        is_no_except: false, // TODO:
        parameters: vec![CppParam {
            def_value: None,
            modifiers: "".to_string(),
            name: setter_var_name.to_string(),
            ty: field_ty_cpp_name.clone(),
        }],
        prefix_modifiers: vec![],
        suffix_modifiers: vec![],
        template: None,
    };

    // construct getter and setter bodies
    let getter_body: Vec<Arc<dyn CppWritable>> =
        if let Some(instance_null_check) = instance_null_check {
            vec![
                Arc::new(CppLine::make(instance_null_check.into())),
                Arc::new(CppLine::make(getter_call)),
            ]
        } else {
            vec![Arc::new(CppLine::make(getter_call))]
        };

    let setter_body: Vec<Arc<dyn CppWritable>> =
        if let Some(instance_null_check) = instance_null_check {
            vec![
                Arc::new(CppLine::make(instance_null_check.into())),
                Arc::new(CppLine::make(setter_call)),
            ]
        } else {
            vec![Arc::new(CppLine::make(setter_call))]
        };

    let declaring_cpp_name = cpp_type.cpp_name_components.remove_pointer().combine_all();
    let template = cpp_type.cpp_template.clone();

    let getter_impl = CppMethodImpl {
        body: getter_body.clone(),
        declaring_cpp_full_name: declaring_cpp_name.clone(),
        template: template.clone(),

        ..getter_decl.clone().into()
    };

    let const_getter_impl = CppMethodImpl {
        body: getter_body,
        declaring_cpp_full_name: declaring_cpp_name.clone(),
        template: template.clone(),

        ..const_getter_decl.clone().into()
    };

    let setter_impl = CppMethodImpl {
        body: setter_body,
        declaring_cpp_full_name: declaring_cpp_name.clone(),
        template: template.clone(),

        ..setter_decl.clone().into()
    };

    (
        vec![getter_decl, const_getter_decl, setter_decl],
        vec![getter_impl, const_getter_impl, setter_impl],
    )
}

pub(crate) fn handle_referencetype_fields(
    cpp_type: &mut CppType,
    fields: &[CsField],
    name_resolver: &CppNameResolver,
    config: &CppGenerationConfig,
) {
    let metadata = name_resolver.cordl_metadata;
    let tdi = cpp_type.self_tag.get_tdi();
    let t = tdi.get_type_definition(metadata.metadata);

    if t.is_explicit_layout() {
        warn!(
            "Reference type with explicit layout: {}",
            cpp_type.cpp_name_components.combine_all()
        );
    }

    // if no fields, skip
    if t.field_count == 0 {
        return;
    }

    for field_info in fields.iter().filter(|f| f.instance) {
        // don't get a template that has no names
        let template = cpp_type
            .cpp_template
            .clone()
            .and_then(|t| match t.names.is_empty() {
                true => None,
                false => Some(t),
            });

        let _declaring_cpp_full_name = cpp_type.cpp_name_components.remove_pointer().combine_all();

        let cpp_field_decl = make_cpp_field_decl(cpp_type, field_info, name_resolver, config);

        let prop = prop_decl_from_fieldinfo(metadata, field_info, &cpp_field_decl);
        let (accessor_decls, accessor_impls) =
            prop_methods_from_fieldinfo(field_info, template, cpp_type, name_resolver, config);

        cpp_type.declarations.push(CppMember::Property(prop).into());

        accessor_decls.into_iter().for_each(|method| {
            cpp_type
                .declarations
                .push(CppMember::MethodDecl(method).into());
        });

        accessor_impls.into_iter().for_each(|method| {
            cpp_type
                .implementations
                .push(CppMember::MethodImpl(method).into());
        });
    }

    let backing_fields = fields
        .iter()
        .map(|f| make_cpp_field_decl(cpp_type, &f, name_resolver, config))
        .map(|mut f| {
            f.cpp_name = fixup_backing_field(&f.cpp_name);
            f
        })
        .collect_vec();

    handle_instance_fields(cpp_type, &backing_fields, metadata, tdi);
}

pub(crate) fn field_collision_check(instance_fields: &[CsField]) -> bool {
    let mut next_offset = 0;
    return instance_fields
        .iter()
        .sorted_by(|a, b| a.offset.cmp(&b.offset))
        .any(|field| {
            let offset = field.offset.unwrap_or(u32::MAX);
            if offset < next_offset {
                true
            } else {
                next_offset = offset + field.size as u32;
                false
            }
        });
}

// inspired by what il2cpp does for explicitly laid out types
pub(crate) fn pack_fields_into_single_union(fields: &[CppFieldDecl]) -> CppNestedUnion {
    // get the min offset to use as a base for the packed structs
    let min_offset = fields.iter().map(|f| f.offset.unwrap()).min().unwrap_or(0);

    let packed_structs = fields
        .into_iter()
        .cloned()
        .map(|field| {
            let structs = field_into_offset_structs(min_offset, field);

            vec![structs.0, structs.1]
        })
        .flat_map(|v| v.into_iter())
        .collect_vec();

    let declarations = packed_structs
        .into_iter()
        .map(|s| CppMember::NestedStruct(s).into())
        .collect_vec();

    CppNestedUnion {
        brief_comment: Some("Explicitly laid out type with union based offsets".into()),
        declarations,
        offset: min_offset,
        is_private: true,
    }
}

pub(crate) fn field_into_offset_structs(
    _min_offset: u32,
    field: CppFieldDecl,
) -> (CppNestedStruct, CppNestedStruct) {
    // il2cpp basically turns each field into 2 structs within a union:
    // 1 which is packed with size 1, and padded with offset to fit to the end
    // the other which has the same padding and layout, except this one is for alignment so it's just packed as the parent struct demands

    let Some(actual_offset) = &field.offset else {
        panic!("don't call field_into_offset_structs with non instance fields!")
    };

    let padding = actual_offset;

    let packed_padding_cpp_name = format!("{}_padding[0x{padding:x}]", field.cpp_name);
    let alignment_padding_cpp_name =
        format!("{}_padding_forAlignment[0x{padding:x}]", field.cpp_name);
    let alignment_cpp_name = format!("{}_forAlignment", field.cpp_name);

    let packed_padding_field = CppFieldDecl {
        brief_comment: Some(format!("Padding field 0x{padding:x}")),
        const_expr: false,
        cpp_name: packed_padding_cpp_name,
        field_ty: "uint8_t".into(),
        offset: Some(*actual_offset),
        instance: true,
        is_private: false,
        readonly: false,
        value: None,
    };

    let alignment_padding_field = CppFieldDecl {
        brief_comment: Some(format!("Padding field 0x{padding:x} for alignment")),
        const_expr: false,
        cpp_name: alignment_padding_cpp_name,
        field_ty: "uint8_t".into(),
        offset: Some(*actual_offset),
        instance: true,
        is_private: false,
        readonly: false,
        value: None,
    };

    let alignment_field = CppFieldDecl {
        cpp_name: alignment_cpp_name,
        is_private: false,
        ..field.clone()
    };

    let packed_field = CppFieldDecl {
        is_private: false,
        ..field
    };

    let packed_struct = CppNestedStruct {
        declaring_name: "".into(),
        base_type: None,
        declarations: vec![
            CppMember::FieldDecl(packed_padding_field).into(),
            CppMember::FieldDecl(packed_field).into(),
        ],
        brief_comment: None,
        is_class: false,
        is_enum: false,
        is_private: false,
        packing: Some(1),
    };

    let alignment_struct = CppNestedStruct {
        declaring_name: "".into(),
        base_type: None,
        declarations: vec![
            CppMember::FieldDecl(alignment_padding_field).into(),
            CppMember::FieldDecl(alignment_field).into(),
        ],
        brief_comment: None,
        is_class: false,
        is_enum: false,
        is_private: false,
        packing: None,
    };

    (packed_struct, alignment_struct)
}

pub fn make_cpp_field_decl(
    cpp_type: &mut CppType,
    field: &CsField,
    name_resolver: &CppNameResolver,
    config: &CppGenerationConfig,
) -> CppFieldDecl {
    let field_ty = field.field_ty.get_type(name_resolver.cordl_metadata);
    let field_resolved_ty = name_resolver
        .resolve_name(
            cpp_type,
            &field.field_ty,
            TypeUsage::Field,
            field_ty.valuetype || field_ty.ty == Il2CppTypeEnum::Valuetype || field_ty.ty == Il2CppTypeEnum::Enum,
            false,
        )
        .combine_all();
    let field_decl = CppFieldDecl {
        cpp_name: config.name_cpp_plus(&field.name, &[cpp_type.cpp_name().as_str()]),
        field_ty: field_resolved_ty,
        offset: field.offset,
        instance: field.instance,
        readonly: field.readonly,
        const_expr: field.is_const,
        value: field.value.as_ref().map(|v| v.to_string()),
        brief_comment: field.brief_comment.clone(),
        is_private: false,
    };

    field_decl
}
