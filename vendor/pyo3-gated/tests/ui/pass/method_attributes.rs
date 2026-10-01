use pyo3_gated::{py_compat_methods, py_compat_struct};

#[py_compat_struct(stub_gen = false)]
pub struct Color {
    r: u8,
}

#[py_compat_methods(stub_gen = false)]
impl Color {
    #[py_attrs]
    #[new]
    pub fn new(r: u8) -> Self {
        Self { r }
    }

    #[getter]
    pub fn r(&self) -> u8 {
        self.r
    }

    #[setter]
    pub fn set_r(&mut self, r: u8) {
        self.r = r;
    }

    #[staticmethod]
    pub fn black() -> Self {
        Self { r: 0 }
    }

    #[classmethod]
    pub fn class_name() -> &'static str {
        "Color"
    }

    #[rust_only]
    pub const CHANNELS: usize = 1;
}

fn main() {
    let mut color = Color::black();
    color.set_r(3);
    assert_eq!(color.r(), 3);
}
