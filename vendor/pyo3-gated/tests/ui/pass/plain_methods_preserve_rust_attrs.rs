use pyo3_gated::{py_compat_methods, py_compat_struct};

#[py_compat_struct(stub_gen = false)]
pub struct Color {
    pub r: u8,
}

#[py_compat_methods(stub_gen = false)]
impl Color {
    #[py_attrs]
    #[doc = "Returns the red channel."]
    #[inline]
    #[allow(clippy::unused_self)]
    #[must_use]
    #[deprecated(note = "kept to verify attribute preservation")]
    #[new]
    #[pyo3(signature = (r))]
    pub fn red(&self, #[pyo3(from_py_with = "convert")] r: u8) -> u8 {
        self.r.saturating_add(r)
    }
}

fn main() {}
