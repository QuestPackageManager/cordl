use std::{collections::HashSet, fs};

use brocolib::runtime_metadata::{Il2CppTypeEnum, TypeData};

use super::*;
use crate::generate::{offsets, type_extensions::TypeDefinitionExtensions};

fn find_type(parsed: &brocolib::Metadata, namespace: &str, name: &str) -> TypeDefinitionIndex {
    let index = parsed
        .global_metadata
        .type_definitions
        .as_vec()
        .iter()
        .position(|ty| ty.namespace(parsed) == namespace && ty.name(parsed) == name)
        .expect("find required type");
    TypeDefinitionIndex::new(index as u32)
}

fn fixture_metadata<'a>(parsed: &'a brocolib::Metadata<'a, 'a>) -> CordlMetadata<'a> {
    let mut metadata = CordlMetadata {
        metadata: parsed,
        metadata_registration: &parsed.runtime_metadata.metadata_registration,
        code_registration: &parsed.runtime_metadata.code_registration,
        method_calculations: Default::default(),
        parent_to_child_map: Default::default(),
        child_to_parent_map: Default::default(),
        unity_object_tdi: find_type(parsed, "UnityEngine", "Object"),
        object_tdi: find_type(parsed, "System", "Object"),
        name_to_tdi: Default::default(),
        blacklisted_types: Default::default(),
        tdi_to_image: Default::default(),
        custom_attributes_by_image: Default::default(),
        pointer_size: PointerSize::Bytes8,
        packing_field_offset: 7,
        size_is_default_offset: 12,
        specified_packing_field_offset: 13,
        packing_is_default_offset: 11,
    };
    metadata.parse();
    metadata
}

/// Run with `cargo test --features il2cpp_v39 v39_enum_inheritance_and_layout -- --ignored --nocapture`.
/// The input files are local game data and are intentionally not committed.
#[test]
#[ignore = "requires matching v39 global-metadata.dat and libil2cpp.so in the working directory"]
fn v39_enum_inheritance_and_layout() {
    let global = fs::read("global-metadata.dat").expect("read global-metadata.dat");
    let library = fs::read("libil2cpp.so").expect("read libil2cpp.so");
    let parsed = brocolib::Metadata::parse(&global, &library).expect("parse v39 metadata");
    let metadata = fixture_metadata(&parsed);

    let definitions = &parsed.global_metadata.type_definitions;
    let enum_base = &definitions[find_type(&parsed, "System", "Enum")];
    let value_base = &definitions[find_type(&parsed, "System", "ValueType")];
    let object_base = &definitions[metadata.object_tdi];
    let mut count = 0;
    let mut widths = HashSet::new();

    for (index, ty) in definitions.as_vec().iter().enumerate() {
        if !ty.is_enum_type() {
            // Ordinary class parents, including Object's absent parent, are unchanged.
            assert_eq!(
                metadata.parent_type_index(ty),
                ty.parent_index
                    .idx_is_valid()
                    .then(|| ty.parent_index.idx())
            );
            continue;
        }
        let name = ty.full_name(&parsed, false);
        assert_eq!(
            metadata.parent_type_index(ty),
            Some(enum_base.byval_type_index.idx()),
            "{name}"
        );
        assert!(ty.is_assignable_to(enum_base, &metadata), "{name}");
        assert!(ty.is_assignable_to(value_base, &metadata), "{name}");
        assert!(ty.is_assignable_to(object_base, &metadata), "{name}");

        let backing = ty.enum_backing_type(&parsed).expect("enum backing type");
        let expected_size = match backing.ty {
            Il2CppTypeEnum::I1 | Il2CppTypeEnum::U1 => 1,
            Il2CppTypeEnum::I2 | Il2CppTypeEnum::U2 => 2,
            Il2CppTypeEnum::I4 | Il2CppTypeEnum::U4 => 4,
            Il2CppTypeEnum::I8 | Il2CppTypeEnum::U8 => 8,
            other => panic!("unexpected enum backing type for {name}: {other:?}"),
        };
        if let TypeData::TypeDefinitionIndex(backing_tdi) = backing.data {
            assert!(
                !ty.is_assignable_to(&definitions[backing_tdi], &metadata),
                "{name}"
            );
        }
        let size = offsets::get_size_info(TypeDefinitionIndex::new(index as u32), None, &metadata);
        assert_eq!(size.instance_size, expected_size, "runtime size for {name}");
        assert_eq!(
            size.calculated_instance_size, expected_size,
            "calculated size for {name}"
        );
        widths.insert(expected_size);
        count += 1;
    }
    assert!(count > 0, "fixture must contain enums");
    println!("Validated inheritance and storage for {count} enums; widths: {widths:?}");
}

#[test]
#[ignore = "requires Unity 6.3 v39 global-metadata.dat and libil2cpp.so in the working directory"]
fn v39_readonly_quaternion_parameters() {
    use crate::data::type_resolver::{ResolvedTypeData, TypeResolver};
    use crate::generate::{cs_context_collection::TypeContextCollection, cs_type::CsType};
    use brocolib::global_metadata::MethodIndex;

    let global = fs::read("global-metadata.dat").expect("read global-metadata.dat");
    let library = fs::read("libil2cpp.so").expect("read libil2cpp.so");
    let parsed = brocolib::Metadata::parse(&global, &library).expect("parse v39 metadata");
    let metadata = fixture_metadata(&parsed);
    let collection = TypeContextCollection::new();
    let resolver = TypeResolver {
        cordl_metadata: &metadata,
        collection: &collection,
    };

    for (namespace, name, wanted) in [
        ("UnityEngine", "Quaternion", "Dot"),
        ("UnityEngine", "Quaternion", "Normalize"),
        ("System", "Int32", "TryParse"),
    ] {
        let tdi = find_type(&parsed, namespace, name);
        let definition = &parsed.global_metadata.type_definitions[tdi];
        let mut ty = CsType::make_cs_type(&metadata, tdi, tdi.into()).unwrap();
        for (index, method) in definition.methods(&parsed).iter().enumerate() {
            if wanted == "TryParse"
                && (method.parameters(&parsed).len() != 2
                    || metadata.metadata_registration.types
                        [method.parameters(&parsed)[0].type_index.idx()]
                    .ty != Il2CppTypeEnum::String)
            {
                continue;
            }
            if method.name(&parsed) == wanted {
                ty.create_method(
                    MethodIndex::new(definition.method_start.index() + index as u32),
                    &resolver,
                    None,
                );
            }
        }
        assert!(!ty.methods.is_empty(), "missing {name}.{wanted}");
        for method in &ty.methods {
            let expected_return = match wanted {
                "Dot" => ResolvedTypeData::Primitive(Il2CppTypeEnum::R4),
                "Normalize" if method.instance => ResolvedTypeData::Primitive(Il2CppTypeEnum::Void),
                "Normalize" => ResolvedTypeData::Type(tdi.into()),
                "TryParse" => ResolvedTypeData::Primitive(Il2CppTypeEnum::Boolean),
                _ => unreachable!(),
            };
            assert_eq!(
                method.return_type.data, expected_return,
                "{name}.{wanted} return type"
            );
        }
        if name == "Quaternion" {
            let parameters: Vec<_> = ty
                .methods
                .iter()
                .flat_map(|method| &method.parameters)
                .collect();
            assert_eq!(parameters.len(), if wanted == "Dot" { 2 } else { 1 });
            for parameter in parameters {
                assert!(
                    matches!(parameter.il2cpp_ty.data, ResolvedTypeData::ByRefConst(_)),
                    "{name}.{wanted}: {parameter:?}"
                );
            }
        } else {
            // Int32.TryParse(string, out int) must keep a mutable out result.
            for method in &ty.methods {
                assert!(matches!(
                    method.parameters.last().unwrap().il2cpp_ty.data,
                    ResolvedTypeData::ByRef(_)
                ));
            }
        }
    }
}
