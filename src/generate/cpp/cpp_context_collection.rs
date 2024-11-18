use std::collections::{HashMap, HashSet};

use crate::generate::{
    cs_context_collection::TypeContextCollection, cs_type_tag::CsTypeTag, metadata::Metadata,
};

use super::{config::CppGenerationConfig, cpp_context::CppContext, cpp_type::CppType};

#[derive(Default)]
pub struct CppContextCollection {
    // Should always be a TypeDefinitionIndex
    all_contexts: HashMap<CsTypeTag, CppContext>,
    alias_context: HashMap<CsTypeTag, CsTypeTag>,
    filled_types: HashSet<CsTypeTag>,
    filling_types: HashSet<CsTypeTag>,
    borrowing_types: HashSet<CsTypeTag>,
}

impl CppContextCollection {
    pub fn from_cs_collection(
        collection: TypeContextCollection,
        metadata: &Metadata,
        config: &CppGenerationConfig,
    ) -> CppContextCollection {
        let mut cpp_collection = CppContextCollection::default();

        for (tag, context) in collection.get() {
            cpp_collection.all_contexts.insert(
                *tag,
                CppContext::make(*tag, context.clone(), metadata, config),
            );
        }
        cpp_collection.alias_context = collection.alias_context;

        cpp_collection
    }

    fn fill_cpp_type(&mut self, cpp_type: &mut CppType, metadata: &Metadata) {
        let tag = cpp_type.self_tag;

        if self.filled_types.contains(&tag) {
            return;
        }
        if self.filling_types.contains(&tag) {
            panic!("Currently filling type {tag:?}, cannot fill")
        }

        // Move ownership to local
        self.filling_types.insert(tag);

        cpp_type.fill_from_il2cpp(metadata, self);

        self.filled_types.insert(tag);
        self.filling_types.remove(&tag.clone());
    }

    pub fn fill(&mut self, metadata: &Metadata, type_tag: CsTypeTag) {
        let context_tag = self.get_context_root_tag(type_tag);

        if self.filled_types.contains(&type_tag) {
            return;
        }

        if self.borrowing_types.contains(&context_tag) {
            panic!("Borrowing context {context_tag:?}");
        }

        // Move ownership to local
        let cpp_type_entry = self
            .all_contexts
            .get_mut(&context_tag)
            .expect("No cpp context")
            .typedef_types
            .remove_entry(&type_tag);

        // In some occasions, the CppContext can be empty
        if let Some((_t, mut cpp_type)) = cpp_type_entry {
            self.fill_cpp_type(&mut cpp_type, metadata);

            // Move ownership back up
            self.all_contexts
                .get_mut(&context_tag)
                .expect("No cpp context")
                .insert_cpp_type(cpp_type);
        }
    }

    ///
    /// By default will only look for nested types of the context, ignoring other CppTypes
    ///
    pub fn get_cpp_type(&self, ty: CsTypeTag) -> Option<&CppType> {
        let context_root_tag = self.get_context_root_tag(ty);

        self.get_context(context_root_tag)
            .and_then(|c| c.get_types().get(&ty))
    }

    ///
    /// By default will only look for nested types of the context, ignoring other CppTypes
    ///
    pub fn get_cpp_type_mut(&mut self, ty: CsTypeTag) -> Option<&mut CppType> {
        let context_root_tag = self.get_context_root_tag(ty);

        self.get_context_mut(context_root_tag)
            .and_then(|c| c.get_types_mut().get_mut(&ty))
    }

    pub fn borrow_cpp_type<F>(&mut self, ty: CsTypeTag, func: F)
    where
        F: Fn(&mut Self, CppType) -> CppType,
    {
        let context_ty = self.get_context_root_tag(ty);
        if self.borrowing_types.contains(&context_ty) {
            panic!("Already borrowing this context!");
        }

        let context = self.all_contexts.get_mut(&context_ty).unwrap();

        // TODO: Needed?
        // self.borrowing_types.insert(context_ty);

        // search in root
        // clone to avoid failing il2cpp_name
        let Some(declaring_cpp_type) = context.typedef_types.get(&ty).cloned() else {
            panic!("No type {context_ty:#?} found!")
        };
        let _old_tag = declaring_cpp_type.self_tag;
        let new_cpp_ty = func(self, declaring_cpp_type);

        let context = self.all_contexts.get_mut(&context_ty).unwrap();

        context.insert_cpp_type(new_cpp_ty);

        self.borrowing_types.remove(&context_ty);
    }

    pub fn get_context(&self, type_tag: CsTypeTag) -> Option<&CppContext> {
        let context_tag = self.get_context_root_tag(type_tag);
        if self.borrowing_types.contains(&context_tag) {
            panic!("Borrowing this context! {context_tag:?}");
        }
        self.all_contexts.get(&context_tag)
    }
    pub fn get_context_mut(&mut self, type_tag: CsTypeTag) -> Option<&mut CppContext> {
        let context_tag = self.get_context_root_tag(type_tag);
        if self.borrowing_types.contains(&context_tag) {
            panic!("Borrowing this context! {context_tag:?}");
        }
        self.all_contexts
            .get_mut(&self.get_context_root_tag(context_tag))
    }

    pub fn get_context_root_tag(&self, ty: CsTypeTag) -> CsTypeTag {
        self.alias_context
            .get(&ty)
            .cloned()
            // .map(|t| self.get_context_root_tag(*t))
            .unwrap_or(ty)
    }
}
