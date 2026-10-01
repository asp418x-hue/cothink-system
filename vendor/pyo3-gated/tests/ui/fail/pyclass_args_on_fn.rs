use pyo3_gated::py_compat_fn;

#[py_compat_fn(pyclass_args(module = "bad"))]
pub fn add(a: i32, b: i32) -> i32 {
    a + b
}

fn main() {}
