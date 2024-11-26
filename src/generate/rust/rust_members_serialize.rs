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
        }
    }
}

impl Writable for RustStruct {
    fn write(&self, writer: &mut Writer) -> Result<()> {
        writeln!(writer, "struct {} {{", self.name)?;
        for field in &self.fields {
            field.write(writer)?;
        }
        writeln!(writer, "}}")?;
        Ok(())
    }
}

impl Writable for RustField {
    fn write(&self, writer: &mut Writer) -> Result<()> {
        writeln!(writer, "{}: {},", self.name, self.field_type)?;
        Ok(())
    }
}

impl Writable for RustEnum {
    fn write(&self, writer: &mut Writer) -> Result<()> {
        writeln!(writer, "enum {} {{", self.name)?;
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
        for (i, field) in self.fields.iter().enumerate() {
            let name = &field.name;
            let ty = &field.field_type;
            write!(writer, "{name}: {ty}")?;
            write!(writer, ", ")?;
        }
        write!(writer, ")")?;

        Ok(())
    }
}

impl Writable for RustFunction {
    fn write(&self, writer: &mut Writer) -> Result<()> {
        let name = &self.name;
        write!(writer, "fn {name}(",)?;
        if self.is_self {
            if self.is_ref {
                write!(writer, "&")?;
            }
            if self.is_mut {
                write!(writer, "mut")?;
            }
            write!(writer, "self")?;
        }

        for (i, param) in self.params.iter().enumerate() {
            param.write(writer)?;
            write!(writer, ", ")?;
        }
        write!(writer, ")")?;

        if let Some(ref return_type) = self.return_type {
            write!(writer, " -> {}", return_type)?;
        }
        if let Some(body) = &self.body {
            writeln!(writer, "{{")?;
            writeln!(writer, "{body}")?;
            writeln!(writer, "}}")?;
        }

        Ok(())
    }
}

impl Writable for RustParam {
    fn write(&self, writer: &mut Writer) -> Result<()> {
        write!(writer, "{}:", self.name)?;

        // &
        if self.is_ref {
            write!(writer, "&")?;
        }
        // mut
        if self.is_mut {
            write!(writer, "mut")?;
        }
        // ty
        write!(writer, "{}", self.param_type)?;

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