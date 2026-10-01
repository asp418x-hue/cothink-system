use pyo3_gated::{py_compat_methods, py_compat_struct};

#[py_compat_struct(stub_gen = false)]
pub struct Color;

#[py_compat_methods(py_only, stub_gen = false)]
impl Color {
    pub fn red(&self) -> u8 {
        0
    }
}

fn main() {}
