use pyo3_gated::{define_py_module, py_compat_fn, py_compat_struct};

#[py_compat_struct(stub_gen = false)]
pub struct Color {
    pub r: u8,
}

#[py_compat_fn(stub_gen = false)]
pub fn add(a: i32, b: i32) -> i32 {
    a + b
}

define_py_module! {
    module module_macro_extended_plain;
    doc: "Extended module macro syntax.";
    classes: [Color];
    functions: [add];
    constants: [("VERSION", "0.0.0")];
    init: |m| {
        let _ = m;
        Ok(())
    };
}

fn main() {}
