use pyo3_gated::{py_compat, py_compat_enum, py_compat_methods, py_compat_struct};

macro_rules! rust_only_macro_item {
    () => {
        pub const FROM_MACRO: u8 = 2;
    };
}

#[py_compat_struct(stub_gen = false)]
#[cfg_attr(any(), repr(C))]
#[doc = "Generic holder."]
#[allow(dead_code)]
#[deprecated(note = "attribute preservation")]
pub struct Generic<'a, T>
where
    T: Clone + 'a,
{
    #[doc = "The held value."]
    pub value: &'a T,
}

#[py_compat_methods(stub_gen = false)]
impl<'a, T> Generic<'a, T>
where
    T: Clone + 'a,
{
    #[inline]
    #[allow(clippy::unused_self)]
    #[must_use]
    pub fn value(&self) -> &'a T {
        self.value
    }

    #[rust_only]
    pub const KIND: &'static str = "generic";

    #[rust_only]
    rust_only_macro_item!();
}

#[py_compat_struct(stub_gen = false)]
pub struct TupleStruct(pub u8, #[pyo3(get)] pub u8);

#[py_compat_struct(stub_gen = false)]
pub struct UnitStruct;

#[py_compat_enum(stub_gen = false)]
pub enum ExplicitDiscriminants {
    Red = 1,
    Green = 2,
    Blue = 3,
}

#[py_compat(stub_gen = false)]
pub fn dispatched_fn(a: u8) -> u8 {
    a
}

fn main() {}
