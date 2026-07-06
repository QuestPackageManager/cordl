#[derive(PartialEq, Eq, PartialOrd, Ord, Debug, Default, Hash, Clone)]
pub struct NameComponents {
    pub namespace: Option<String>,
    pub declaring_types: Option<Vec<String>>,
    pub name: String,
    pub generics: Option<Vec<String>>,
}

impl NameComponents {
    pub fn combine_all(&self) -> String {
        let mut completed = self.declaring_name();

        // add namespace
        if let Some(namespace) = self.namespace.as_ref() {
            completed = format!("{namespace}.{completed}");
        }

        // add generics
        if let Some(generics) = &self.generics {
            completed = format!("{completed}<{}>", generics.join(","));
        }

        completed
    }

    pub fn remove_generics(self) -> Self {
        Self {
            generics: None,
            ..self
        }
    }

    pub fn remove_namespace(self) -> Self {
        Self {
            namespace: None,
            ..self
        }
    }

    pub fn declaring_name(&self) -> String {
        if let Some(declaring_types) = self.declaring_types.as_ref() {
            format!("{}/{}", declaring_types.join("/"), self.name)
        } else {
            self.name.clone()
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
