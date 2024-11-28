use std::default;

use super::rust_name_components::RustNameComponents;

#[derive(Clone, Debug, Default)]
pub enum Visibility {
    Public,
    PublicCrate,
    #[default]
    Private,
}

#[derive(Clone, Debug)]
pub enum RustItem {
    Struct(RustStruct),
    Enum(RustEnum),
    Function(RustFunction),
    TypeAlias(String, String),
    NamedType(String),
}

#[derive(Clone, Debug)]
pub struct RustStruct {
    pub name: String,
    pub fields: Vec<RustField>,
    pub visibility: Visibility,
    pub packing: Option<u32>,
}

#[derive(Clone, Debug)]
pub struct RustUnion {
    pub name: String,
    pub fields: Vec<RustField>,
    pub visibility: Visibility,
}

#[derive(Clone, Debug)]
pub struct RustField {
    pub name: String,
    pub field_type: RustItem,
    pub visibility: Visibility,
    pub offset: u32,
}

#[derive(Clone, Debug)]
pub struct RustEnum {
    pub name: String,
    pub variants: Vec<RustVariant>,
    pub visibility: Visibility,
}

#[derive(Clone, Debug)]
pub struct RustVariant {
    pub name: String,
    pub fields: Vec<RustField>,
}

#[derive(Clone, Debug)]
pub struct RustFunction {
    pub name: String,
    pub params: Vec<RustParam>,
    pub return_type: Option<String>,
    pub body: Option<String>,

    pub is_self: bool,
    pub is_ref: bool,
    pub is_mut: bool,
    pub visibility: Visibility,
}

#[derive(Clone, Debug)]
pub struct RustParam {
    pub name: String,
    pub param_type: String,

    pub is_mut: bool,
}

#[derive(Clone, Debug)]
pub struct RustTrait {
    pub name: String,
    pub methods: Vec<RustFunction>,
    pub visibility: Visibility,
}

#[derive(Clone, Debug)]
pub struct RustImpl {
    pub trait_name: Option<String>,
    pub type_name: String,

    pub generics: Vec<Generic>,
    pub lifetimes: Vec<Lifetime>,

    pub methods: Vec<RustFunction>,
}

type Generic = String;
type Lifetime = String;

impl ToString for Visibility {
    fn to_string(&self) -> String {
        match self {
            Visibility::Public => "pub".to_string(),
            Visibility::PublicCrate => "pub(crate)".to_string(),
            Visibility::Private => "".to_string(),
        }
    }
}
