use std::{path::PathBuf, sync::LazyLock};

use itertools::Itertools;

pub static STATIC_CONFIG: LazyLock<RustGenerationConfig> = LazyLock::new(|| RustGenerationConfig {
    source_path: PathBuf::from("./codegen-rs/src"),
    cargo_config: PathBuf::from("./codegen-rs/Cargo.toml"),
});

pub struct RustGenerationConfig {
    pub source_path: PathBuf,
    pub cargo_config: PathBuf,
}

/// Sanitizes an arbitrary Il2Cpp metadata name (type name, generic parameter
/// name, field name, ...) into something usable as a Rust identifier.
///
/// Il2Cpp metadata names come from the target game/assembly and may be
/// arbitrarily obfuscated (e.g. renamed to raw control bytes by a name
/// obfuscator), so every character outside `[A-Za-z0-9_]` is replaced rather
/// than relying on a fixed blocklist of "expected" punctuation.
pub fn sanitize_rs_ident(string: &str) -> String {
    let mut s: String = string
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '_' { c } else { '_' })
        .collect();

    if s.chars().next().is_some_and(|c| c.is_numeric()) {
        s = format!("_cordl_{s}");
    }

    // A name that's just underscores (e.g. a single obfuscated control byte
    // replaced above) parses as the `_` wildcard token in most syn grammar
    // positions rather than a plain identifier - not usable as a generic
    // parameter/type name.
    if s.chars().all(|c| c == '_') {
        s = format!("_cordl_{s}");
    }
    s
}

impl RustGenerationConfig {
    pub fn namespace_rs(&self, string: &str) -> String {
        let final_ns = if string.is_empty() {
            "GlobalNamespace".to_owned()
        } else {
            string.split('.').map(sanitize_rs_ident).join("::")
        };

        format!("crate::{final_ns}")
    }

    #[inline]
    pub fn name_rs(&self, string: &str) -> String {
        self.name_rs_plus(string, &[])
    }

    pub fn name_rs_plus(&self, string: &str, additional_exclude: &[&str]) -> String {
        if string.trim().is_empty() {
            // TODO: handle when multiple params are empty whitespace
            return "_cordl_fixed_empty_name_whitespace".to_string();
        }

        if additional_exclude.contains(&string) {
            return format!("_cordl_{string}");
        }
        if string.to_lowercase() == "mod" {
            return format!("_cordl_{string}");
        }

        match string {
            // https://github.com/sc2ad/Il2Cpp-Modding-Codegen/blob/b3267c7099f0cc1853e57a1118d1bba3884b5f03/Codegen-CLI/Program.cs#L77-L87
            "alignas" | "alignof" | "and" | "and_eq" | "asm" | "atomic_cancel"
            | "atomic_commit" | "atomic_noexcept" | "auto" | "bitand" | "bitor" | "bool"
            | "break" | "case" | "catch" | "char" | "char8_t" | "char16_t" | "char32_t"
            | "class" | "compl" | "concept" | "const" | "consteval" | "constexpr" | "constinit"
            | "const_cast" | "continue" | "co_await" | "co_return" | "co_yield" | "decltype"
            | "default" | "delete" | "do" | "double" | "dynamic_cast" | "else" | "enum"
            | "explicit" | "export" | "extern" | "false" | "float" | "for" | "friend" | "goto"
            | "if" | "inline" | "int" | "long" | "mutable" | "namespace" | "new" | "noexcept"
            | "not" | "not_eq" | "nullptr" | "operator" | "or" | "or_eq" | "private"
            | "protected" | "public" | "reflexpr" | "register" | "reinterpret_cast"
            | "requires" | "return" | "short" | "signed" | "sizeof" | "static"
            | "static_assert" | "static_cast" | "struct" | "switch" | "synchronized"
            | "template" | "this" | "thread_local" | "throw" | "true" | "try" | "typedef"
            | "typeid" | "typename" | "union" | "unsigned" | "using" | "virtual" | "void"
            | "volatile" | "wchar_t" | "while" | "xor" | "xor_eq" | "INT_MAX" | "INT_MIN"
            | "Assert" | "bzero" | "ID" | "VERSION" | "NULL" | "EOF" | "MOD_ID" | "errno" | "linux" | "module"
            | "INFINITY" | "NAN" | "type" | "size" | "time" | "clock" | "rand" | "srand" | "exit" | "match" | 
            "panic" | "assert" | "debug_assert" | "assert_eq" | "assert_ne" | "debug_assert_eq" | "debug_assert_ne" 
            | "unreachable" | "unimplemented" | "todo" | "trait" | "impl" | "ref" | "mut" | "as" | "use" | "pub"
            | "Ok" | "Err" | "ffi" | "c_void" | "c_char" | "c_uchar" | "c_schar" | "c_short" | "c_ushort"
            | "c_int" | "c_uint" | "c_long" | "c_ulong" | "c_longlong" | "c_ulonglong" | "c_float" | "c_double" 
            | "where" | "Self" | "async" | "await" | "move" | "dyn" | "super" | "crate" | "mod" | "let" | "fn" | "in"
            | "priv" | "box" | "loop" | "final" | "macro" | "override" | "self" | "gen" | "_" |
            // networking headers
            "EPERM"
            | "ENOENT" | "ESRCH" | "EINTR" | "EIO" | "ENXIO" | "E2BIG" | "ENOEXEC" | "EBADF"
            | "ECHILD" | "EAGAIN" | "ENOMEM" | "EACCES" | "EFAULT" | "ENOTBLK" | "EBUSY"
            | "EEXIST" | "EXDEV" | "ENODEV" | "ENOTDIR" | "EISDIR" | "EINVAL" | "ENFILE"
            | "EMFILE" | "ENOTTY" | "ETXTBSY" | "EFBIG" | "ENOSPC" | "ESPIPE" | "EROFS"
            | "EMLINK" | "EPIPE" | "EDOM" | "ERANGE" | "EDEADLK" | "ENAMETOOLONG" | "ENOLCK"
            | "ENOSYS" | "ENOTEMPTY" | "ELOOP" | "EWOULDBLOCK" | "ENOMSG" | "EIDRM" | "ECHRNG"
            | "EL2NSYNC" | "EL3HLT" | "EL3RST" | "ELNRNG" | "EUNATCH" | "ENOCSI" | "EL2HLT"
            | "EBADE" | "EBADR" | "EXFULL" | "ENOANO" | "EBADRQC" | "EBADSLT" | "EDEADLOCK"
            | "EBFONT" | "ENOSTR" | "ENODATA" | "ETIME" | "ENOSR" | "ENONET" | "ENOPKG"
            | "EREMOTE" | "ENOLINK" | "EADV" | "ESRMNT" | "ECOMM" | "EPROTO" | "EMULTIHOP"
            | "EDOTDOT" | "EBADMSG" | "EOVERFLOW" | "ENOTUNIQ" | "EBADFD" | "EREMCHG"
            | "ELIBACC" | "ELIBBAD" | "ELIBSCN" | "ELIBMAX" | "ELIBEXEC" | "EILSEQ"
            | "ERESTART" | "ESTRPIPE" | "EUSERS" | "ENOTSOCK" | "EDESTADDRREQ" | "EMSGSIZE"
            | "EPROTOTYPE" | "ENOPROTOOPT" | "EPROTONOSUPPORT" | "ESOCKTNOSUPPORT"
            | "EOPNOTSUPP" | "EPFNOSUPPORT" | "EAFNOSUPPORT" | "EADDRINUSE" | "EADDRNOTAVAIL"
            | "ENETDOWN" | "ENETUNREACH" | "ENETRESET" | "ECONNABORTED" | "ECONNRESET"
            | "ENOBUFS" | "EISCONN" | "ENOTCONN" | "ESHUTDOWN" | "ETOOMANYREFS" | "ETIMEDOUT"
            | "ECONNREFUSED" | "EHOSTDOWN" | "EHOSTUNREACH" | "EALREADY" | "EINPROGRESS"
            | "ESTALE" | "EUCLEAN" | "ENOTNAM" | "ENAVAIL" | "EISNAM" | "EREMOTEIO" | "EDQUOT"
            | "ENOMEDIUM" | "EMEDIUMTYPE" | "ECANCELED" | "ENOKEY" | "EKEYEXPIRED"
            | "EKEYREVOKED" | "EKEYREJECTED" | "EOWNERDEAD" | "ENOTRECOVERABLE" | "ERFKILL"
            | "EHWPOISON" | "ENOTSUP" => {
                format!("_cordl_{string}")
            }


            _ => self.sanitize_to_rs_name(string),
        }
    }
    /// for converting C++ names into just a single C++ word
    pub fn sanitize_to_rs_name(&self, string: &str) -> String {
        sanitize_rs_ident(string)
    }
    pub fn namespace_path(&self, string: &str) -> String {
        string.replace(['<', '>', '`', '/'], "_").replace('.', "/")
    }

    pub(crate) fn feature_impl_class(&self, s: &str) -> String {
        s.replace([':', '`', '<', '>', '$', '=', ',', '|'], "_")
            .replace(['.', '/'], "+")
    }
    /// Represents a feature class name, which is used to generate the feature name in Rust.
    pub(crate) fn feature_def_class(&self, s: &str) -> String {
        format!("cordl_class_{}", self.feature_impl_class(s))
    }
}
