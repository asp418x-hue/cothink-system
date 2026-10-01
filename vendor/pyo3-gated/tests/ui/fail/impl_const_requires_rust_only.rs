use pyo3_gated::{py_compat_methods, py_compat_struct};

#[py_compat_struct(stub_gen = false)]
pub struct Color;

#[py_compat_methods(stub_gen = false)]
impl Color {
    pub const CHANNELS: u8 = 3;
}

fn main() {}
