use core::panic;
use std::collections::{HashMap, HashSet};

use brocolib::{
    global_metadata::TypeDefinitionIndex,
    runtime_metadata::{Il2CppMethodSpec, TypeData},
};
use itertools::Itertools;
use log::{info, warn};

use crate::generate::cs_type::CsType;

use super::{
    context::TypeContext,
    cs_type_tag::{CsTypeTag, GenericInstantiation},
    metadata::Metadata,
    type_extensions::TypeDefinitionExtensions,
};

pub struct TypeContextCollection {
    // Should always be a TypeDefinitionIndex
    all_contexts: HashMap<CsTypeTag, TypeContext>,
    pub alias_context: HashMap<CsTypeTag, CsTypeTag>,
    pub alias_nested_type_to_parent: HashMap<CsTypeTag, CsTypeTag>,
    filled_types: HashSet<CsTypeTag>,
    filling_types: HashSet<CsTypeTag>,
    borrowing_types: HashSet<CsTypeTag>,
}

impl TypeContextCollection {
    
fn fill_cpp_type(
        &mut self,
        cpp_type: &mut CsType,
        metadata: &Metadata,
    ) {
        let tag = cpp_type.self_tag;

        if self.filled_types.contains(&tag) {
            return;
        }
        if self.filling_types.contains(&tag) {
            panic!("Currently filling type {tag:?}, cannot fill")
        }

        // Move ownership to local
        self.filling_types.insert(tag);

        cpp_type.fill_from_il2cpp(metadata);

        self.filled_types.insert(tag);
        self.filling_types.remove(&tag.clone());
    }

    ///
    /// Generate the aliases for the nested types through il2cpp
    ///
    /// If nested is true, aliases all nested types to their root type
    ///
    pub fn alias_nested_types_il2cpp(
        &mut self,
        owner_tdi: TypeDefinitionIndex,
        root_tag: CsTypeTag,
        metadata: &Metadata,
        nested: bool,
    ) {
        let owner_tag = CsTypeTag::TypeDefinitionIndex(owner_tdi);
        let owner_ty = &metadata.metadata.global_metadata.type_definitions[owner_tdi];

        for nested_type_tdi in owner_ty.nested_types(metadata.metadata) {
            // let nested_type = &metadata.metadata.global_metadata.type_definitions[*nested_type_tdi];

            let nested_tag = CsTypeTag::TypeDefinitionIndex(*nested_type_tdi);

            self.alias_type_to_context(nested_tag, root_tag);
            if nested {
                self.alias_type_to_parent(nested_tag, owner_tag);
            }
            self.alias_nested_types_il2cpp(*nested_type_tdi, root_tag, metadata, nested);
        }
    }

    pub fn alias_type_to_context(&mut self, src: CsTypeTag, dest: CsTypeTag) {
        assert!(
            !self.alias_context.contains_key(&dest),
            "Aliasing an aliased type! {src:?} to {dest:?}"
        );
        assert!(
            !self.alias_context.contains_key(&src),
            "Already aliased this key!"
        );
        assert!(
            self.all_contexts.contains_key(&dest),
            "Aliased context {src:?} to {dest:?} doesn't have a context"
        );

        self.alias_context.insert(src, dest);
    }

    pub fn alias_type_to_parent(&mut self, src: CsTypeTag, dest: CsTypeTag) {
        // if context_check && !self.all_contexts.contains_key(&dest) {
        //     panic!("Aliased nested type {src:?} to {dest:?} doesn't have a parent");
        // }
        if src == dest {
            panic!("Self {src:?} can't point to dest!")
        }
        if self.alias_nested_type_to_parent.get(&dest) == Some(&src) {
            panic!("Parent {dest:?} can't be assigned to src {src:?}!")
        }
        self.alias_nested_type_to_parent.insert(src, dest);
    }

    pub fn get_context_root_tag(&self, ty: CsTypeTag) -> CsTypeTag {
        self.alias_context
            .get(&ty)
            .cloned()
            // .map(|t| self.get_context_root_tag(*t))
            .unwrap_or(ty)
    }
    pub fn get_parent_or_self_tag(&self, ty: CsTypeTag) -> CsTypeTag {
        self.alias_nested_type_to_parent
            .get(&ty)
            .cloned()
            .map(|t| self.get_parent_or_self_tag(t))
            .unwrap_or(ty)
    }

    pub fn make_nested_from(
        &mut self,
        metadata: &Metadata<'_>,
        tdi: TypeDefinitionIndex,
        generic_inst: Option<&Vec<usize>>,
    ) -> Option<&mut TypeContext> {
        let ty_tag = CsTypeTag::TypeDefinitionIndex(tdi);
        let ty_def = &metadata.metadata.global_metadata.type_definitions[tdi];
        let context_root_tag = self.get_context_root_tag(ty_tag);

        if self.filling_types.contains(&context_root_tag) {
            panic!("Currently filling type {context_root_tag:?}, cannot fill")
        }

        // Why is the borrow checker so dumb?
        // Using entries causes borrow checker to die :(
        if self.filled_types.contains(&ty_tag) {
            return Some(self.all_contexts.get_mut(&context_root_tag).unwrap());
        }

        if self.get_cs_type(ty_tag).is_some() {
            return self.get_context_mut(ty_tag);
        }

        let context_tag = self.get_context_root_tag(ty_tag);
        let context_type_data: TypeDefinitionIndex = context_tag.into();
        let context_td = &metadata.metadata.global_metadata.type_definitions[context_type_data];

        if metadata.blacklisted_types.contains(&tdi) {
            warn!(
                "Skipping nested type because it's blacklisted! {context_tag:?} {}",
                context_td.full_name(metadata.metadata, true)
            );
            return None;
        }

        let nested_inherits_declaring = ty_def.is_assignable_to(context_td, metadata.metadata);
        if nested_inherits_declaring {
            warn!(
                "Nested type \"{}\" inherits declaring type \"{}\"",
                ty_def.full_name(metadata.metadata, true),
                context_td.full_name(metadata.metadata, true)
            );
        }

        match nested_inherits_declaring {
            true => {
                // If a nested type inherits its declaring type, move it to its own CppContext
                let context = TypeContext::make(metadata, tdi, ty_tag, generic_inst);

                // Unnest type does not alias to another context or type
                self.alias_context.remove(&ty_tag);
                self.alias_nested_type_to_parent.remove(&ty_tag);

                self.all_contexts.insert(ty_tag, context);
                self.all_contexts.get_mut(&ty_tag)
            }
            false => {
                let new_cpp_type = CsType::make_cs_type(metadata, tdi, ty_tag, generic_inst)
                    .expect("Failed to make nested type");

                let context = self.get_context_mut(ty_tag).unwrap();
                // self.alias_type_to_context(new_cpp_type.self_tag, context_root_tag, true);

                // context.insert_cpp_type(stub);
                context.insert_cpp_type(new_cpp_type);

                Some(context)
            }
        }
    }

    /// Make a generic type
    /// based of an existing type definition
    /// and give it the generic args
    pub fn make_generic_from(
        &mut self,
        method_spec: &Il2CppMethodSpec,
        metadata: &mut Metadata,
    ) -> Option<&mut TypeContext> {
        // Not a generic class, no type needed
        if method_spec.class_inst_index == u32::MAX {
            return None;
        }
        // Skip generic methods?
        if method_spec.method_inst_index != u32::MAX {
            return None;
        }

        let method =
            &metadata.metadata.global_metadata.methods[method_spec.method_definition_index];
        let ty_def = &metadata.metadata.global_metadata.type_definitions[method.declaring_type];

        if ty_def.is_interface() {
            // Skip interface
            info!(
                "Skipping make interface for generic instantiation {}",
                ty_def.full_name(metadata.metadata, true)
            );
            return None;
        }

        let type_data = CsTypeTag::TypeDefinitionIndex(method.declaring_type);
        let tdi = method.declaring_type;
        let context_root_tag = self.get_context_root_tag(type_data);

        if metadata.blacklisted_types.contains(&tdi) {
            warn!(
                "Skipping generic instantiation {tdi:?} {} {}",
                method_spec.class_inst_index,
                ty_def.full_name(metadata.metadata, true)
            );
            return None;
        }

        if self.filling_types.contains(&context_root_tag) {
            panic!("Currently filling type {context_root_tag:?}, cannot fill")
        }

        let generic_class_ty_data = CsTypeTag::GenericInstantiation(GenericInstantiation {
            tdi,
            inst: method_spec.class_inst_index as usize,
        });

        let generic_inst =
            &metadata.metadata_registration.generic_insts[method_spec.class_inst_index as usize];

        // Why is the borrow checker so dumb?
        // Using entries causes borrow checker to die :(
        if self.filled_types.contains(&generic_class_ty_data) {
            return Some(self.all_contexts.get_mut(&context_root_tag).unwrap());
        }

        if self.get_cs_type(generic_class_ty_data).is_some() {
            return self.get_context_mut(generic_class_ty_data);
        }

        let mut new_cpp_type = CsType::make_cs_type(
            metadata,
            tdi,
            generic_class_ty_data,
            Some(&generic_inst.types),
        )
        .expect("Failed to make generic type");
        new_cpp_type.self_tag = generic_class_ty_data;
        self.alias_type_to_context(new_cpp_type.self_tag, context_root_tag);

        // TODO: Not needed since making a cpp type will already be a stub in other passes?
        // this is the generic stub
        // this might cause problems, hopefully not
        // since two types can coexist with the TDI though only one is nested
        // let mut stub = new_cpp_type.clone();
        // stub.self_tag = type_data;

        new_cpp_type.requirements.add_dependency_tag(type_data);

        // if generic type is a nested type
        // put it under the parent's `nested_types` field
        // otherwise put it in the typedef's hashmap

        let context = self.get_context_mut(generic_class_ty_data).unwrap();

        // context.insert_cpp_type(stub);
        context.insert_cpp_type(new_cpp_type);

        Some(context)
    }

    ///
    /// It's important this gets called AFTER the type is filled
    ///
    pub fn fill_generic_method_inst(
        &mut self,
        method_spec: &Il2CppMethodSpec,
        metadata: &mut Metadata,
    ) -> Option<&mut TypeContext> {
        if method_spec.method_inst_index == u32::MAX {
            return None;
        }

        let method =
            &metadata.metadata.global_metadata.methods[method_spec.method_definition_index];

        // is reference type
        // only make generic spatialization
        let type_data = CsTypeTag::TypeDefinitionIndex(method.declaring_type);
        let tdi = method.declaring_type;

        let ty_def = &metadata.metadata.global_metadata.type_definitions[method.declaring_type];

        if metadata.blacklisted_types.contains(&tdi) {
            info!(
                "Skipping {tdi:?} {} since it is blacklisted",
                ty_def.full_name(metadata.metadata, true)
            );
            return None;
        }

        if ty_def.is_interface() {
            // Skip interface
            info!(
                "Skipping fill generic method interface for generic instantiation {}",
                ty_def.full_name(metadata.metadata, true)
            );
            return None;
        }

        let context_root_tag = self.get_context_root_tag(type_data);

        let generic_class_ty_data = if method_spec.class_inst_index != u32::MAX {
            CsTypeTag::GenericInstantiation(GenericInstantiation {
                tdi,
                inst: method_spec.class_inst_index as usize,
            })
        } else {
            type_data
        };

        self.borrow_cs_type(generic_class_ty_data, |collection, mut cpp_type| {
            let method_index = method_spec.method_definition_index;
            cpp_type.add_method_generic_inst(method_spec, metadata);
            cpp_type.create_method(ty_def, method_index, metadata, true);

            cpp_type
        });

        self.all_contexts.get_mut(&context_root_tag)
    }

    pub fn fill_generic_class_inst(
        &mut self,
        method_spec: &Il2CppMethodSpec,
        metadata: &mut Metadata,
    ) -> Option<&mut TypeContext> {
        if method_spec.class_inst_index == u32::MAX {
            return None;
        }
        // Skip generic methods?
        if method_spec.method_inst_index != u32::MAX {
            return None;
        }

        let method =
            &metadata.metadata.global_metadata.methods[method_spec.method_definition_index];

        let ty_def = &metadata.metadata.global_metadata.type_definitions[method.declaring_type];

        // only make generic spatialization
        let type_data = CsTypeTag::TypeDefinitionIndex(method.declaring_type);
        let tdi = method.declaring_type;

        if metadata.blacklisted_types.contains(&tdi) {
            info!(
                "Skipping {tdi:?} {} since it is blacklisted",
                ty_def.full_name(metadata.metadata, true)
            );
            return None;
        }

        if ty_def.is_interface() {
            // Skip interface
            info!(
                "Skipping fill class interface for generic instantiation {}",
                ty_def.full_name(metadata.metadata, true)
            );
            return None;
        }

        let context_root_tag = self.get_context_root_tag(type_data);

        let generic_class_ty_data = if method_spec.class_inst_index != u32::MAX {
            CsTypeTag::GenericInstantiation(GenericInstantiation {
                tdi,
                inst: method_spec.class_inst_index as usize,
            })
        } else {
            type_data
        };

        self.borrow_cs_type(generic_class_ty_data, |collection: &mut TypeContextCollection, mut cpp_type| {
            // cpp_type.make_generics_args(metadata, collection);
            collection.fill_cpp_type(&mut cpp_type, metadata);

            cpp_type
        });

        self.all_contexts.get_mut(&context_root_tag)
    }

    pub fn make_from(
        &mut self,
        metadata: &Metadata,

        type_data: TypeData,
        generic_inst: Option<&Vec<usize>>,
    ) -> &mut TypeContext {
        let type_tag = CsType::get_tag_tdi(type_data);
        assert!(
            !metadata.child_to_parent_map.contains_key(&type_tag),
            "Cannot create context for nested type",
        );
        let context_root_tag = self.get_context_root_tag(type_tag.into());

        if self.filling_types.contains(&context_root_tag) {
            panic!("Currently filling type {context_root_tag:?}, cannot fill")
        }

        if self.borrowing_types.contains(&context_root_tag) {
            panic!("Currently borrowing context {context_root_tag:?}, cannot fill")
        }

        // Why is the borrow checker so dumb?
        // Using entries causes borrow checker to die :(
        if self.all_contexts.contains_key(&context_root_tag) {
            return self.all_contexts.get_mut(&context_root_tag).unwrap();
        }

        let tdi = context_root_tag.get_tdi();
        let context = TypeContext::make(metadata, tdi, context_root_tag, generic_inst);
        // Now do children
        for cpp_type in context.typedef_types.values() {
            for n in &cpp_type.nested_types {
                self.alias_type_to_parent(*n, cpp_type.self_tag);
                self.alias_type_to_context(*n, context_root_tag);
            }
        }
        self.all_contexts.insert(context_root_tag, context);
        self.all_contexts.get_mut(&context_root_tag).unwrap()
    }

    ///
    /// By default will only look for nested types of the context, ignoring other CppTypes
    ///
    pub fn get_cs_type(&self, ty: CsTypeTag) -> Option<&CsType> {
        let context_root_tag = self.get_context_root_tag(ty);
        let _parent_root_tag = self.get_parent_or_self_tag(ty);

        self.get_context(context_root_tag)
            .and_then(|c| c.get_types().get(&ty))
    }

    ///
    /// By default will only look for nested types of the context, ignoring other CppTypes
    ///
    pub fn get_cs_type_mut(&mut self, ty: CsTypeTag) -> Option<&mut CsType> {
        let context_root_tag = self.get_context_root_tag(ty);
        let _parent_root_tag = self.get_parent_or_self_tag(ty);
        self.get_context_mut(context_root_tag)
            .and_then(|c| c.get_types_mut().get_mut(&ty))
    }

    pub fn borrow_cs_type<F>(&mut self, ty: CsTypeTag, func: F)
    where
        F: Fn(&mut Self, CsType) -> CsType,
    {
        let context_ty = self.get_context_root_tag(ty);
        if self.borrowing_types.contains(&context_ty) {
            panic!("Already borrowing this context!");
        }

        let declaring_ty = self.get_parent_or_self_tag(ty);

        let context = self.all_contexts.get_mut(&context_ty).unwrap();

        // TODO: Needed?
        // self.borrowing_types.insert(context_ty);

        // search in root
        // clone to avoid failing il2cpp_name
        let Some(declaring_cpp_type) = context.typedef_types.get(&declaring_ty).cloned() else {
            panic!("No type {declaring_ty:#?} found!")
        };
        let _old_tag = declaring_cpp_type.self_tag;
        let new_cpp_ty = func(self, declaring_cpp_type);

        let context = self.all_contexts.get_mut(&context_ty).unwrap();

        context.insert_cpp_type(new_cpp_ty);

        self.borrowing_types.remove(&context_ty);
    }

    pub fn get_context(&self, type_tag: CsTypeTag) -> Option<&TypeContext> {
        let context_tag = self.get_context_root_tag(type_tag);
        if self.borrowing_types.contains(&context_tag) {
            panic!("Borrowing this context! {context_tag:?}");
        }
        self.all_contexts.get(&context_tag)
    }
    pub fn get_context_mut(&mut self, type_tag: CsTypeTag) -> Option<&mut TypeContext> {
        let context_tag = self.get_context_root_tag(type_tag);
        if self.borrowing_types.contains(&context_tag) {
            panic!("Borrowing this context! {context_tag:?}");
        }
        self.all_contexts
            .get_mut(&self.get_context_root_tag(context_tag))
    }

    pub fn new() -> TypeContextCollection {
        TypeContextCollection {
            all_contexts: Default::default(),
            filled_types: Default::default(),
            filling_types: Default::default(),
            alias_nested_type_to_parent: Default::default(),
            alias_context: Default::default(),
            borrowing_types: Default::default(),
        }
    }
    pub fn get(&self) -> &HashMap<CsTypeTag, TypeContext> {
        &self.all_contexts
    }
    pub fn get_mut(&mut self) -> &mut HashMap<CsTypeTag, TypeContext> {
        &mut self.all_contexts
    }
}
