use pyo3_gated::{define_py_module, py_compat_fn, py_compat_struct};

#[py_compat_struct(stub_gen = false, pyo3_crate = "::my_pyo3")]
pub struct Color {
    pub r: u8,
}

#[py_compat_fn(stub_gen = false, pyo3_crate = "::my_pyo3")]
pub fn add(a: i32, b: i32) -> i32 {
    a + b
}

define_py_module! {
    module override_module;
    pyo3_crate = "::my_pyo3";
    classes: [Color];
    functions: [add];
}

fn main() {}
