use pyo3_gated::define_py_module;

define_py_module! {
    feature = "python";
    feature = "bindings";
    module duplicate_module_feature;
}

fn main() {}
