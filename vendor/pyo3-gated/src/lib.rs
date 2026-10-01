#![doc = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/README.md"))]

pub use pyo3_gated_macros::{
    define_py_module, py_compat, py_compat_enum, py_compat_fn, py_compat_methods, py_compat_struct,
};

#[doc(hidden)]
pub use pyo3_gated_macros::__pyo3_gated_stub_gen_alias;

#[cfg(feature = "stub-gen")]
pub use pyo3_stub_gen::Result as StubGenResult;

#[cfg(feature = "python")]
pub use pyo3;

// Macro-private compatibility surface: pyo3-stub-gen derives currently emit
// absolute `::pyo3_stub_gen::...` and `::pyo3::...` paths. The helper macro
// aliases this facade crate under those names, so these root items must exist
// but are not part of the documented stable API.
#[doc(hidden)]
#[allow(ambiguous_glob_reexports)]
#[cfg(feature = "stub-gen")]
pub use pyo3::*;

#[doc(hidden)]
#[allow(ambiguous_glob_reexports)]
#[cfg(feature = "stub-gen")]
pub use pyo3_stub_gen::*;

#[cfg(feature = "stub-gen")]
pub mod stub_gen {
    pub use pyo3_stub_gen::*;
}

pub mod prelude {
    pub use crate::{
        define_py_module, py_compat, py_compat_enum, py_compat_fn, py_compat_methods,
        py_compat_struct,
    };

    #[cfg(feature = "stub-gen")]
    pub use crate::StubGenResult;

    #[cfg(feature = "python")]
    pub use crate::pyo3::prelude::*;
}

#[doc(hidden)]
pub mod __private {
    #[cfg(feature = "python")]
    pub use crate::pyo3;

    #[cfg(feature = "stub-gen")]
    pub use pyo3_stub_gen;
}

#[macro_export]
macro_rules! define_pyo3_gated_stub_info {
    ($name:ident) => {
        #[cfg(feature = "stub-gen")]
        $crate::__pyo3_gated_stub_gen_alias!();

        #[cfg(feature = "stub-gen")]
        $crate::__private::pyo3_stub_gen::define_stub_info_gatherer!($name);
    };
}

#[macro_export]
macro_rules! define_stub_info_gatherer {
    ($name:ident) => {
        $crate::define_pyo3_gated_stub_info!($name);
    };
}
