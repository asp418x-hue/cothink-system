use pyo3_gated::py_compat_fn;

#[py_compat_fn(stub_gen = false)]
#[gen_stub]
#[pyo3(signature = (a, b = 0))]
pub fn add(#[pyo3(from_py_with = "convert")] a: i32, b: i32) -> i32 {
    a + b
}

#[py_compat_fn]
#[gen_stub(override_return_type(type_repr = "object"))]
pub fn callback(
    #[gen_stub(override_type(type_repr = "collections.abc.Callable"))] value: usize,
) -> usize {
    value
}

fn main() {}
