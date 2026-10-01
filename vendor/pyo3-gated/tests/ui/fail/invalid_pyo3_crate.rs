use pyo3_gated::py_compat_struct;

#[py_compat_struct(pyo3_crate = "not a path")]
pub struct Color;

fn main() {}
