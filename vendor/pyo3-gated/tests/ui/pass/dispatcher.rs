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

    pub fn red(&self) -> u8 {
        self.r
    }
}

#[py_compat(stub_gen = false)]
pub enum Palette {
    Red,
    Green,
}

#[py_compat(stub_gen = false)]
pub fn add(a: i32, b: i32) -> i32 {
    a + b
}

fn main() {}
