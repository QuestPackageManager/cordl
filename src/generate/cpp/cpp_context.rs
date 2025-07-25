use std::cmp::Ordering;
use std::io::{BufWriter, Write};
use std::{
    collections::{HashMap, HashSet},
    fs::{File, create_dir_all, remove_file},
    path::PathBuf,
};

use color_eyre::eyre::ContextCompat;

use itertools::Itertools;
use log::trace;
use pathdiff::diff_paths;

use crate::generate::context::TypeContext;
use crate::generate::cpp::config::STATIC_CONFIG;
use crate::generate::cpp::cpp_members::{CppForwardDeclare, CppInclude};
use crate::generate::cpp::cpp_type::CORDL_NO_INCLUDE_IMPL_DEFINE;

use crate::generate::cs_type_tag::CsTypeTag;
use crate::generate::metadata::CordlMetadata;
use crate::generate::type_extensions::TypeDefinitionExtensions;
use crate::generate::writer::{Writable, Writer};
use crate::helpers::sorting::DependencyGraph;

use super::config::CppGenerationConfig;
use super::cpp_members::CppUsingAlias;
use super::cpp_name_resolver::{IL2CPP_OBJECT_TYPE, VALUE_WRAPPER_TYPE};
use super::cpp_type::CppType;

// Holds the contextual information for creating a C++ file
// Will hold various metadata, such as includes, type definitions, and extraneous writes
#[derive(Debug, Clone)]
pub struct CppContext {
    pub typedef_path: PathBuf,
    pub type_impl_path: PathBuf,

    // combined header
    pub fundamental_path: PathBuf,

    // Types to write, typedef
    pub typedef_types: HashMap<CsTypeTag, CppType>,

    // Namespace -> alias
    pub typealias_types: HashSet<(String, CppUsingAlias)>,
}

/// `CppContext` provides methods to manage and generate C++ type definitions and implementations
/// based on the provided metadata and configuration.
///
/// # Methods
///
/// - `get_cpp_type_recursive_mut`: Retrieves a mutable reference to a C++ type based on the given root tag.
/// - `get_cpp_type_recursive`: Retrieves an immutable reference to a C++ type based on the given root tag.
/// - `get_include_path`: Returns the include path for the C++ type definitions.
/// - `get_types`: Returns an immutable reference to the map of C++ types.
/// - `get_types_mut`: Returns a mutable reference to the map of C++ types.
/// - `make`: Creates a new `CppContext` instance based on the provided context tag, type context, metadata, and configuration.
/// - `insert_cpp_type`: Inserts a new C++ type into the context.
/// - `write`: Writes the C++ type definitions and implementations to the appropriate files.
///
/// # Example
///
/// ```rust
/// let cpp_context = CppContext::make(context_tag, &type_context, &metadata, &config);
/// cpp_context.write(&config)?;
/// ```
///
/// # Errors
///
/// The `write` method can return an error if file operations fail, such as creating or removing files and directories.
///
/// # Internal Functions
///
/// - `write_il2cpp_arg_macros`: Writes IL2CPP argument macros for the given C++ type.
impl CppContext {
    /// Returns the include path for the C++ type definitions.
    pub fn get_include_path(&self) -> &PathBuf {
        &self.typedef_path
    }

    /// Returns an immutable reference to the map of C++ types.
    pub fn get_types(&self) -> &HashMap<CsTypeTag, CppType> {
        &self.typedef_types
    }

    /// Returns a mutable reference to the map of C++ types.
    pub fn get_types_mut(&mut self) -> &mut HashMap<CsTypeTag, CppType> {
        &mut self.typedef_types
    }

    /// Creates a new `CppContext` instance based on the provided context tag, type context, metadata, and configuration.
    pub fn make(
        context_tag: CsTypeTag,
        context: &TypeContext,
        metadata: &CordlMetadata,
        config: &CppGenerationConfig,
    ) -> CppContext {
        let tdi = context_tag.get_tdi();
        let t = &metadata.metadata.global_metadata.type_definitions[tdi];

        let components = t.get_name_components(metadata.metadata);

        let ns = &components.namespace.unwrap_or_default();
        let name = &components.name;

        let ns_path = config.namespace_path(ns);
        let path = if ns_path.is_empty() {
            "GlobalNamespace/".to_string()
        } else {
            ns_path + "/"
        };
        let path_name = match t.declaring_type_index != u32::MAX {
            true => {
                let name = config.path_name(name);
                let base_name = components.declaring_types.unwrap_or_default().join("_");

                format!("{base_name}_{name}")
            }
            false => config.path_name(name),
        };

        let mut x = CppContext {
            typedef_path: config
                .header_path
                .join(format!("{path}zzzz__{path_name}_def.hpp")),
            type_impl_path: config
                .header_path
                .join(format!("{path}zzzz__{path_name}_impl.hpp")),
            fundamental_path: config.header_path.join(format!("{path}{path_name}.hpp")),
            typedef_types: Default::default(),
            typealias_types: Default::default(),
        };

        for (tag, ty) in &context.typedef_types {
            let tdi = tag.get_tdi();

            let mut cpp_ty = CppType::make_cpp_type(*tag, ty, config);
            cpp_ty.nested_fixup(context_tag, ty, metadata, config);

            if metadata.blacklisted_types.contains(&tdi) {
                let result = match t.is_value_type() {
                    true => format!(
                        "{VALUE_WRAPPER_TYPE}<{:x}>",
                        ty.size_info.as_ref().unwrap().instance_size
                    ),
                    false => IL2CPP_OBJECT_TYPE.to_string(),
                };

                if !t.is_value_type() {
                    x.typealias_types
                        .insert((cpp_ty.cpp_namespace(), CppUsingAlias {
                            alias: cpp_ty.name().to_string(),
                            result,
                            template: Default::default(),
                        }));
                    continue;
                }
            }

            x.typedef_types.insert(*tag, cpp_ty);
        }

        x
    }

    /// Inserts a new C++ type into the context.
    pub fn insert_cpp_type(&mut self, cpp_type: CppType) {
        self.typedef_types.insert(cpp_type.self_tag, cpp_type);
    }

    /// Writes the C++ type definitions and implementations to the appropriate files.
    pub fn write(&self, config: &CppGenerationConfig) -> color_eyre::Result<()> {
        // Write typedef file first
        if self.typedef_path.exists() {
            remove_file(self.typedef_path.as_path())?;
        }
        if !self
            .typedef_path
            .parent()
            .context("parent is not a directory!")?
            .is_dir()
        {
            // Assume it's never a file
            create_dir_all(
                self.typedef_path
                    .parent()
                    .context("Failed to create all directories!")?,
            )?;
        }

        let base_path = &config.header_path;

        trace!("Writing {:?}", self.typedef_path.as_path());
        let mut typedef_writer = Writer {
            stream: BufWriter::new(File::create(self.typedef_path.as_path())?),
            indent: 0,
            newline: true,
        };
        let mut typeimpl_writer = Writer {
            stream: BufWriter::new(File::create(self.type_impl_path.as_path())?),
            indent: 0,
            newline: true,
        };
        let mut fundamental_writer = Writer {
            stream: BufWriter::new(File::create(self.fundamental_path.as_path())?),
            indent: 0,
            newline: true,
        };

        writeln!(typedef_writer, "#pragma once")?;
        writeln!(typeimpl_writer, "#pragma once")?;
        writeln!(fundamental_writer, "#pragma once")?;

        // add IWYU
        let typedef_include_path = diff_paths(&self.typedef_path, base_path)
            .context("Failed to get typedef include path")?;
        let _typeimpl_include_path = diff_paths(&self.type_impl_path, base_path)
            .context("Failed to get typeimpl include path")?;
        let fundamental_include_path = diff_paths(&self.fundamental_path, base_path)
            .context("Failed to get fundamental include path")?;

        let fundamental_include_pragma = format!(
            "// IWYU pragma private; include \"{}\"",
            fundamental_include_path.display()
        );
        writeln!(typedef_writer, "{fundamental_include_pragma}")?;
        writeln!(typeimpl_writer, "{fundamental_include_pragma}")?;
        writeln!(fundamental_writer, "// IWYU pragma: begin_exports")?;

        // Include cordl config
        // this is so confusing but basically gets the relative folder
        // navigation for `_config.hpp`
        let dest_path = diff_paths(
            &STATIC_CONFIG.dst_header_internals_file,
            self.typedef_path.parent().unwrap(),
        )
        .unwrap();

        // write typedefs.h include first - this makes include order mostly happy (probably System.Object would still be weird!)
        CppInclude::new_exact("beatsaber-hook/shared/utils/typedefs.h")
            .write(&mut typedef_writer)?;
        CppInclude::new_exact(dest_path).write(&mut typedef_writer)?;

        // after including cordl internals
        // macro module init
        writeln!(typedef_writer, "CORDL_MODULE_INIT")?;

        // alphabetical sorted
        let typedef_types = self
            .typedef_types
            .values()
            .sorted_by(|a, b| a.cpp_name_components.cmp(&b.cpp_name_components))
            // Enums go after stubs
            .sorted_by(|a, b| {
                if a.is_enum_type == b.is_enum_type {
                    return Ordering::Equal;
                }

                if a.is_enum_type {
                    Ordering::Less
                } else if b.is_enum_type {
                    Ordering::Greater
                } else {
                    Ordering::Equal
                }
            })
            // Value types are last
            .sorted_by(|a, b| {
                let a_strictly_vt = a.is_value_type && !a.is_enum_type;
                let b_strictly_vt = b.is_value_type && !b.is_enum_type;

                if a_strictly_vt == b_strictly_vt {
                    return Ordering::Equal;
                }

                if a_strictly_vt {
                    Ordering::Greater
                } else if b_strictly_vt {
                    Ordering::Less
                } else {
                    Ordering::Equal
                }
            })
            .collect_vec();

        let typedef_root_types = typedef_types
            .iter()
            .filter(|t| self.typedef_types.contains_key(&t.self_tag))
            .collect_vec();

        let mut ts = DependencyGraph::<CsTypeTag, _>::new(|a, b| a.cmp(b));
        for cpp_type in &typedef_root_types {
            ts.add_root_dependency(&cpp_type.self_tag);

            for dep in cpp_type.requirements.depending_types.iter().sorted() {
                ts.add_dependency(&cpp_type.self_tag, dep);

                // add dependency for generic instantiations
                // for all types with the same TDI
                if let CsTypeTag::TypeDefinitionIndex(tdi) = dep {
                    // find all generic tags that have the same TDI
                    let generic_tags_in_context =
                        typedef_root_types.iter().filter(|t| match t.self_tag {
                            CsTypeTag::TypeDefinitionIndex(_) => false,
                            CsTypeTag::GenericInstantiation(gen_inst) => gen_inst.tdi == *tdi,
                        });

                    generic_tags_in_context.for_each(|generic_dep| {
                        ts.add_dependency(&cpp_type.self_tag, &generic_dep.self_tag);
                    })
                }
            }
        }

        // types that don't depend on anyone
        // we take these because they get undeterministically sorted
        // and can be first anyways
        let mut undepended_cpp_types = vec![];

        // currently sorted from root to dependencies
        // aka least depended to most depended
        let mut typedef_root_types_sorted = ts
            .topological_sort()
            .into_iter()
            .filter_map(|t| self.typedef_types.get(t))
            .collect_vec();

        // add the items with no dependencies at the tail
        // when reversed these will be first and can be allowed to be first
        typedef_root_types_sorted.append(&mut undepended_cpp_types);
        // typedef_root_types_sorted.reverse();

        // Write includes for typedef
        typedef_types
            .iter()
            .flat_map(|t| &t.requirements.required_def_includes)
            .unique()
            .sorted()
            .try_for_each(|i| i.write(&mut typedef_writer))?;

        // Write includes for typeimpl
        typedef_types
            .iter()
            .flat_map(|t| &t.requirements.required_impl_includes)
            .unique()
            .sorted()
            .try_for_each(|i| i.write(&mut typeimpl_writer))?;

        // add module declarations
        writeln!(
            typedef_writer,
            "CORDL_MODULE_EXPORT({})",
            self.fundamental_path.file_stem().unwrap().to_string_lossy()
        )?;

        // anonymous namespace
        if STATIC_CONFIG.use_anonymous_namespace {
            writeln!(typedef_writer, "CORDL_MODULE_EXPORT_STRUCT namespace {{")?;
            writeln!(typeimpl_writer, "CORDL_MODULE_EXPORT_STRUCT namespace {{")?;
        }

        // write forward declares
        // and includes for impl
        {
            CppInclude::new_exact(&typedef_include_path).write(&mut typeimpl_writer)?;

            let forward_declare_and_includes = || {
                typedef_types
                    .iter()
                    .flat_map(|t| &t.requirements.forward_declares)
            };

            forward_declare_and_includes()
                .map(|(_fd, inc)| inc)
                .unique()
                .sorted()
                // TODO: Check forward declare is not of own type
                .try_for_each(|i| -> color_eyre::Result<()> {
                    i.write(&mut typeimpl_writer)?;
                    Ok(())
                })?;

            forward_declare_and_includes()
                .map(|(fd, _inc)| fd)
                .unique()
                .sorted_by(|a, b| {
                    let do_format = |fd: &CppForwardDeclare| {
                        format!(
                            "{}_{}_{}_{}",
                            fd.cpp_namespace.clone().unwrap_or_default(),
                            fd.cpp_name,
                            fd.literals.clone().unwrap_or_default().join(","),
                            fd.templates
                                .clone()
                                .unwrap_or_default()
                                .just_names()
                                .join(",")
                        )
                    };
                    let a_str = do_format(a);
                    let b_str = do_format(b);

                    a_str.cmp(&b_str)
                })
                .try_for_each(|fd| fd.write(&mut typedef_writer))?;

            writeln!(typedef_writer, "// Forward declare root types")?;
            //Forward declare all types
            typedef_root_types
                .iter()
                .map(|t| CppForwardDeclare::from_cpp_type(t))
                // TODO: Check forward declare is not of own type
                .try_for_each(|fd| {
                    // Forward declare and include
                    fd.write(&mut typedef_writer)
                })?;

            writeln!(typedef_writer, "// Write type traits")?;
            typedef_root_types
                .iter()
                .try_for_each(|cpp_type| -> color_eyre::Result<()> {
                    if cpp_type.generic_instantiations_args_types.is_none() {
                        cpp_type.write_type_trait(&mut typedef_writer)?;
                    }
                    Ok(())
                })?;
        }

        for t in &typedef_root_types_sorted {
            t.write_def(&mut typedef_writer)?;
            t.write_impl(&mut typeimpl_writer)?;
        }

        // end anonymous namespace
        if STATIC_CONFIG.use_anonymous_namespace {
            writeln!(typedef_writer, "}} // end anonymous namespace")?;
            writeln!(typeimpl_writer, "}} // end anonymous namespace")?;
        }

        // write macros
        typedef_types
            .iter()
            .try_for_each(|t| Self::write_il2cpp_arg_macros(t, &mut typedef_writer))?;

        // Fundamental
        {
            CppInclude::new_exact(typedef_include_path).write(&mut fundamental_writer)?;

            // if guard for intellisense
            writeln!(fundamental_writer, "#ifndef {CORDL_NO_INCLUDE_IMPL_DEFINE}")?;
            CppInclude::new_exact(diff_paths(&self.type_impl_path, base_path).unwrap())
                .write(&mut fundamental_writer)?;
            writeln!(fundamental_writer, "#endif")?;

            // end IWYU
            writeln!(fundamental_writer, "// IWYU pragma: end_exports")?;
        }

        Ok(())
    }

    /// Writes IL2CPP argument macros for the given C++ type.
    fn write_il2cpp_arg_macros(ty: &CppType, writer: &mut Writer) -> color_eyre::Result<()> {
        let is_generic_instantiation = ty.generic_instantiations_args_types.is_some();
        if is_generic_instantiation {
            return Ok(());
        }

        let template_container_type = ty
            .cpp_template
            .as_ref()
            .is_some_and(|t| !t.names.is_empty());

        if !ty.is_value_type && !template_container_type && !is_generic_instantiation {
            // reference types need no boxing
            writeln!(
                writer,
                "NEED_NO_BOX({});",
                ty.cpp_name_components
                    .clone()
                    .remove_generics()
                    .remove_pointer()
                    .combine_all()
            )?;
        }

        let macro_arg_define = {
            match //ty.generic_instantiation_args.is_some() ||
                    template_container_type {
                    true => match ty.is_value_type {
                        true => "DEFINE_IL2CPP_ARG_TYPE_GENERIC_STRUCT",
                        false => "DEFINE_IL2CPP_ARG_TYPE_GENERIC_CLASS",
                    },
                    false => "DEFINE_IL2CPP_ARG_TYPE",
                }
        };

        // Essentially splits namespace.foo/nested_foo into (namespace, foo/nested_foo)

        let namespace = ty.cs_name_components.namespace.clone().unwrap_or_default();
        let combined_name = match &ty.cs_name_components.declaring_types {
            None => ty.cs_name_components.name.clone(),
            Some(declaring_types) => format!(
                "{}/{}",
                declaring_types.join("/"),
                ty.cs_name_components.name.clone()
            ),
        };

        // generics shouldn't emit with a pointer, while regular types should honor the pointer
        let cpp_name = match template_container_type {
            true => ty
                .cpp_name_components
                .clone()
                .remove_generics()
                .remove_pointer()
                .combine_all(),

            false => ty
                .cpp_name_components
                .clone()
                .remove_generics()
                .combine_all(),
        };

        writeln!(
            writer,
            "{macro_arg_define}({cpp_name}, \"{namespace}\", \"{combined_name}\");",
        )?;

        Ok(())
    }
}
