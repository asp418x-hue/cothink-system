use pyo3_gated::py_compat_struct;

#[py_compat_struct(pyfunction_args(name = "bad"))]
pub struct Color;

fn main() {}
