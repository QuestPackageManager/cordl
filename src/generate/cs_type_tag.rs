use brocolib;

use brocolib::runtime_metadata::TypeData;

use brocolib::global_metadata::TypeDefinitionIndex;

// TODO:
/// Indices into the [`Il2CppMetadataRegistration::generic_insts`] field
pub type GenericInstIndex = usize;

// TDI -> Generic inst
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct GenericInstantiation {
    pub tdi: TypeDefinitionIndex,
    /// Indices into the [`Il2CppMetadataRegistration::generic_insts`] field
    pub inst: GenericInstIndex,
}

// Unique identifier for a CppType
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum CsTypeTag {
    TypeDefinitionIndex(TypeDefinitionIndex),
    GenericInstantiation(GenericInstantiation),
}

impl From<TypeDefinitionIndex> for CsTypeTag {
    fn from(value: TypeDefinitionIndex) -> Self {
        CsTypeTag::TypeDefinitionIndex(value)
    }
}

impl From<TypeData> for CsTypeTag {
    fn from(value: TypeData) -> Self {
        match value {
            TypeData::TypeDefinitionIndex(i) => i.into(),
            _ => panic!("Can't use {value:?} for CppTypeTag"),
        }
    }
}

impl From<CsTypeTag> for TypeData {
    fn from(value: CsTypeTag) -> Self {
        match value {
            CsTypeTag::TypeDefinitionIndex(i) => TypeData::TypeDefinitionIndex(i),
            CsTypeTag::GenericInstantiation(g) => TypeData::GenericClassIndex(g.inst), // TODO:?
            _ => panic!("Can't go from {value:?} to TypeData"),
        }
    }
}

impl From<CsTypeTag> for TypeDefinitionIndex {
    fn from(value: CsTypeTag) -> Self {
        match value {
            CsTypeTag::TypeDefinitionIndex(i) => i,
            CsTypeTag::GenericInstantiation(generic_inst) => generic_inst.tdi,
            _ => panic!("Type is not a TDI! {value:?}"),
        }
    }
}

impl CsTypeTag {
    pub fn from_generic_class_index(
        generic_class_idx: usize,
        metadata: &brocolib::Metadata,
    ) -> Self {
        let generic_class = &metadata
            .runtime_metadata
            .metadata_registration
            .generic_classes[generic_class_idx];

        let ty: brocolib::runtime_metadata::Il2CppType =
            metadata.runtime_metadata.metadata_registration.types[generic_class.type_index];
        // Unwrap
        let TypeData::TypeDefinitionIndex(tdi) = ty.data else {
            panic!("No TDI for generic inst!")
        };

        Self::GenericInstantiation(GenericInstantiation {
            tdi,
            inst: generic_class
                .context
                .class_inst_idx
                .expect("Not a generic class inst idx"),
        })
    }
    pub fn from_type_data(type_data: TypeData, metadata: &brocolib::Metadata) -> Self {
        match type_data {
            TypeData::TypeDefinitionIndex(tdi) => tdi.into(),
            TypeData::TypeIndex(_) => panic!("Not supported on array! {type_data:#?}"),
            TypeData::GenericClassIndex(generic_class_idx) => {
                Self::from_generic_class_index(generic_class_idx, metadata)
            }
            _ => todo!("{type_data:#?}"),
        }
    }

    pub fn get_tdi(&self) -> TypeDefinitionIndex {
        match self {
            CsTypeTag::TypeDefinitionIndex(tdi) => *tdi,
            CsTypeTag::GenericInstantiation(gen_inst) => gen_inst.tdi,
        }
    }
}
