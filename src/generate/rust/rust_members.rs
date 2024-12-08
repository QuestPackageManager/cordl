use std::str::FromStr;

use itertools::Itertools;
use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::parse_quote;

use super::rust_name_components::RustNameComponents;

#[derive(Clone, Debug, Default)]
pub enum Visibility {
    Public,
    PublicCrate,
    #[default]
    Private,
}

#[derive(Clone)]
pub struct RustStruct {
    pub fields: Vec<RustField>,
    pub packing: Option<u32>,
}
#[derive(PartialEq, Eq, PartialOrd, Ord, Debug, Default, Hash, Clone)]
pub struct RustGeneric {
    pub name: String,
    pub bounds: Vec<String>,
}

impl RustGeneric {
    pub fn to_token_stream(&self) -> syn::GenericParam {
        let name = format_ident!("{}", self.name);
        match self.bounds.is_empty() {
            true => parse_quote!(#name),
            false => {
                let bounds = self
                    .bounds
                    .iter()
                    .map(|b| -> syn::Type { syn::parse_str(b.as_str()).unwrap() });
                parse_quote!(#name: #(#bounds)+*)
            }
        }
    }
}

impl FromStr for RustGeneric {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(RustGeneric {
            name: s.to_string(),
            bounds: Default::default(),
        })
    }
}

impl From<String> for RustGeneric {
    fn from(value: String) -> Self {
        RustGeneric {
            name: value,
            bounds: Default::default(),
        }
    }
}

impl ToString for RustGeneric {
    fn to_string(&self) -> String {
        match self.bounds.is_empty() {
            true => self.name.clone(),
            false => format!("{}: {}", self.name, self.bounds.join("+")),
        }
    }
}

#[derive(Clone)]
pub struct RustUnion {
    pub fields: Vec<RustField>,
}

#[derive(Clone)]

pub struct ConstRustField {
    pub name: syn::Ident,
    pub field_type: syn::Type,
    pub value: syn::Expr,
    pub visibility: Visibility,
}

#[derive(Clone)]
pub struct RustField {
    pub name: syn::Ident,
    pub field_type: syn::Type,
    pub visibility: Visibility,
    pub offset: u32,
}

#[derive(Clone, Debug)]

pub struct RustFeature {
    pub name: String,
}

#[derive(Clone)]
pub struct RustFunction {
    pub name: syn::Ident,
    pub params: Vec<RustParam>,
    pub return_type: Option<syn::Type>,
    pub body: Option<Vec<syn::Stmt>>,
    pub generics: Vec<RustGeneric>,
    pub where_clause: Option<syn::WhereClause>,

    pub is_self: bool,
    pub is_ref: bool,
    pub is_mut: bool,
    pub visibility: Visibility,
}

#[derive(Clone)]
pub struct RustParam {
    pub name: syn::Ident,
    pub param_type: syn::Type,
}

#[derive(Clone)]
pub struct RustTraitImpl {
    pub impl_data: syn::ItemImpl,
}

#[derive(Clone)]
pub struct RustImpl {
    pub trait_name: Option<String>,
    pub type_name: String,

    pub generics: Vec<Generic>,
    pub lifetimes: Vec<Lifetime>,

    pub methods: Vec<RustFunction>,
}

type Generic = String;
type Lifetime = String;

impl RustFunction {
    pub fn to_token_stream(&self) -> TokenStream {
        let name: syn::Ident = format_ident!("{}", self.name);
        let generics: Option<syn::Generics> = match self.generics.is_empty() {
            true => None,
            false => {
                let generics = self.generics.iter().map(|g| -> syn::GenericParam {
                    syn::parse_str(g.to_string().as_str()).unwrap()
                });
                Some(parse_quote!(<#(#generics),*>))
            }
        };

        let self_param: Option<syn::FnArg> = match self.is_self {
            true if self.is_mut && self.is_ref => Some(parse_quote! { &mut self }),
            true if self.is_ref => Some(parse_quote! { &self }),
            true if self.is_mut => Some(parse_quote! { mut self }),
            true => Some(parse_quote! { self }),
            false => None,
        };

        let params = self.params.iter().map(|p| -> syn::FnArg {
            let name = format_ident!("{}", p.name);
            let param_type = &p.param_type;
            parse_quote! { #name: #param_type }
        });
        let return_type: syn::ReturnType = match &self.return_type {
            Some(t_ty) => {
                parse_quote! { -> #t_ty }
            }
            None => parse_quote! {},
        };
        let where_clause = &self.where_clause;

        let visibility = self.visibility.to_token_stream();
        let mut tokens = match self_param {
            Some(self_param) => {
                quote! {
                    #visibility fn #name #generics (#self_param, #(#params),*) #return_type #where_clause
                }
            }
            None => {
                quote! {
                    #visibility fn #name #generics (#(#params),*) #return_type #where_clause
                }
            }
        };

        if let Some(body) = &self.body {
            tokens = quote! {
                #tokens {
                    #(#body)*
                }
            };
        } else {
            tokens = quote! {
                #tokens;
            };
        }

        tokens
    }
}

impl Visibility {
    pub fn to_token_stream(&self) -> syn::Visibility {
        match self {
            Visibility::Public => parse_quote! { pub },
            Visibility::PublicCrate => parse_quote! { pub(crate) },
            Visibility::Private => parse_quote! {},
        }
    }
}

impl ToString for Visibility {
    fn to_string(&self) -> String {
        match self {
            Visibility::Public => "pub".to_string(),
            Visibility::PublicCrate => "pub(crate)".to_string(),
            Visibility::Private => "".to_string(),
        }
    }
}
