#[derive(PartialEq, Eq, PartialOrd, Ord, Debug, Default, Hash, Clone)]
pub struct NameComponents {
    pub namespace: Option<String>,
    pub declaring_types: Option<Vec<String>>,
    pub name: String,
    pub generics: Option<Vec<String>>,
}

impl NameComponents {
    // TODO: Add setting for adding :: prefix
    // however, this cannot be allowed in all cases
    pub fn combine_all(&self) -> String {
        let namespace = self.namespace.as_deref().unwrap_or("");
        let combined_declaring_types = self.declaring_types.as_ref().map(|d| d.join("/"));

        let mut completed = match combined_declaring_types {
            Some(combined_declaring_types) => {
                format!("{namespace}.{combined_declaring_types}/{}", self.name)
            }
            None => {
                format!("{namespace}.{}", self.name)
            }
        };

        if let Some(generics) = &self.generics {
            completed = format!("{completed}<{}>", generics.join(","));
        }

        completed
    }

    pub fn into_ref_generics(self) -> Self {
        Self {
            generics: self
                .generics
                .map(|opt| opt.into_iter().map(|_| "void*".to_string()).collect()),
            ..self
        }
    }

    pub fn remove_generics(self) -> Self {
        Self {
            generics: None,
            ..self
        }
    }

    /// just cpp name with generics
    pub fn formatted_name(&self, include_generics: bool) -> String {
        if let Some(generics) = &self.generics
            && include_generics
        {
            format!("{}<{}>", self.name, generics.join(","))
        } else {
            self.name.to_string()
        }
    }
}

impl From<String> for NameComponents {
    fn from(value: String) -> Self {
        Self {
            name: value,
            ..Default::default()
        }
    }
}
