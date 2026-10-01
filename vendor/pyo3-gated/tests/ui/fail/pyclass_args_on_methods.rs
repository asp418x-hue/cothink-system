use pyo3_gated::{py_compat_methods, py_compat_struct};

#[py_compat_struct(stub_gen = false)]
pub struct Color;

#[py_compat_methods(pyclass_args(module = "bad"))]
impl Color {
    pub fn red(&self) -> u8 {
        0
    }
}

fn main() {}
