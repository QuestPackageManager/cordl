#[cfg(feature = "il2cpp_v29")]
mod offsets_29;

#[cfg(feature = "il2cpp_v29")]
pub use offsets_29::*;

#[cfg(any(feature = "il2cpp_v31", feature = "il2cpp_v39"))]
mod offsets_31;

#[cfg(any(feature = "il2cpp_v31", feature = "il2cpp_v39"))]
pub use offsets_31::*;
