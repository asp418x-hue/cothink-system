use pyo3_gated::{py_compat_methods, py_compat_struct};

macro_rules! helper {
    () => {
        pub const VALUE: u8 = 1;
    };
}

#[py_compat_struct(stub_gen = false)]
pub struct Color;

#[py_compat_methods(stub_gen = false)]
impl Color {
    helper!();
}

fn main() {}
