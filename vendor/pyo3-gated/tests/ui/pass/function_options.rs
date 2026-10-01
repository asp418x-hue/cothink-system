use pyo3_gated::py_compat_fn;

#[py_compat_fn(stub_gen = false, pyfunction_args(name = "add_numbers", signature = (a, b = 0)))]
#[pyo3(text_signature = "(a, b=0)")]
pub fn add(a: i32, b: i32) -> i32 {
    a + b
}

#[py_compat_fn(stub_gen = false, py_only)]
pub fn python_only() -> i32 {
    1
}

fn main() {
    assert_eq!(add(1, 2), 3);
}
