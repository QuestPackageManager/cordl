use std::collections::{HashMap, HashSet};

use brocolib::global_metadata::{ImageIndex, Il2CppTypeDefinition, MethodIndex, TypeDefinitionIndex};
use itertools::Itertools;

#[cfg(feature = "il2cpp_v39")]
use super::type_extensions::TypeDefinitionExtensions;
use super::type_extensions::TypeIndexExt;

#[cfg(all(test, feature = "il2cpp_v39"))]
mod tests;

pub struct MethodCalculations {
    pub estimated_size: usize,
    pub addrs: u64,
}

#[repr(u8)]
#[derive(Clone, Copy)]
pub enum PointerSize {
    Bytes8 = 8,
}

#[derive(Clone)]
pub struct TypeDefinitionPair<'a> {
    pub ty: &'a Il2CppTypeDefinition,
    pub tdi: TypeDefinitionIndex,
}

impl<'a> TypeDefinitionPair<'a> {
    fn new(ty: &'a Il2CppTypeDefinition, tdi: TypeDefinitionIndex) -> TypeDefinitionPair<'a> {
        TypeDefinitionPair { ty, tdi }
    }
}

pub type Il2cppNamespace<'a> = &'a str;
pub type Il2cppName<'a> = &'a str;

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Il2cppFullName<'a>(pub Il2cppNamespace<'a>, pub Il2cppName<'a>);

pub struct CordlMetadata<'a> {
    pub metadata: &'a brocolib::Metadata<'a, 'a>,
    pub metadata_registration: &'a brocolib::runtime_metadata::Il2CppMetadataRegistration,
    pub code_registration: &'a brocolib::runtime_metadata::Il2CppCodeRegistration<'a>,

    // Method index in metadata
    pub method_calculations: HashMap<MethodIndex, MethodCalculations>,
    pub parent_to_child_map: HashMap<TypeDefinitionIndex, Vec<TypeDefinitionPair<'a>>>,
    pub child_to_parent_map: HashMap<TypeDefinitionIndex, TypeDefinitionPair<'a>>,

    pub unity_object_tdi: TypeDefinitionIndex,
    // pub string_tdi: TypeDefinitionIndex,
    pub object_tdi: TypeDefinitionIndex,

    pub name_to_tdi: HashMap<Il2cppFullName<'a>, TypeDefinitionIndex>,
    pub blacklisted_types: HashSet<TypeDefinitionIndex>,

    /// Which image (assembly) declares a given type - custom attribute tokens are only
    /// unique within their own image, since il2cpp keeps each assembly's original token
    /// numbering when merging them into one metadata file. Built from
    /// `Il2CppImageDefinition::typeStart`/`typeCount`
    /// (`vm/GlobalMetadataFileInternals.h:209`), the same fields `img.types(metadata)` walks.
    pub tdi_to_image: HashMap<TypeDefinitionIndex, ImageIndex>,
    /// Per image: token (raw value) -> offset into `attribute_data` where that member's
    /// custom attributes are encoded. Built from each image's slice of
    /// `Il2CppCustomAttributeDataRange { token, startOffset }`
    /// (`vm/GlobalMetadataFileInternals.h:236`), addressed via
    /// `Il2CppImageDefinition::customAttributeStart`/`customAttributeCount` (same header, line
    /// 209) - a `HashMap` standing in for the `bsearch` by token that
    /// `vm/GlobalMetadata.cpp:857` (`GetCustomAttributeTypeToken`) does at runtime.
    /// See [`crate::generate::cs_attributes`].
    pub custom_attributes_by_image: HashMap<u32, HashMap<u32, u32>>,

    pub pointer_size: PointerSize,
    pub packing_field_offset: u8,
    pub size_is_default_offset: u8,
    pub specified_packing_field_offset: u8,
    pub packing_is_default_offset: u8,
}

impl<'a> CordlMetadata<'a> {
    /// Resolve the runtime parent rather than the enum backing type stored in v39.
    pub fn parent_type_index(&self, ty: &Il2CppTypeDefinition) -> Option<usize> {
        #[cfg(feature = "il2cpp_v39")]
        if ty.is_enum_type() {
            // Unity 6000.3 GlobalMetadata::FromTypeDefinition sets enum_class as the
            // parent and uses the serialized parentIndex for element_class instead.
            let enum_tdi = self
                .name_to_tdi
                .get(&Il2cppFullName("System", "Enum"))
                .expect("No System.Enum type found");
            return Some(
                self.metadata.global_metadata.type_definitions[*enum_tdi]
                    .byval_type_index
                    .idx(),
            );
        }

        ty.parent_index.idx_is_valid().then(|| ty.parent_index.idx())
    }

    /// Returns the size of the base object.
    /// To be used for boxing/unboxing and various offset computations.
    pub fn object_size(&self) -> u8 {
        (self.pointer_size as u8) * 2
    }

    pub fn parse(&mut self) {
        let gm = &self.metadata.global_metadata;
        self.parse_name_tdi(gm);
        self.parse_type_hierarchy(gm);
        self.parse_method_size(gm);
        self.parse_custom_attributes(gm);
    }

    /// Builds the lookup tables [`Self::tdi_to_image`] and
    /// [`Self::custom_attributes_by_image`] used to find a member's custom attribute data.
    fn parse_custom_attributes(&mut self, gm: &'a brocolib::global_metadata::GlobalMetadata) {
        let mut tdi_to_image = HashMap::new();
        let mut custom_attributes_by_image = HashMap::new();

        for (image_idx, image) in gm.images.as_vec().iter().enumerate() {
            let image_index = ImageIndex::new(image_idx as u32);

            // Il2CppImageDefinition::typeStart/typeCount, GlobalMetadataFileInternals.h:214-215
            let type_start = image.type_start.index();
            for offset in 0..image.type_count {
                tdi_to_image.insert(TypeDefinitionIndex::new(type_start + offset), image_index);
            }

            // image.custom_attributes() is Il2CppImageDefinition::customAttributeStart/Count
            // (GlobalMetadataFileInternals.h:223-224) slicing the table of
            // Il2CppCustomAttributeDataRange { token, startOffset } (same header, line 236) -
            // the exact table vm/GlobalMetadata.cpp:857 (GetCustomAttributeTypeToken) binary
            // searches by token; a HashMap serves the same purpose here.
            let attr_offsets: HashMap<u32, u32> = image
                .custom_attributes(self.metadata)
                .iter()
                .map(|range| (range.token.0, range.start_offset))
                .collect();
            custom_attributes_by_image.insert(image_idx as u32, attr_offsets);
        }

        self.tdi_to_image = tdi_to_image;
        self.custom_attributes_by_image = custom_attributes_by_image;
    }

    fn parse_type_hierarchy(&mut self, gm: &'a brocolib::global_metadata::GlobalMetadata) {
        // self.parentToChildMap = childToParent
        //     .into_iter()
        //     .map(|(p, p_tdi, c)| (p_tdi, c))
        //     .collect();

        // child -> parent
        let parent_to_child_map: Vec<(TypeDefinitionPair<'a>, Vec<TypeDefinitionPair<'a>>)> = gm
            .type_definitions
            .as_vec()
            .iter()
            .enumerate()
            .filter_map(|(tdi, td)| {
                if td.nested_type_count == 0 {
                    return None;
                }

                let nested_types: Vec<TypeDefinitionPair> = td
                    .nested_types(self.metadata)
                    .iter()
                    .map(|&nested_tdi| {
                        let nested_td = &gm.type_definitions[nested_tdi];
                        TypeDefinitionPair::new(nested_td, nested_tdi)
                    })
                    .collect();

                if nested_types.is_empty() {
                    return None;
                }
                Some((
                    TypeDefinitionPair::new(td, TypeDefinitionIndex::new(tdi as u32)),
                    nested_types,
                ))
            })
            .collect();

        let child_to_parent_map: Vec<(&TypeDefinitionPair<'a>, &TypeDefinitionPair<'a>)> =
            parent_to_child_map
                .iter()
                .flat_map(|(p, children)| {
                    let reverse = children.iter().map(|c| (c, p)).collect_vec();

                    reverse
                })
                .collect();

        self.child_to_parent_map = child_to_parent_map
            .into_iter()
            .map(|(c, p)| (c.tdi, p.clone()))
            .collect();

        self.parent_to_child_map = parent_to_child_map
            .into_iter()
            .map(|(p, c)| (p.tdi, c.into_iter().collect_vec()))
            .collect();
    }

    fn parse_method_size(&mut self, gm: &brocolib::global_metadata::GlobalMetadata) {
        // sorted by address
        // method index -> address
        let method_addresses_sorted: Vec<u64> = self
            .code_registration
            .code_gen_modules
            .iter()
            .flat_map(|m| &m.method_pointers)
            .copied()
            .sorted()
            .collect();
        // address -> method index in sorted list
        let method_addresses_sorted_map: HashMap<u64, usize> = method_addresses_sorted
            .iter()
            .enumerate()
            .map(|(index, m_ptr)| (*m_ptr, index))
            .collect();

        self.method_calculations = self
            .metadata
            .runtime_metadata
            .code_registration
            .code_gen_modules
            .iter()
            .flat_map(|cgm| {
                let img = gm
                    .images
                    .as_vec()
                    .iter()
                    .find(|i| cgm.name == i.name(self.metadata))
                    .unwrap();

                let method_calculations: HashMap<MethodIndex, MethodCalculations> =
                    img.types(self.metadata)
                        .iter()
                        // get all methods
                        .flat_map(|ty| {
                            ty.methods(self.metadata).iter().enumerate().map(|(i, m)| {
                                (MethodIndex::new(ty.method_start.index() + i as u32), m)
                            })
                        })
                        // get method calculations
                        .map(|(method_index, method)| {
                            let method_pointer_index = method.token.rid() as usize - 1;
                            let method_pointer =
                                *cgm.method_pointers.get(method_pointer_index).unwrap();

                            let sorted_address_index =
                                *method_addresses_sorted_map.get(&method_pointer).unwrap();
                            let next_method_pointer = method_addresses_sorted
                                .get(sorted_address_index + 1)
                                .cloned()
                                .unwrap_or(0);

                            let estimated_size =
                                if method_pointer == 0x0 || next_method_pointer == 0x0 {
                                    usize::MAX
                                } else {
                                    method_pointer.abs_diff(next_method_pointer) as usize
                                };

                            (
                                method_index,
                                MethodCalculations {
                                    estimated_size,
                                    addrs: method_pointer,
                                },
                            )
                        })
                        .collect();

                method_calculations
            })
            .collect();
    }

    fn parse_name_tdi(&mut self, gm: &brocolib::global_metadata::GlobalMetadata) {
        self.name_to_tdi = gm
            .type_definitions
            .as_vec()
            .iter()
            .enumerate()
            .map(|(tdi, td)| {
                (
                    Il2cppFullName(td.namespace(self.metadata), td.name(self.metadata)),
                    TypeDefinitionIndex::new(tdi as u32),
                )
            })
            .collect();
    }
}
