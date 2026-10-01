use pyo3_gated::py_compat_struct;

#[py_compat_struct(feature = "python", feature = "bindings")]
pub struct Color;

fn main() {}
