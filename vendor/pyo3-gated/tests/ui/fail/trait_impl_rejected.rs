use pyo3_gated::{py_compat_methods, py_compat_struct};

#[py_compat_struct(stub_gen = false)]
pub struct Color {
    pub r: u8,
}

trait Value {
    fn value(&self) -> u8;
}

#[py_compat_methods(stub_gen = false)]
impl Value for Color {
    fn value(&self) -> u8 {
        self.r
    }
}

fn main() {}
