use color_eyre::eyre::Result;
use quote::ToTokens;

use crate::generate::writer::{Writable, Writer};
use std::io::Write;

use super::rust_members::{
    RustEnum, RustField, RustFunction, RustImpl, RustItem, RustNamedItem, RustParam, RustStruct,
    RustTrait, RustUnion, RustVariant,
};

impl Writable for RustNamedItem {
    fn write(&self, writer: &mut Writer) -> color_eyre::Result<()> {
        writeln!(writer, "{}", self.visibility.to_string())?;
        match &self.item {
            RustItem::Struct(s) => s.write_named(writer, &self.name),
            RustItem::Union(u) => u.write_named(writer, &self.name),
            RustItem::Enum(e) => e.write_named(writer, &self.name),
            RustItem::NamedType(_) => {
                write!(writer, "{name}", name = self.name)?;
                Ok(())
            }
        }
    }
}

impl Writable for RustItem {
    fn write(&self, writer: &mut Writer) -> Result<()> {
        match self {
            RustItem::Union(u) => u.write(writer),
            RustItem::Struct(s) => s.write(writer),
            RustItem::Enum(e) => e.write(writer),
            RustItem::NamedType(s) => {
                write!(writer, "{s}")?;
                Ok(())
            }
        }
    }
}

impl RustStruct {
    pub fn write_named(&self, writer: &mut Writer, name: &str) -> Result<()> {
        writeln!(writer, "struct {name} {{")?;
        for field in &self.fields {
            field.write(writer)?;
        }
        writeln!(writer, "}}")?;
        Ok(())
    }
}

impl Writable for RustStruct {
    fn write(&self, writer: &mut Writer) -> Result<()> {
        self.write_named(writer, "")
    }
}

impl RustUnion {
    pub fn write_named(&self, writer: &mut Writer, name: &str) -> Result<()> {
        writeln!(writer, "union {name} {{")?;
        for field in &self.fields {
            field.write(writer)?;
        }
        writeln!(writer, "}}")?;
        Ok(())
    }
}

impl Writable for RustUnion {
    fn write(&self, writer: &mut Writer) -> Result<()> {
        self.write_named(writer, "")
    }
}

impl Writable for RustField {
    fn write(&self, writer: &mut Writer) -> Result<()> {
        let visibility = self.visibility.to_string();
        let name = &self.name;
        let field_type = self.field_type.to_token_stream().to_string();

        write!(writer, "{visibility} {name}: {field_type}")?;
        writeln!(writer, ",")?;

        Ok(())
    }
}

impl RustEnum {
    pub fn write_named(&self, writer: &mut Writer, name: &str) -> Result<()> {
        writeln!(writer, "enum {name} {{")?;
        for variant in &self.variants {
            variant.write(writer)?;
            writeln!(writer, ",")?;
        }
        writeln!(writer, "}}")?;
        Ok(())
    }
}

impl Writable for RustEnum {
    fn write(&self, writer: &mut Writer) -> Result<()> {
        self.write_named(writer, "")
    }
}

impl Writable for RustVariant {
    fn write(&self, writer: &mut Writer) -> Result<()> {
        write!(writer, "{}", self.name)?;
        if self.fields.is_empty() {
            return Ok(());
        }

        write!(writer, " (")?;
        for (_i, field) in self.fields.iter().enumerate() {
            let name = &field.name;
            let ty = field.field_type.to_token_stream().to_string();
            write!(writer, "{name}: {ty}")?;
            write!(writer, ", ")?;
        }
        write!(writer, ")")?;

        Ok(())
    }
}

impl Writable for RustFunction {
    fn write(&self, writer: &mut Writer) -> Result<()> {
        let visibility = self.visibility.to_string();

        let name = &self.name;
        write!(writer, "{visibility} fn {name}(",)?;
        if self.is_self {
            if self.is_ref {
                write!(writer, "&")?;
            }
            if self.is_mut {
                write!(writer, "mut ")?;
            }
            write!(writer, "self,")?;
        }

        for (_i, param) in self.params.iter().enumerate() {
            param.write(writer)?;
            write!(writer, ", ")?;
        }
        write!(writer, ")")?;

        if let Some(ref return_type) = self.return_type {
            write!(writer, " -> {}", return_type.to_token_stream())?;
        }
        match &self.body {
            Some(body) => {
                writeln!(writer, "{{")?;
                writeln!(writer, "{}", body.to_token_stream().to_string())?;
                writeln!(writer, "}}")?;
            }
            _ => {
                writeln!(writer, ";")?;
            }
        }

        Ok(())
    }
}

impl Writable for RustParam {
    fn write(&self, writer: &mut Writer) -> Result<()> {
        write!(writer, "{}: ", self.name)?;

        // ty
        write!(writer, " {}", self.param_type.to_token_stream())?;

        // => {name}: &mut {ty}
        Ok(())
    }
}

impl Writable for RustTrait {
    fn write(&self, writer: &mut Writer) -> Result<()> {
        writeln!(writer, "trait {} {{", self.name)?;
        for method in &self.methods {
            method.write(writer)?;
        }
        writeln!(writer, "}}")?;
        Ok(())
    }
}

impl Writable for RustImpl {
    fn write(&self, writer: &mut Writer) -> Result<()> {
        write!(writer, "impl")?;
        if !self.lifetimes.is_empty() {
            write!(writer, "<")?;
            for (i, lifetime) in self.lifetimes.iter().enumerate() {
                if i > 0 {
                    write!(writer, ", ")?;
                }
                write!(writer, "{}", lifetime)?;
            }
            write!(writer, ">")?;
        }
        write!(writer, " ")?;
        if let Some(ref trait_name) = self.trait_name {
            write!(writer, "{} for ", trait_name)?;
        }
        write!(writer, "{}", self.type_name)?;
        if !self.generics.is_empty() {
            write!(writer, "<")?;
            for (i, generic) in self.generics.iter().enumerate() {
                if i > 0 {
                    write!(writer, ", ")?;
                }
                write!(writer, "{}", generic)?;
            }
            write!(writer, ">")?;
        }
        writeln!(writer, " {{")?;
        for method in &self.methods {
            method.write(writer)?;
        }
        writeln!(writer, "}}")?;
        Ok(())
    }
}
