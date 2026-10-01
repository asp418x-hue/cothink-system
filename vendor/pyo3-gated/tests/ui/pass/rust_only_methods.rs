use pyo3_gated::{py_compat_methods, py_compat_struct};

#[py_compat_struct(stub_gen = false)]
pub struct Config {
    raw: u8,
}

#[py_compat_methods(stub_gen = false)]
impl Config {
    pub fn new(raw: u8) -> Self {
        Self { raw }
    }

    #[rust_only]
    pub fn from_raw_parts(raw: u8) -> Self {
        Self { raw }
    }

    pub fn raw(&self) -> u8 {
        self.raw
    }
}

fn main() {
    let config = Config::from_raw_parts(7);
    assert_eq!(config.raw(), 7);
}
