use pyo3_gated::{py_compat_methods, py_compat_struct};

#[py_compat_struct(stub_gen = false)]
pub struct Color {
    pub r: u8,
}

#[py_compat_methods(stub_gen = false)]
impl Color {
    #[py_only]
    #[py_attrs]
    pub fn bad(&self) {}
}

fn main() {}
