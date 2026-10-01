use pyo3_gated::prelude::*;

#[py_compat(stub_gen = false)]
pub struct Color {
    #[pyo3(get, set)]
    pub r: u8,
}

#[py_compat(stub_gen = false)]
impl Color {
    #[py_attrs]
    #[new]
    pub fn new(r: u8) -> Self {
        Self { r }
    }

    #[rust_only]
    pub fn raw(self) -> u8 {
        self.r
    }
}

#[py_compat_fn(stub_gen = false, pyfunction_args(name = "add_numbers"))]
#[gen_stub(override_return_type(type_repr = "int"))]
pub fn add(#[gen_stub(override_type(type_repr = "int"))] a: i32, b: i32) -> i32 {
    a + b
}

pyo3_gated::define_py_module! {
    module plain;
    classes: [Color];
    functions: [add];
}

fn main() {}
