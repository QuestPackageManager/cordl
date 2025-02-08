#![feature(entry_insert)]
#![feature(let_chains)]
#![feature(slice_as_chunks)]
#![feature(read_buf)]
#![feature(map_try_insert)]
#![feature(lazy_cell)]
#![feature(exit_status_error)]
#![feature(iterator_try_collect)]

#[cfg(feature = "il2cpp_v31")]
extern crate brocolib_il2cpp_v31 as brocolib;

#[cfg(feature = "il2cpp_v29")]
extern crate brocolib_il2cpp_v29 as brocolib;

use brocolib::{global_metadata::TypeDefinitionIndex, runtime_metadata::TypeData};
use byteorder::LittleEndian;
use color_eyre::eyre::Context;
use generate::metadata::CordlMetadata;
use itertools::Itertools;
extern crate pretty_env_logger;

use include_dir::{include_dir, Dir};
use log::{info, trace, warn};
use rayon::prelude::*;

use std::{
    fs,
    path::{Path, PathBuf},
    time,
};

use clap::{Parser, Subcommand};

use crate::generate::{cs_context_collection::TypeContextCollection, cs_type_tag::CsTypeTag};
mod data;
mod generate;
// mod handlers;
mod helpers;

#[derive(Clone, Copy, Debug, clap::ValueEnum)]
enum TargetLang {
    #[cfg(feature = "cpp")]
    Cpp,
    #[cfg(feature = "json")]
    SingleJSON,
    #[cfg(feature = "json")]
    MultiJSON,
    #[cfg(feature = "rust")]
    Rust,
}

#[derive(Parser)]
#[clap(author, version, about, long_about = None)]
struct Cli {
    /// The global-metadata.dat file to use
    #[clap(short, long, value_parser, value_name = "FILE")]
    metadata: PathBuf,

    /// The libil2cpp.so file to use
    #[clap(short, long, value_parser, value_name = "FILE")]
    libil2cpp: PathBuf,

    /// Whether to format
    #[clap(short, long)]
    format: bool,

    #[clap(short, long)]
    remove_verbose_comments: bool,

    #[clap(value_parser)]
    target: TargetLang,

    /// Whether to generate generic method specializations
    #[clap(short, long)]
    gen_generic_methods_specializations: bool,

    #[clap(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {}

static INTERNALS_DIR: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/cordl_internals");

pub type Endian = LittleEndian;

fn main() -> color_eyre::Result<()> {
    color_eyre::install()?;
    let cli: Cli = Cli::parse();
    pretty_env_logger::formatted_builder()
        .filter_level(log::LevelFilter::Trace)
        .parse_default_env()
        .init();
    if !cli.format {
        info!("Add --format/-f to format with clang-format at end")
    }

    println!(
        "Running on {}",
        Path::new("./").canonicalize().unwrap().display()
    );
    let global_metadata_data = fs::read(&cli.metadata)
        .with_context(|| format!("il2cpp metadata not found {}", cli.metadata.display()))?;
    let elf_data = fs::read(&cli.libil2cpp).with_context(|| {
        format!(
            "libil2cpp.so shared object not found {}",
            cli.metadata.display()
        )
    })?;
    let il2cpp_metadata = brocolib::Metadata::parse(&global_metadata_data, &elf_data)?;

    let get_tdi = |full_name: &str| {
        let tdi = il2cpp_metadata
            .global_metadata
            .type_definitions
            .as_vec()
            .iter()
            .position(|t| t.full_name(&il2cpp_metadata, false) == full_name)
            .unwrap_or_else(|| panic!("Unable to find TDI for {full_name}"));

        TypeDefinitionIndex::new(tdi as u32)
    };

    let unity_object_tdi_idx = get_tdi("UnityEngine.Object");
    let object_tdi_idx = get_tdi("System.Object");
    let str_tdi_idx = get_tdi("System.String");

    let mut metadata = CordlMetadata {
        metadata: &il2cpp_metadata,
        code_registration: &il2cpp_metadata.runtime_metadata.code_registration,
        metadata_registration: &il2cpp_metadata.runtime_metadata.metadata_registration,
        method_calculations: Default::default(),
        parent_to_child_map: Default::default(),
        child_to_parent_map: Default::default(),

        unity_object_tdi: unity_object_tdi_idx,
        object_tdi: object_tdi_idx,
        string_tdi: str_tdi_idx,

        name_to_tdi: Default::default(),
        blacklisted_types: Default::default(),
        pointer_size: generate::metadata::PointerSize::Bytes8,
        // For most il2cpp versions
        packing_field_offset: 7,
        size_is_default_offset: 12,
        specified_packing_field_offset: 13,
        packing_is_default_offset: 11,
    };
    let t = time::Instant::now();
    info!("Parsing metadata methods");
    metadata.parse();
    info!("Finished in {}ms", t.elapsed().as_millis());

    let mut cs_context_collection = TypeContextCollection::new();

    // blacklist types
    {
        let mut blacklist_type = |full_name: &str| {
            let tdi = metadata
                .metadata
                .global_metadata
                .type_definitions
                .as_vec()
                .iter()
                .enumerate()
                .find(|(_, t)| t.full_name(metadata.metadata, false) == full_name);

            if let Some((tdi, _td)) = tdi {
                info!("Blacklisted {full_name}");

                metadata
                    .blacklisted_types
                    .insert(TypeDefinitionIndex::new(tdi as u32));
            } else {
                warn!("Unable to blacklist {full_name}")
            }
        };

        blacklist_type("UnityEngine.XR.XRInputSubsystemDescriptor");
        blacklist_type("UnityEngine.XR.XRMeshSubsystemDescriptor");
        blacklist_type("UnityEngine.XR.XRDisplaySubsystem");
        blacklist_type("UIToolkitUtilities.Controls.Table"); // TODO: Make System.Enum work properly
                                                             // blacklist_type("NetworkPacketSerializer`2::<>c__DisplayClass4_0`1");
                                                             // blacklist_type("NetworkPacketSerializer`2::<>c__DisplayClass8_0`1");
                                                             // blacklist_type("NetworkPacketSerializer`2::<>c__DisplayClass7_0`1");
                                                             // blacklist_type("NetworkPacketSerializer`2::<>c__DisplayClass5_0`1");
                                                             // blacklist_type("NetworkPacketSerializer`2::<>c__DisplayClass10_0");
                                                             // blacklist_type("NetworkPacketSerializer`2::<>c__6`1");
                                                             // blacklist_type("RpcHandler`1::<>c__DisplayClass14_0`5");
                                                             // blacklist_type("RpcHandler`1::<>c__DisplayClass10_0`1");
                                                             // blacklist_type("RpcHandler`1::<>c__DisplayClass11_0`2");
                                                             // blacklist_type("RpcHandler`1::<>c__DisplayClass12_0`3");
                                                             // blacklist_type("RpcHandler`1::<>c__DisplayClass13_0`4");
                                                             // blacklist_type("RpcHandler`1::<>c__DisplayClass14_0`5");
                                                             // blacklist_type("RpcHandler`1::<>c__DisplayClass15_0`1");
                                                             // blacklist_type("RpcHandler`1::<>c__DisplayClass16_0`2");
                                                             // blacklist_type("RpcHandler`1::<>c__DisplayClass17_0`3");
                                                             // blacklist_type("RpcHandler`1::<>c__DisplayClass18_0`4");
                                                             // blacklist_type("RpcHandler`1::<>c__DisplayClass19_0`5");

        // Incorrect offsets / sizes due to il2cpp bug
        blacklist_type("UnityEngine.InputSystem.InputInteractionContext");
        blacklist_type("UnityEngine.InputSystem.IInputInteraction");
        blacklist_type("UnityEngine.InputSystem.LowLevel.ActionEvent");
        blacklist_type("UnityEngine.InputSystem.Interactions.HoldInteraction");
        blacklist_type("UnityEngine.InputSystem.Interactions.MultiTapInteraction");
        blacklist_type("UnityEngine.InputSystem.Interactions.PressInteraction");
        blacklist_type("UnityEngine.InputSystem.Interactions.TapInteraction");
        blacklist_type("UnityEngine.InputSystem.Interactions.SlowTapInteraction");
        blacklist_type("UnityEngine.InputSystem.LowLevel.UseWindowsGamingInputCommand");
        blacklist_type("UnityEngine.InputSystem.LowLevel.EnableIMECompositionCommand");
        blacklist_type("UnityEngine.InputSystem.LowLevel.MouseState");
        blacklist_type("UnityEngine.InputSystem.LowLevel.QueryCanRunInBackground");
        blacklist_type("UnityEngine.InputSystem.LowLevel.QueryEnabledStateCommand");
        blacklist_type("UnityEngine.InputSystem.Utilities.InputActionTrace");
        blacklist_type("UnityEngine.InputSystem.Utilities.InputActionTrace::ActionEventPtr");
        blacklist_type("UnityEngine.InputSystem.Utilities.InputActionTrace::Enumerator");
        blacklist_type("System.MonoLimitationAttribute");
    }
    {
        let _blacklist_types = |full_name: &str| {
            let tdis = metadata
                .metadata
                .global_metadata
                .type_definitions
                .as_vec()
                .iter()
                .enumerate()
                .filter(|(_, t)| t.full_name(metadata.metadata, false).contains(full_name))
                .collect_vec();

            match tdis.is_empty() {
                true => warn!("Unable to blacklist {full_name}"),
                false => {
                    for (tdi, td) in tdis {
                        info!("Blacklisted {}", td.full_name(metadata.metadata, true));

                        metadata
                            .blacklisted_types
                            .insert(TypeDefinitionIndex::new(tdi as u32));
                    }
                }
            }
        };
        // blacklist_types("<>c__DisplayClass");
    }
    {
        // First, make all the contexts
        info!("Making types");
        let type_defs = metadata.metadata.global_metadata.type_definitions.as_vec();
        let total = type_defs.len();
        for tdi_u64 in 0..total {
            let tdi = TypeDefinitionIndex::new(tdi_u64 as u32);

            let ty_def = &metadata.metadata.global_metadata.type_definitions[tdi];
            let _ty = &metadata.metadata_registration.types[ty_def.byval_type_index as usize];

            // only make the roots
            if ty_def.declaring_type_index != u32::MAX {
                continue;
            }

            trace!(
                "Making types {:.4}% ({tdi_u64}/{total})",
                (tdi_u64 as f64 / total as f64 * 100.0)
            );
            cs_context_collection.make_from(&metadata, TypeData::TypeDefinitionIndex(tdi), None);
            cs_context_collection.alias_nested_types_il2cpp(
                tdi,
                CsTypeTag::TypeDefinitionIndex(tdi),
                &metadata,
            );
        }
    }
    {
        // First, make all the contexts
        info!("Making nested types");
        let type_defs = metadata.metadata.global_metadata.type_definitions.as_vec();
        let total = type_defs.len();
        for tdi_u64 in 0..total {
            let tdi = TypeDefinitionIndex::new(tdi_u64 as u32);

            let ty_def = &metadata.metadata.global_metadata.type_definitions[tdi];

            if ty_def.declaring_type_index == u32::MAX {
                continue;
            }

            trace!(
                "Making nested types {:.4}% ({tdi_u64}/{total})",
                (tdi_u64 as f64 / total as f64 * 100.0)
            );
            cs_context_collection.make_nested_from(&metadata, tdi);
        }
    }

    // {
    //     let total = metadata.metadata_registration.generic_method_table.len() as f64;
    //     info!("Making generic type instantiations");
    //     for (i, generic_class) in metadata
    //         .metadata_registration
    //         .generic_method_table
    //         .iter()
    //         .enumerate()
    //     {
    //         trace!(
    //             "Making generic type instantiations {:.4}% ({i}/{total})",
    //             (i as f64 / total * 100.0)
    //         );
    //         let method_spec = metadata
    //             .metadata_registration
    //             .method_specs
    //             .get(generic_class.generic_method_index as usize)
    //             .unwrap();

    //         cpp_context_collection.make_generic_from(method_spec, &mut metadata, &STATIC_CONFIG);
    //     }
    // }
    // {
    //     let total = metadata.metadata_registration.generic_method_table.len() as f64;
    //     info!("Filling generic types!");
    //     for (i, generic_class) in metadata
    //         .metadata_registration
    //         .generic_method_table
    //         .iter()
    //         .enumerate()
    //     {
    //         trace!(
    //             "Filling generic type instantiations {:.4}% ({i}/{total})",
    //             (i as f64 / total * 100.0)
    //         );
    //         let method_spec = metadata
    //             .metadata_registration
    //             .method_specs
    //             .get(generic_class.generic_method_index as usize)
    //             .unwrap();

    //         cpp_context_collection.fill_generic_class_inst(
    //             method_spec,
    //             &mut metadata,
    //
    //         );
    //     }
    // }

    if cli.gen_generic_methods_specializations {
        let total = metadata.metadata_registration.generic_method_table.len() as f64;
        info!("Filling generic methods!");
        for (i, generic_class) in metadata
            .metadata_registration
            .generic_method_table
            .iter()
            .enumerate()
        {
            trace!(
                "Filling generic method instantiations {:.4}% ({i}/{total})",
                (i as f64 / total * 100.0)
            );
            let method_spec = metadata
                .metadata_registration
                .method_specs
                .get(generic_class.generic_method_index as usize)
                .unwrap();

            cs_context_collection.fill_generic_method_inst(method_spec, &mut metadata);
        }
    }

    info!("Registering handlers!");
    // il2cpp_internals::register_il2cpp_types(&mut metadata)?;

    // TODO: uncomment
    // unity::register_unity(&mut metadata)?;
    // object::register_system(&mut metadata)?;
    // value_type::register_value_type(&mut metadata)?;
    info!("Handlers registered!");

    {
        // Fill them now
        info!("Filling types");
        let type_defs = metadata.metadata.global_metadata.type_definitions.as_vec();
        let total = type_defs.len();
        for tdi_u64 in 0..total {
            let tdi = TypeDefinitionIndex::new(tdi_u64 as u32);

            trace!(
                "Filling type {:.4} ({tdi_u64}/{total})",
                (tdi_u64 as f64 / total as f64 * 100.0)
            );

            cs_context_collection.fill(CsTypeTag::TypeDefinitionIndex(tdi), &metadata);
        }
    }

    if cli.remove_verbose_comments {
        // TODO: uncomment
        // remove_coments(&mut cpp_context_collection)?;
    }

    match cli.target {
        #[cfg(feature = "cpp")]
        TargetLang::Cpp => {
            use generate::cpp;

            cpp::cpp_main::run_cpp(cs_context_collection, &metadata, cli.format)?;
            Ok(())
        }
        #[cfg(feature = "json")]
        TargetLang::SingleJSON => {
            use generate::json;

            let json = Path::new("./json");
            println!("Writing json file {json:?}");
            json::make_json(&metadata, &cs_context_collection, json, cli.format)?;
            Ok(())
        }
        #[cfg(feature = "json")]
        TargetLang::MultiJSON => {
            use generate::json;

            let json_folder = Path::new("./multi_json");

            println!("Writing json file {json_folder:?}");
            json::make_json_folder(&metadata, &cs_context_collection, json_folder)?;
            Ok(())
        }

        #[cfg(feature = "rust")]
        TargetLang::Rust => {
            use generate::rust;
            rust::rust_main::run_rust(cs_context_collection, &metadata)?;

            Ok(())
        }
        _ => color_eyre::Result::<()>::Ok(()),
    }?;

    Ok(())
}
