use pyo3_gated::py_compat_enum;

#[py_compat_enum(pyfunction_args(signature = ()))]
pub enum Color {
    Red,
}

fn main() {}
