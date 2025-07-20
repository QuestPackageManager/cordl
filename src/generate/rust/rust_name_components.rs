use itertools::Itertools;
use proc_macro2::TokenStream;
use quote::{ToTokens, format_ident, quote};
use syn::parse_quote;

use crate::data::name_components::NameComponents;

use super::rust_members::RustGeneric;

#[derive(PartialEq, Eq, PartialOrd, Ord, Debug, Default, Hash, Clone)]
pub struct RustNameComponents {
    pub name: String,
    pub namespace: Option<String>,
    pub generics: Option<Vec<RustGeneric>>,

    pub is_ref: bool,
    pub is_dyn: bool,
    pub is_static_ref: bool,
    pub is_ptr: bool,
    pub is_mut: bool,
}

impl RustNameComponents {
    // TODO: Add setting for adding :: prefix
    // however, this cannot be allowed in all cases
    pub fn combine_all(&self) -> String {
        // will be empty if no namespace or declaring types
        let prefix = self
            .namespace
            .as_ref()
            .map(|s| format!("{s}::"))
            .unwrap_or_default();

        let mut completed = format!("{prefix}{}", self.name);

        if let Some(generics) = &self.generics {
            completed = format!(
                "{completed}<{}>",
                generics.iter().map(|s| &s.name).join(",")
            );
        }

        let mut prefix: String = String::new();
        // &
        if self.is_static_ref {
            prefix = "&'static ".to_string();
        } else if self.is_ref {
            prefix = "&".to_string();
        } else if self.is_ptr {
            prefix = "*".to_string();
        }
        // mut
        if self.is_mut {
            prefix += "mut ";
        }
        if self.is_dyn {
            prefix += "dyn ";
        }

        // add & or * or mut
        completed = prefix + &completed;

        completed
    }

    pub fn wrap_by_gc(self) -> RustNameComponents {
        if !self.is_ptr
            || (self.namespace.as_deref() == Some("quest_hook::libil2cpp") && self.name == "Gc")
        {
            return self;
        }

        RustNameComponents {
            namespace: Some("quest_hook::libil2cpp".to_owned()),
            name: "Gc".to_string(),
            generics: Some(vec![RustGeneric {
                name: self.with_no_prefix().combine_all(),
                ..Default::default()
            }]),
            is_ref: false,
            is_ptr: false,
            is_mut: false,
            is_static_ref: false,
            is_dyn: false,
        }
    }

    pub fn with_no_prefix(mut self) -> RustNameComponents {
        self.is_ref = false;
        self.is_ptr = false;
        self.is_mut = false;
        self.is_static_ref = false;
        self
    }

    pub fn with_ref(mut self) -> RustNameComponents {
        self.is_ref = true;

        self.is_static_ref = false;
        self.is_ptr = false;
        self
    }
    pub fn with_static_ref(mut self) -> RustNameComponents {
        self.is_static_ref = true;

        self.is_ref = false;
        self.is_ptr = false;
        self
    }

    pub fn with_ptr(mut self) -> RustNameComponents {
        self.is_ptr = true;

        self.is_ref = false;
        self.is_static_ref = false;
        self
    }
    pub fn with_mut(mut self) -> RustNameComponents {
        self.is_mut = true;
        self
    }
    pub fn without_mut(mut self) -> RustNameComponents {
        self.is_mut = false;
        self
    }

    pub fn remove_generics(mut self) -> RustNameComponents {
        self.generics = None;
        self
    }
    pub fn remove_generics_bounds(mut self) -> RustNameComponents {
        self.generics = self.generics.map(|g| {
            g.into_iter()
                .map(|mut g| {
                    g.bounds = Default::default();
                    g
                })
                .collect()
        });
        self
    }
    pub fn remove_namespace(mut self) -> RustNameComponents {
        self.namespace = None;
        self
    }

    pub fn to_name_ident(&self) -> TokenStream {
        match self.generics {
            Some(ref generics) => {
                let generics = generics.iter().map(|g| -> syn::GenericArgument {
                    let s = g.to_string();
                    syn::parse_str(&s).unwrap()
                });

                let name = format_ident!(r#"{}"#, self.name);

                quote! {
                    #name <#(#generics),*>
                }
            }
            None => format_ident!(r#"{}"#, self.name).to_token_stream(),
        }
    }

    pub fn to_type_path_token(&self) -> syn::TypePath {
        let mut completed = self
            .clone()
            .remove_generics_bounds()
            .to_name_ident()
            .to_token_stream();

        // add namespace
        if let Some(namespace) = self.namespace.as_ref() {
            let namespace_ident: syn::Path =
                syn::parse_str(namespace).expect("Failed to parse namespace");
            completed = quote! {
                #namespace_ident::#completed
            }
        }

        parse_quote! {
             #completed
        }
    }

    pub fn to_type_token(&self) -> syn::Type {
        let completed = self.to_type_path_token();

        // add & or * or mut
        let mut prefix = if self.is_ref {
            quote! { & }
        } else if self.is_ptr {
            quote! { * }
        } else {
            quote! {}
        };

        if self.is_mut {
            prefix = parse_quote! { #prefix mut  };
        }
        if self.is_dyn {
            prefix = parse_quote! { #prefix dyn  };
        }

        parse_quote! {
            #prefix #completed
        }
    }
}

impl NameComponents {
    pub fn to_combined_ident(&self) -> TokenStream {
        todo!();
        let mut completed = match self.name.split_once('`') {
            Some((a, b)) => {
                let ident_a = format_ident!(r#"{}"#, a);
                let ident_b: syn::Lit = syn::parse_str(b).expect("Failed to parse number");

                quote! {
                    #ident_a ^ #ident_b
                }
            }
            None => format_ident!(r#"{}"#, self.name).to_token_stream(),
        };

        // add declaring types
        if let Some(declaring_types) = self.declaring_types.as_ref() {
            let declaring_types = declaring_types.iter().map(|g| format_ident!(r#"{}"#, g));

            completed = quote! {
                #(#declaring_types)/ * / #completed
            }
        }

        // add namespace
        // if let Some(namespace_str) = self.namespace.as_ref() {
        //     let namespace: syn::punctuated::Punctuated<Ident, Token![.]> =
        //         syn::parse_str(&namespace_str).expect("Failed to parse namespace");

        //     completed = quote! {
        //         #namespace.#completed
        //     }
        // }

        // add generics
        if let Some(generics_strings) = &self.generics {
            let generics: Vec<syn::GenericArgument> = generics_strings
                .iter()
                .map(|g| syn::parse_str(g).expect("Failed to parse generic"))
                .collect();

            completed = quote! {
                #completed <#(#generics),*>
            }
        }

        completed
    }
}

impl From<NameComponents> for RustNameComponents {
    fn from(value: NameComponents) -> Self {
        Self {
            name: value.name,
            namespace: value.namespace,
            generics: value.generics.map(|g| {
                g.into_iter()
                    .map(|g| RustGeneric {
                        name: g,
                        ..Default::default()
                    })
                    .collect_vec()
            }),
            ..Default::default()
        }
    }
}

impl From<String> for RustNameComponents {
    fn from(value: String) -> Self {
        Self {
            name: value,
            ..Default::default()
        }
    }
}
