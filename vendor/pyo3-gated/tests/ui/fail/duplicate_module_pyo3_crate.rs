use pyo3_gated::define_py_module;

define_py_module! {
    pyo3_crate = "::pyo3";
    pyo3_crate = "::pyo3";
    module duplicate_module_pyo3_crate;
}

fn main() {}
