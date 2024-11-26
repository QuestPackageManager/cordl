use std::sync::Arc;

use crate::{
    data::name_components::NameComponents,
    generate::{cs_type::CsType, cs_type_tag::CsTypeTag, offsets::SizeInfo},
};

use super::{config::RustGenerationConfig, rust_members::{RustField, RustFunction, RustTrait}, rust_name_components::RustNameComponents, rust_name_resolver::RustNameResolver};

#[derive(Clone, Debug)]
pub struct RustType {
    pub fields: Vec<RustField>,
    pub methods: Vec<RustFunction>,
    pub traits: Vec<RustTrait>,

    pub interfaces: Vec<String>,

    pub is_value_type: bool,
    pub is_enum_type: bool,
    pub is_reference_type: bool,
    pub is_interface: bool,

    pub self_tag: CsTypeTag,

    pub generics: Option<Vec<String>>,
    pub cs_name_components: NameComponents,
    pub rs_name_components: RustNameComponents,
    pub(crate) prefix_comments: Vec<String>,

    pub packing: Option<u32>,
    pub size_info: Option<SizeInfo>,
}
impl RustType {
    pub(crate) fn fill(
        &self,
        cs_type: CsType,
        name_resolver: &RustNameResolver,
        config: &RustGenerationConfig,
    ) {
        todo!()
    }
}
