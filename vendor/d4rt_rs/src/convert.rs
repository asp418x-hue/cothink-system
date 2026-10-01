use std::cell::RefCell;
use std::rc::Rc;

use crate::error::InterpError;
use crate::value::{Value, value_display};

/// Converts a Rust value into an interpreter [`Value`].
///
/// Mirrors QuickJS's `IntoJs`/`IntoValue` pattern: native functions
/// registered through [`crate::Context::register_function`] use this to
/// produce results.
pub trait ToValue {
    fn to_value(self) -> Value;
}

/// Extracts a typed Rust value from an interpreter [`Value`].
///
/// Mirrors QuickJS's `FromJs`/`FromValue` pattern. Fails with a runtime
/// error when the value has an incompatible type.
pub trait FromValue: Sized {
    fn from_value(value: &Value) -> Result<Self, InterpError>;
}

fn type_error(expected: &str, value: &Value) -> InterpError {
    InterpError::runtime(format!("Expected {expected}, got {}", value_display(value)))
}

// ---------------------------------------------------------------------------
// ToValue impls
// ---------------------------------------------------------------------------

impl ToValue for Value {
    fn to_value(self) -> Value {
        self
    }
}

impl ToValue for () {
    fn to_value(self) -> Value {
        Value::Null
    }
}

impl ToValue for i64 {
    fn to_value(self) -> Value {
        Value::Int(self)
    }
}

impl ToValue for i32 {
    fn to_value(self) -> Value {
        Value::Int(self as i64)
    }
}

impl ToValue for f64 {
    fn to_value(self) -> Value {
        Value::Double(self)
    }
}

impl ToValue for bool {
    fn to_value(self) -> Value {
        Value::Bool(self)
    }
}

impl ToValue for String {
    fn to_value(self) -> Value {
        Value::str(self)
    }
}

impl ToValue for &str {
    fn to_value(self) -> Value {
        Value::str(self)
    }
}

impl ToValue for Vec<Value> {
    fn to_value(self) -> Value {
        Value::List(Rc::new(RefCell::new(self)))
    }
}

impl ToValue for Vec<i64> {
    fn to_value(self) -> Value {
        Value::List(Rc::new(RefCell::new(
            self.into_iter().map(Value::Int).collect(),
        )))
    }
}

// ---------------------------------------------------------------------------
// FromValue impls
// ---------------------------------------------------------------------------

impl FromValue for Value {
    fn from_value(value: &Value) -> Result<Self, InterpError> {
        Ok(value.clone())
    }
}

impl FromValue for i64 {
    fn from_value(value: &Value) -> Result<Self, InterpError> {
        value.as_int().ok_or_else(|| type_error("int", value))
    }
}

impl FromValue for f64 {
    fn from_value(value: &Value) -> Result<Self, InterpError> {
        value
            .num_to_f64()
            .ok_or_else(|| type_error("double", value))
    }
}

impl FromValue for bool {
    fn from_value(value: &Value) -> Result<Self, InterpError> {
        value.as_bool().ok_or_else(|| type_error("bool", value))
    }
}

impl FromValue for String {
    fn from_value(value: &Value) -> Result<Self, InterpError> {
        value
            .as_str()
            .map(str::to_string)
            .ok_or_else(|| type_error("string", value))
    }
}

impl FromValue for Vec<Value> {
    fn from_value(value: &Value) -> Result<Self, InterpError> {
        match value {
            Value::List(l) => Ok(l.borrow().clone()),
            Value::Bridged(b) => Vec::<Value>::from_value(&b.borrow().native),
            other => Err(type_error("list", other)),
        }
    }
}
