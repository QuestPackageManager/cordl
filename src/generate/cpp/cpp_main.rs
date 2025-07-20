use std::process::Command;

use brocolib::{global_metadata::TypeDefinitionIndex, runtime_metadata::TypeData};
use color_eyre::{Section, eyre::Result};
use filesize::PathExt;
use itertools::Itertools;
use log::{error, info, warn};
use rayon::iter::{ParallelBridge, ParallelIterator};
use walkdir::DirEntry;

use crate::{
    INTERNALS_DIR,
    generate::{
        cpp::{
            config::STATIC_CONFIG,
            cpp_context_collection::CppContextCollection,
            cpp_members::CppMember,
            handlers::{object, unity, value_type},
        },
        cs_context_collection::TypeContextCollection,
        metadata::CordlMetadata,
    },
};

pub fn run_cpp(
    cs_collection: TypeContextCollection,
    metadata: &CordlMetadata,
    format: bool,
) -> color_eyre::Result<()> {
    let mut cpp_context_collection =
        CppContextCollection::from_cs_collection(cs_collection, metadata, &STATIC_CONFIG);

    info!("Registering handlers!");
    // il2cpp_internals::register_il2cpp_types(&mut metadata)?;
    unity::register_unity(metadata, &mut cpp_context_collection)?;
    object::register_system(metadata, &mut cpp_context_collection)?;
    value_type::register_value_type(metadata, &mut cpp_context_collection)?;

    // let e = cpp_context_collection.cyclic_include_check()?;

    if STATIC_CONFIG.header_path.exists() {
        std::fs::remove_dir_all(&STATIC_CONFIG.header_path)?;
    }
    std::fs::create_dir_all(&STATIC_CONFIG.header_path)?;

    info!(
        "Copying config to codegen folder {:?}",
        STATIC_CONFIG.dst_internals_path
    );

    std::fs::create_dir_all(&STATIC_CONFIG.dst_internals_path)?;

    // extract contents of the cordl internals folder into destination
    INTERNALS_DIR.extract(&STATIC_CONFIG.dst_internals_path)?;

    const write_all: bool = true;
    if write_all {
        info!("Writing all");
        cpp_context_collection.write_all(&STATIC_CONFIG)?;
        cpp_context_collection.write_namespace_headers()?;
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
        cpp_context_collection
            .get()
            .iter()
            .find(|(_, c)| c.get_types().iter().any(|(_, t)| t.cpp_template.is_some()))
            .unwrap()
            .1
            .write(&STATIC_CONFIG)?;
        info!("List Generic type");
        cpp_context_collection
            .get()
            .iter()
            .find(|(_, c)| {
                c.get_types().iter().any(|(_, t)| {
                    t.cpp_name_components.generics.is_some() && t.cpp_name() == "List_1"
                })
            })
            .unwrap()
            .1
            .write(&STATIC_CONFIG)?;
        info!("Value type");
        cpp_context_collection
            .get()
            .iter()
            .find(|(_, c)| {
                c.get_types().iter().any(|(_, t)| {
                    t.is_value_type && t.name() == "Color" && t.namespace() == "UnityEngine"
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
        cpp_context_collection
            .get()
            .iter()
            .find(|(_, c)| {
                c.get_types()
                    .iter()
                    .any(|(_, t)| t.name() == "Array" && t.namespace() == "System")
            })
            .unwrap()
            .1
            .write(&STATIC_CONFIG)?;
        info!("Default param");
        cpp_context_collection
            .get()
            .iter()
            .filter(|(_, c)| {
                c.get_types().iter().any(|(_, t)| {
                    t.implementations.iter().any(|d| {
                        if let CppMember::MethodImpl(m) = d.as_ref() {
                            m.parameters.iter().any(|p| p.def_value.is_some())
                        } else {
                            false
                        }
                    })
                })
            })
            .nth(2)
            .unwrap()
            .1
            .write(&STATIC_CONFIG)?;
        info!("Enum type");
        cpp_context_collection
            .get()
            .iter()
            .find(|(_, c)| c.get_types().iter().any(|(_, t)| t.is_enum_type))
            .unwrap()
            .1
            .write(&STATIC_CONFIG)?;
        info!("UnityEngine.Object");
        cpp_context_collection
            .get()
            .iter()
            .find(|(_, c)| {
                c.get_types()
                    .iter()
                    .any(|(_, t)| t.name() == "Object" && t.namespace() == "UnityEngine")
            })
            .unwrap()
            .1
            .write(&STATIC_CONFIG)?;
        info!("BeatmapSaveDataHelpers");
        cpp_context_collection
            .get()
            .iter()
            .find(|(_, c)| {
                c.get_types()
                    .iter()
                    .any(|(_, t)| t.name() == "BeatmapSaveDataHelpers")
            })
            .unwrap()
            .1
            .write(&STATIC_CONFIG)?;
        info!("HMUI.ViewController");
        cpp_context_collection
            .get()
            .iter()
            .find(|(_, c)| {
                c.get_types()
                    .iter()
                    .any(|(_, t)| t.namespace() == "HMUI" && t.name() == "ViewController")
            })
            .unwrap()
            .1
            .write(&STATIC_CONFIG)?;
        info!("UnityEngine.Component");
        cpp_context_collection
            .get()
            .iter()
            .find(|(_, c)| {
                c.get_types()
                    .iter()
                    .any(|(_, t)| t.namespace() == "UnityEngine" && t.name() == "Component")
            })
            .unwrap()
            .1
            .write(&STATIC_CONFIG)?;
        info!("UnityEngine.GameObject");
        cpp_context_collection
            .get()
            .iter()
            .find(|(_, c)| {
                c.get_types()
                    .iter()
                    .any(|(_, t)| t.namespace() == "UnityEngine" && t.name() == "GameObject")
            })
            .unwrap()
            .1
            .write(&STATIC_CONFIG)?;
        info!("MainFlowCoordinator");
        cpp_context_collection
            .get()
            .iter()
            .find(|(_, c)| {
                c.get_types()
                    .iter()
                    .any(|(_, t)| t.namespace().is_empty() && t.name() == "MainFlowCoordinator")
            })
            .unwrap()
            .1
            .write(&STATIC_CONFIG)?;
        info!("OVRPlugin");
        cpp_context_collection
            .get()
            .iter()
            .find(|(_, c)| {
                c.get_types()
                    .iter()
                    .any(|(_, t)| t.namespace().is_empty() && t.name() == "OVRPlugin")
            })
            .unwrap()
            .1
            .write(&STATIC_CONFIG)?;
        info!("HMUI.IValueChanger");
        cpp_context_collection
            .get()
            .iter()
            .find(|(_, c)| {
                c.get_types()
                    .iter()
                    .any(|(_, t)| t.namespace() == "HMUI" && t.name() == "IValueChanger`1")
            })
            .unwrap()
            .1
            .write(&STATIC_CONFIG)?;
        info!("System.ValueType");
        cpp_context_collection
            .get()
            .iter()
            .find(|(_, c)| {
                c.get_types()
                    .iter()
                    .any(|(_, t)| t.namespace() == "System" && t.name() == "ValueType")
            })
            .unwrap()
            .1
            .write(&STATIC_CONFIG)?;
        info!("System.ValueTuple_2");
        cpp_context_collection
            .get()
            .iter()
            .find(|(_, c)| {
                c.get_types()
                    .iter()
                    .any(|(_, t)| t.namespace() == "System" && t.name() == "ValueTuple`2")
            })
            .unwrap()
            .1
            .write(&STATIC_CONFIG)?;
        info!("System.Decimal");
        cpp_context_collection
            .get()
            .iter()
            .find(|(_, c)| {
                c.get_types()
                    .iter()
                    .any(|(_, t)| t.namespace() == "System" && t.name() == "Decimal")
            })
            .unwrap()
            .1
            .write(&STATIC_CONFIG)?;
        info!("System.Enum");
        cpp_context_collection
            .get()
            .iter()
            .find(|(_, c)| {
                c.get_types()
                    .iter()
                    .any(|(_, t)| t.namespace() == "System" && t.name() == "Enum")
            })
            .unwrap()
            .1
            .write(&STATIC_CONFIG)?;
        info!("System.Multicast");
        cpp_context_collection
            .get()
            .iter()
            .find(|(_, c)| {
                c.get_types()
                    .iter()
                    .any(|(_, t)| t.namespace() == "System" && t.name() == "MulticastDelegate")
            })
            .unwrap()
            .1
            .write(&STATIC_CONFIG)?;
        info!("System.Delegate");
        cpp_context_collection
            .get()
            .iter()
            .find(|(_, c)| {
                c.get_types()
                    .iter()
                    .any(|(_, t)| t.namespace() == "System" && t.name() == "Delegate")
            })
            .unwrap()
            .1
            .write(&STATIC_CONFIG)?;
        info!("BeatmapSaveDataVersion3.BeatmapSaveData.EventBoxGroup`1");
        cpp_context_collection
            .get()
            .iter()
            .find(|(_, c)| {
                c.get_types()
                    .iter()
                    .any(|(_, t)| t.name().contains("EventBoxGroup`1"))
            })
            .unwrap()
            .1
            .write(&STATIC_CONFIG)?;
        // for (_, context) in cpp_context_collection.get() {
        //     context.write().unwrap();
        // }
    }

    if format {
        format_files()?;
    }

    Ok(())
}

fn format_files() -> color_eyre::Result<()> {
    info!("Formatting!");

    use walkdir::WalkDir;

    let files: Vec<DirEntry> = WalkDir::new(&STATIC_CONFIG.header_path)
        .into_iter()
        .filter(|f| f.as_ref().is_ok_and(|f| f.path().is_file()))
        .try_collect()?;

    let file_count = files.len();

    info!(
        "{file_count} files across {} threads",
        rayon::current_num_threads()
    );
    // easily get file size for a given file
    fn file_size(file: &DirEntry) -> usize {
        match std::fs::metadata(file.path()) {
            Ok(data) => file.path().size_on_disk_fast(&data).unwrap() as usize,
            Err(_) => 0,
        }
    }

    // TODO: Debug
    warn!("Do not run with debugger, for some reason an early abrupt exit.");

    files
        .iter()
        // sort on file size
        .sorted_by(|a, b| file_size(a).cmp(&file_size(b)))
        // reverse to go big -> small, so we can work on other files while big files are happening
        .rev()
        // parallelism
        .enumerate()
        .par_bridge()
        .try_for_each(|(file_num, file)| -> Result<()> {
            let path = file.path();
            info!(
                "Formatting [{}/{file_count}] {}",
                file_num + 1,
                path.display()
            );
            let mut command = Command::new("clang-format");
            command.arg("-i").arg(path);

            let spawn = command
                .output()
                .suggestion("You may be missing clang-format. Ensure it is on PATH")?;

            if !spawn.stderr.is_empty() {
                error!(
                    "Error {} {}",
                    path.display(),
                    String::from_utf8(spawn.stderr)?
                );
            }

            spawn.status.exit_ok()?;

            Ok(())
        })?;

    info!("Done formatting!");
    Ok(())
}
