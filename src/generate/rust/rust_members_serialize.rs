use color_eyre::eyre::Result;

use crate::generate::writer::{Writable, Writer};
use std::io::Write;

use super::rust_members::{
    RustEnum, RustField, RustFunction, RustImpl, RustItem, RustParam, RustStruct, RustTrait,
    RustVariant,
};

impl Writable for RustItem {
    fn write(&self, writer: &mut Writer) -> Result<()> {
        match self {
            RustItem::Struct(s) => s.write(writer),
            RustItem::Enum(e) => e.write(writer),
            RustItem::Function(func) => func.write(writer),
            RustItem::TypeAlias(_, _) => todo!(),
            RustItem::NamedType(s) => {
                write!(writer, "{s}")?;
                Ok(())
            }
        }
    }
}

impl Writable for RustStruct {
    fn write(&self, writer: &mut Writer) -> Result<()> {
        let visibility = self.visibility.to_string();
        let name = &self.name;

        writeln!(writer, "{visibility} struct {name} {{")?;
        for field in &self.fields {
            field.write(writer)?;
        }
        writeln!(writer, "}}")?;
        Ok(())
    }
}

impl Writable for RustField {
    fn write(&self, writer: &mut Writer) -> Result<()> {
        let visibility = self.visibility.to_string();
        let name = &self.name;
        let field_type = &self.field_type;

        write!(writer, "{visibility} {name}: ")?;
        field_type.write(writer)?;
        writeln!(writer, ",")?;

        Ok(())
    }
}

impl Writable for RustEnum {
    fn write(&self, writer: &mut Writer) -> Result<()> {
        let visibility = self.visibility.to_string();
        let name = &self.name;

        writeln!(writer, "{visibility} enum {name} {{")?;
        for variant in &self.variants {
            variant.write(writer)?;
            writeln!(writer, ",")?;
        }
        writeln!(writer, "}}")?;
        Ok(())
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
            let ty = &field.field_type;
            write!(writer, "{name}: ")?;
            ty.write(writer)?;
            write!(writer, ", ")?;
        }
        write!(writer, ")")?;

        Ok(())
    }
}

impl Writable for RustFunction {
    fn write(&self, writer: &mut Writer) -> Result<()> {
        let visibility = self
            .visibility
            .as_ref()
            .map(|s| s.to_string())
            .unwrap_or_default();

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
            write!(writer, " -> {}", return_type)?;
        }
        match &self.body {
            Some(body) => {
                writeln!(writer, "{{")?;
                writeln!(writer, "{body}")?;
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
        write!(writer, " {}", self.param_type)?;

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
