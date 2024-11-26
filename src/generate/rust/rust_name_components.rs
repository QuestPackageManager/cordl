use crate::data::name_components::NameComponents;

#[derive(PartialEq, Eq, PartialOrd, Ord, Debug, Default, Hash, Clone)]
pub struct RustNameComponents {
    pub name: String,
    pub namespace: Option<String>,
    pub declaring_types: Option<Vec<String>>,
    pub generics: Option<Vec<String>>,
}

impl RustNameComponents {
    // TODO: Add setting for adding :: prefix
    // however, this cannot be allowed in all cases
    pub fn combine_all(&self) -> String {
        let combined_declaring_types = self.declaring_types.as_ref().map(|d| d.join("::"));

        // will be empty if no namespace or declaring types
        let prefix = combined_declaring_types
            .as_ref()
            .or(self.namespace.as_ref())
            .map(|s| {
                if s.is_empty() {
                    "::".to_string()
                } else {
                    format!("::{s}::")
                }
            })
            .unwrap_or_default();

        let mut completed = format!("{prefix}{}", self.name);

        if let Some(generics) = &self.generics {
            completed = format!("{completed}<{}>", generics.join(","));
        }

        completed
    }
}

impl From<NameComponents> for RustNameComponents {
    fn from(value: NameComponents) -> Self {
        Self {
            name: value.name,
            namespace: value.namespace,
            declaring_types: value.declaring_types,
            generics: value.generics,
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
