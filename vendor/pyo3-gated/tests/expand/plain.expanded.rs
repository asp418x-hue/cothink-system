use pyo3_gated::prelude::*;
#[allow(unexpected_cfgs)]
pub struct Color {
    pub r: u8,
}
#[allow(unexpected_cfgs)]
impl Color {
    pub fn new(r: u8) -> Self {
        Self { r }
    }
    pub fn raw(self) -> u8 {
        self.r
    }
}
#[allow(unexpected_cfgs)]
pub fn add(a: i32, b: i32) -> i32 {
    a + b
}
fn main() {}
