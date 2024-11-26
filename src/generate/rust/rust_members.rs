

#[derive(Clone, Debug)]
pub enum RustItem {
    Struct(RustStruct),
    Enum(RustEnum),
    Function(RustFunction),
}

#[derive(Clone, Debug)]
pub struct RustStruct {
    pub name: String,
    pub fields: Vec<RustField>,
}

#[derive(Clone, Debug)]
pub struct RustField {
    pub name: String,
    pub field_type: String,
}

#[derive(Clone, Debug)]
pub struct RustEnum {
    pub name: String,
    pub variants: Vec<RustVariant>,
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
}

#[derive(Clone, Debug)]
pub struct RustParam {
    pub name: String,
    pub param_type: String,
    pub is_ref: bool,
    pub is_mut: bool,
}

#[derive(Clone, Debug)]
pub struct RustTrait {
    pub name: String,
    pub methods: Vec<RustFunction>,
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