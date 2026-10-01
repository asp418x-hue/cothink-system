use pyo3_gated::{py_compat_enum, py_compat_methods, py_compat_struct};

#[py_compat_struct(stub_gen = false)]
#[derive(Clone, Debug, PartialEq)]
pub struct Generic<T>
where
    T: Clone,
{
    /// Preserved field docs.
    pub value: T,
}

#[py_compat_methods(stub_gen = false)]
impl<T> Generic<T>
where
    T: Clone,
{
    pub fn value(&self) -> T {
        self.value.clone()
    }
}

#[py_compat_struct(stub_gen = false)]
pub struct TupleStruct(pub u8, #[pyo3(get)] pub u8);

#[py_compat_struct(stub_gen = false)]
pub struct UnitStruct;

#[py_compat_enum(stub_gen = false)]
#[derive(Clone, Debug, PartialEq)]
pub enum ComplexEnum {
    Rgb { r: u8, g: u8, b: u8 },
    Named(String),
}

fn main() {
    let item = Generic { value: 1 };
    assert_eq!(item.value(), 1);
}
