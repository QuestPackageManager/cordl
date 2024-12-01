use brocolib::{global_metadata::TypeDefinitionIndex, runtime_metadata::TypeData};

use itertools::Itertools;
use log::info;
use rayon::iter::ParallelIterator;

use crate::generate::{
    cs_context_collection::TypeContextCollection,
    metadata::CordlMetadata,
    rust::{config::STATIC_CONFIG, rust_context_collection::RustContextCollection},
    type_extensions::{TypeDefinitionExtensions, TypeDefinitionIndexExtensions},
};

pub fn run_rust(
    cs_collection: TypeContextCollection,
    metadata: &CordlMetadata,
) -> color_eyre::Result<()> {
    let rs_context_collection =
        RustContextCollection::from_cs_collection(cs_collection, metadata, &STATIC_CONFIG);

    info!("Registering handlers!");

    // let e = cpp_context_collection.cyclic_include_check()?;

    if STATIC_CONFIG.source_path.exists() {
        std::fs::remove_dir_all(&STATIC_CONFIG.source_path)?;
    }
    std::fs::create_dir_all(&STATIC_CONFIG.source_path)?;

    const write_all: bool = false;
    if write_all {
        info!("Writing all");
        rs_context_collection.write_all(&STATIC_CONFIG)?;
        rs_context_collection.write_namespace_headers()?;
    } else {
        // for t in &metadata.type_definitions {
        //     // Handle the generation for a single type
        //     let dest = open_writer(&metadata, &config, &t);
        //     write_type(&metadata, &config, &t, &dest);
        // }
        fn make_td_tdi(idx: u32) -> TypeData {
            TypeData::TypeDefinitionIndex(TypeDefinitionIndex::new(idx))
        }
        // All indices require updating
        // cpp_context_collection.get()[&make_td_tdi(123)].write()?;
        // cpp_context_collection.get()[&make_td_tdi(342)].write()?;
        // cpp_context_collection.get()[&make_td_tdi(512)].write()?;
        // cpp_context_collection.get()[&make_td_tdi(1024)].write()?;
        // cpp_context_collection.get()[&make_td_tdi(600)].write()?;
        // cpp_context_collection.get()[&make_td_tdi(1000)].write()?;
        // cpp_context_collection.get()[&make_td_tdi(420)].write()?;
        // cpp_context_collection.get()[&make_td_tdi(69)].write()?;
        // cpp_context_collection.get()[&make_td_tdi(531)].write()?;
        // cpp_context_collection.get()[&make_td_tdi(532)].write()?;
        // cpp_context_collection.get()[&make_td_tdi(533)].write()?;
        // cpp_context_collection.get()[&make_td_tdi(534)].write()?;
        // cpp_context_collection.get()[&make_td_tdi(535)].write()?;
        // cpp_context_collection.get()[&make_td_tdi(1455)].write()?;

        info!("Generic type");
        let types = || {
            rs_context_collection
                .get()
                .iter()
                .filter(|(tag, _)| !metadata.blacklisted_types.contains(&tag.get_tdi()))
        };

        types()
            .find(|(_, c)| c.get_types().iter().any(|(_, t)| t.generics.is_some()))
            .unwrap()
            .1
            .write(&STATIC_CONFIG)?;
        info!("List Generic type");
        types()
            .find(|(_, c)| {
                c.get_types().iter().any(|(_, t)| {
                    t.rs_name_components.generics.is_some() && t.rs_name() == "List_1"
                })
            })
            .unwrap()
            .1
            .write(&STATIC_CONFIG)?;
        info!("Value type");
        types()
            .find(|(_, c)| {
                c.get_types().iter().any(|(_, t)| {
                    t.is_value_type && t.name() == "Color" && t.namespace() == Some("UnityEngine")
                })
            })
            .unwrap()
            .1
            .write(&STATIC_CONFIG)?;
        // info!("Nested type");
        // cpp_context_collection
        //     .get()
        //     .iter()
        //     .find(|(_, c)| {
        //         c.get_types().iter().any(|(_, t)| {
        //             t.nested_types
        //                 .iter()
        //                 .any(|(_, n)| !n.declarations.is_empty())
        //         })
        //     })
        //     .unwrap()
        //     .1
        //     .write()?;
        // Doesn't exist anymore?
        // info!("AlignmentUnion type");
        // cpp_context_collection
        //     .get()
        //     .iter()
        //     .find(|(_, c)| {
        //         c.get_types()
        //             .iter()
        //             .any(|(_, t)| t.is_value_type && &t.name()== "AlignmentUnion")
        //     })
        //     .unwrap()
        //     .1
        //     .write()?;
        info!("Array type");
        types()
            .find(|(_, c)| {
                c.get_types()
                    .iter()
                    .any(|(_, t)| t.name() == "Array" && t.namespace() == Some("System"))
            })
            .unwrap()
            .1
            .write(&STATIC_CONFIG)?;

        info!("Enum type");
        types()
            .find(|(_, c)| c.get_types().iter().any(|(_, t)| t.is_enum_type))
            .unwrap()
            .1
            .write(&STATIC_CONFIG)?;
        info!("UnityEngine.Object");
        types()
            .find(|(_, c)| {
                c.get_types()
                    .iter()
                    .any(|(_, t)| t.name() == "Object" && t.namespace() == Some("UnityEngine"))
            })
            .unwrap()
            .1
            .write(&STATIC_CONFIG)?;
        info!("BeatmapSaveDataHelpers");
        types()
            .find(|(_, c)| {
                c.get_types()
                    .iter()
                    .any(|(_, t)| t.name() == "BeatmapSaveDataHelpers")
            })
            .unwrap()
            .1
            .write(&STATIC_CONFIG)?;
        info!("HMUI.ViewController");
        types()
            .find(|(_, c)| {
                c.get_types()
                    .iter()
                    .any(|(_, t)| t.namespace() == Some("HMUI") && t.name() == "ViewController")
            })
            .unwrap()
            .1
            .write(&STATIC_CONFIG)?;
        info!("UnityEngine.Component");
        types()
            .find(|(_, c)| {
                c.get_types()
                    .iter()
                    .any(|(_, t)| t.namespace() == Some("UnityEngine") && t.name() == "Component")
            })
            .unwrap()
            .1
            .write(&STATIC_CONFIG)?;
        info!("UnityEngine.GameObject");
        types()
            .find(|(_, c)| {
                c.get_types()
                    .iter()
                    .any(|(_, t)| t.namespace() == Some("UnityEngine") && t.name() == "GameObject")
            })
            .unwrap()
            .1
            .write(&STATIC_CONFIG)?;
        info!("MainFlowCoordinator");
        types()
            .find(|(_, c)| {
                c.get_types()
                    .iter()
                    .any(|(_, t)| t.namespace().is_none() && t.name() == "MainFlowCoordinator")
            })
            .unwrap()
            .1
            .write(&STATIC_CONFIG)?;
        info!("OVRPlugin");
        types()
            .find(|(_, c)| {
                c.get_types()
                    .iter()
                    .any(|(_, t)| t.namespace().is_none() && t.name() == "OVRPlugin")
            })
            .unwrap()
            .1
            .write(&STATIC_CONFIG)?;
        info!("HMUI.IValueChanger");
        types()
            .find(|(_, c)| {
                c.get_types()
                    .iter()
                    .any(|(_, t)| t.namespace() == Some("HMUI") && t.name() == "IValueChanger`1")
            })
            .unwrap()
            .1
            .write(&STATIC_CONFIG)?;
        info!("System.ValueType");
        types()
            .find(|(_, c)| {
                c.get_types()
                    .iter()
                    .any(|(_, t)| t.namespace() == Some("System") && t.name() == "ValueType")
            })
            .unwrap()
            .1
            .write(&STATIC_CONFIG)?;
        info!("System.ValueTuple_2");
        types()
            .find(|(_, c)| {
                c.get_types()
                    .iter()
                    .any(|(_, t)| t.namespace() == Some("System") && t.name() == "ValueTuple`2")
            })
            .unwrap()
            .1
            .write(&STATIC_CONFIG)?;
        info!("System.Decimal");
        types()
            .find(|(_, c)| {
                c.get_types()
                    .iter()
                    .any(|(_, t)| t.namespace() == Some("System") && t.name() == "Decimal")
            })
            .unwrap()
            .1
            .write(&STATIC_CONFIG)?;
        info!("System.Enum");
        types()
            .find(|(_, c)| {
                c.get_types()
                    .iter()
                    .any(|(_, t)| t.namespace() == Some("System") && t.name() == "Enum")
            })
            .unwrap()
            .1
            .write(&STATIC_CONFIG)?;
        info!("System.Multicast");
        types()
            .find(|(_, c)| {
                c.get_types().iter().any(|(_, t)| {
                    t.namespace() == Some("System") && t.name() == "MulticastDelegate"
                })
            })
            .unwrap()
            .1
            .write(&STATIC_CONFIG)?;
        info!("System.Delegate");
        types()
            .find(|(_, c)| {
                c.get_types()
                    .iter()
                    .any(|(_, t)| t.namespace() == Some("System") && t.name() == "Delegate")
            })
            .unwrap()
            .1
            .write(&STATIC_CONFIG)?;
        info!("BeatmapSaveDataVersion3.BeatmapSaveData.EventBoxGroup`1");
        types()
            .find(|(_, c)| {
                c.get_types()
                    .iter()
                    .any(|(_, t)| t.name().contains("EventBoxGroup`1"))
            })
            .unwrap()
            .1
            .write(&STATIC_CONFIG)?;
        info!("Explicitly laid out type");
        types()
            .find(|(_, c)| {
                c.get_types().iter().any(|(_, t)| {
                    !t.is_compiler_generated
                        && t.self_tag
                            .get_tdi()
                            .get_type_definition(metadata.metadata)
                            .is_explicit_layout()
                })
            })
            .unwrap()
            .1
            .write(&STATIC_CONFIG)?;

        rs_context_collection.write_namespace_headers()?;

        // for (_, context) in cpp_context_collection.get() {
        //     context.write().unwrap();
        // }
    }

    Ok(())
}

fn format_files() -> color_eyre::Result<()> {
    todo!();
}
