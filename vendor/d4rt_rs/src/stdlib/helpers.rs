#![allow(dead_code)]

use crate::class::{BridgedClassDef, BridgedInstanceData};
use crate::error::InterpError;
use crate::interp::InterpreterVisitor;
use crate::rt::{RuntimeType, runtime_type_of};
use crate::value::{Value, value_display};
use num_traits::ToPrimitive;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

pub type M = Box<
    dyn Fn(
            &mut InterpreterVisitor,
            Value,
            Vec<Value>,
            HashMap<String, Value>,
        ) -> Result<Value, InterpError>
        + Send
        + Sync,
>;
pub type G =
    Box<dyn Fn(&mut InterpreterVisitor, Value) -> Result<Value, InterpError> + Send + Sync>;
pub type S =
    Box<dyn Fn(&mut InterpreterVisitor, Value, Value) -> Result<(), InterpError> + Send + Sync>;
pub type SM = Box<
    dyn Fn(
            &mut InterpreterVisitor,
            Vec<Value>,
            HashMap<String, Value>,
        ) -> Result<Value, InterpError>
        + Send
        + Sync,
>;
pub type SG = Box<dyn Fn(&mut InterpreterVisitor) -> Result<Value, InterpError> + Send + Sync>;
pub type C = Box<
    dyn Fn(
            &mut InterpreterVisitor,
            Vec<Value>,
            HashMap<String, Value>,
        ) -> Result<Value, InterpError>
        + Send
        + Sync,
>;

pub fn method<F>(f: F) -> M
where
    F: Fn(
            &mut InterpreterVisitor,
            Value,
            Vec<Value>,
            HashMap<String, Value>,
        ) -> Result<Value, InterpError>
        + Send
        + Sync
        + 'static,
{
    Box::new(f)
}

pub fn getter<F>(f: F) -> G
where
    F: Fn(&mut InterpreterVisitor, Value) -> Result<Value, InterpError> + Send + Sync + 'static,
{
    Box::new(f)
}

pub fn setter<F>(f: F) -> S
where
    F: Fn(&mut InterpreterVisitor, Value, Value) -> Result<(), InterpError> + Send + Sync + 'static,
{
    Box::new(f)
}

pub fn static_method<F>(f: F) -> SM
where
    F: Fn(
            &mut InterpreterVisitor,
            Vec<Value>,
            HashMap<String, Value>,
        ) -> Result<Value, InterpError>
        + Send
        + Sync
        + 'static,
{
    Box::new(f)
}

pub fn static_getter<F>(f: F) -> SG
where
    F: Fn(&mut InterpreterVisitor) -> Result<Value, InterpError> + Send + Sync + 'static,
{
    Box::new(f)
}

pub fn ctor<F>(f: F) -> C
where
    F: Fn(
            &mut InterpreterVisitor,
            Vec<Value>,
            HashMap<String, Value>,
        ) -> Result<Value, InterpError>
        + Send
        + Sync
        + 'static,
{
    Box::new(f)
}

pub fn runtime_type(v: &Value) -> Value {
    Value::Type(runtime_type_of(Some(v)).unwrap_or_else(|| RuntimeType::named("dynamic")))
}

pub fn err(msg: impl Into<String>) -> InterpError {
    InterpError::runtime(msg)
}

pub fn as_str(v: &Value) -> Option<&str> {
    match v {
        Value::Str(s) => Some(s),
        Value::Bridged(b) => match &b.borrow().native {
            Value::Str(s) => Some(unsafe { &*Rc::as_ptr(s) }),
            _ => None,
        },
        _ => None,
    }
}

pub fn as_int(v: &Value) -> Option<i64> {
    match v {
        Value::Int(i) => Some(*i),
        Value::Bridged(b) => as_int(&b.borrow().native),
        _ => None,
    }
}

pub fn as_double(v: &Value) -> Option<f64> {
    match v {
        Value::Int(i) => Some(*i as f64),
        Value::Double(d) => Some(*d),
        Value::Bridged(b) => as_double(&b.borrow().native),
        _ => None,
    }
}

pub fn as_bool(v: &Value) -> Option<bool> {
    match v {
        Value::Bool(b) => Some(*b),
        Value::Bridged(b) => as_bool(&b.borrow().native),
        _ => None,
    }
}

pub fn as_bigint(v: &Value) -> Option<num_bigint::BigInt> {
    match v {
        Value::BigInt(b) => Some(b.clone()),
        Value::Int(i) => Some(num_bigint::BigInt::from(*i)),
        Value::Bridged(b) => as_bigint(&b.borrow().native),
        _ => None,
    }
}

pub fn as_list(v: &Value) -> Option<Rc<RefCell<Vec<Value>>>> {
    match v {
        Value::List(l) => Some(l.clone()),
        Value::Bridged(b) => as_list(&b.borrow().native),
        _ => None,
    }
}

pub fn as_map(v: &Value) -> Option<Rc<RefCell<crate::value::DartMap>>> {
    match v {
        Value::Map(m) => Some(m.clone()),
        Value::Bridged(b) => as_map(&b.borrow().native),
        _ => None,
    }
}

pub fn as_set(v: &Value) -> Option<Rc<RefCell<crate::value::DartSet>>> {
    match v {
        Value::Set(s) => Some(s.clone()),
        Value::Bridged(b) => as_set(&b.borrow().native),
        _ => None,
    }
}

pub fn as_strbuf(v: &Value) -> Option<Rc<RefCell<String>>> {
    match v {
        Value::StringBuffer(s) => Some(s.clone()),
        Value::Bridged(b) => as_strbuf(&b.borrow().native),
        _ => None,
    }
}

pub fn arg_str<'a>(v: &'a [Value], i: usize, name: &str) -> Result<&'a str, InterpError> {
    v.get(i)
        .and_then(as_str)
        .ok_or_else(|| err(format!("Expected a String argument for '{name}'")))
}

pub fn arg_int(v: &[Value], i: usize, name: &str) -> Result<i64, InterpError> {
    v.get(i)
        .and_then(as_int)
        .ok_or_else(|| err(format!("Expected an int argument for '{name}'")))
}

pub fn arg_double(v: &[Value], i: usize, name: &str) -> Result<f64, InterpError> {
    v.get(i)
        .and_then(as_double)
        .ok_or_else(|| err(format!("Expected a num argument for '{name}'")))
}

pub fn arg_bool(v: &[Value], i: usize, name: &str) -> Result<bool, InterpError> {
    v.get(i)
        .and_then(as_bool)
        .ok_or_else(|| err(format!("Expected a bool argument for '{name}'")))
}

pub fn item_at(v: &Value, i: usize) -> Option<Value> {
    let l = as_list(v)?;
    l.borrow().get(i).cloned()
}

pub fn require_args(args: &[Value], min: usize, name: &str) -> Result<(), InterpError> {
    if args.len() < min {
        Err(err(format!("{name} expects at least {min} argument(s)")))
    } else {
        Ok(())
    }
}

pub fn wrap(bc: &Arc<BridgedClassDef>, native: Value) -> Value {
    Value::Bridged(Rc::new(RefCell::new(BridgedInstanceData {
        bridged_class: bc.clone(),
        native,
        type_arguments: vec![],
        bridged_super_object: None,
    })))
}

pub fn register_class(bc: Arc<BridgedClassDef>) {
    crate::state::current()
        .bridged
        .borrow_mut()
        .insert(bc.name.clone(), bc);
}

pub fn class(name: &str) -> Arc<BridgedClassDef> {
    crate::state::current()
        .bridged
        .borrow()
        .get(name)
        .cloned()
        .unwrap_or_else(|| panic!("Bridged class '{name}' not registered"))
}

pub fn format_exception(message: impl Into<String>) -> InterpError {
    let message = message.into();
    InterpError::Throw(Value::Bridged(Rc::new(RefCell::new(BridgedInstanceData {
        bridged_class: class("FormatException"),
        native: Value::List(Rc::new(RefCell::new(vec![
            Value::str(message),
            Value::Null,
            Value::Null,
        ]))),
        type_arguments: vec![],
        bridged_super_object: None,
    }))))
}

pub fn call_closure(
    visitor: &mut InterpreterVisitor,
    v: &Value,
    args: Vec<Value>,
) -> Result<Value, InterpError> {
    visitor.call_value(v, args, HashMap::new())
}

pub fn display(v: &Value) -> String {
    value_display(v)
}

pub fn str(v: impl Into<String>) -> Value {
    Value::str(v)
}

pub fn hash_code(v: &Value) -> i64 {
    use std::hash::{Hash, Hasher};
    match v {
        Value::Null => 0,
        Value::Bool(true) => 1231,
        Value::Bool(false) => 1237,
        Value::Int(i) => *i,
        Value::Double(d) => d.to_bits() as i64,
        Value::Str(s) => {
            let mut h = std::collections::hash_map::DefaultHasher::new();
            s.hash(&mut h);
            h.finish() as i64
        }
        Value::BigInt(b) => match b.to_i64() {
            Some(i) => i,
            None => {
                let mut h = std::collections::hash_map::DefaultHasher::new();
                b.to_string().hash(&mut h);
                h.finish() as i64
            }
        },
        Value::Bridged(b) => hash_code(&b.borrow().native),
        other => match other {
            Value::List(l) => Rc::as_ptr(l) as i64,
            Value::Map(m) => Rc::as_ptr(m) as i64,
            Value::Set(s) => Rc::as_ptr(s) as i64,
            Value::StringBuffer(s) => Rc::as_ptr(s) as i64,
            Value::Instance(i) => Rc::as_ptr(i) as i64,
            Value::Function(f) => Rc::as_ptr(f) as i64,
            Value::NativeFn(f) => Rc::as_ptr(f) as i64,
            Value::Class(c) => Rc::as_ptr(c) as i64,
            _ => 0,
        },
    }
}
