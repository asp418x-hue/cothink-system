use super::super::helpers as h;
use crate::class::BridgedClassDef;
use crate::error::InterpError;
use crate::value::Value;

pub fn error_definition() -> BridgedClassDef {
    let mut bc = BridgedClassDef::new("Error");
    bc.constructors.insert(
        "".into(),
        h::ctor(|_v, _args, _n| Ok(h::wrap(&h::class("Error"), Value::Null))),
    );
    bc.static_methods.insert(
        "safeToString".into(),
        h::static_method(|_v, args, _n| {
            let object = args.first().cloned().unwrap_or(Value::Null);
            Ok(Value::str(h::display(&object)))
        }),
    );
    bc.methods.insert(
        "toString".into(),
        h::method(|_v, _t, _a, _n| Ok(Value::str("Error"))),
    );
    bc.getters.insert(
        "stackTrace".into(),
        h::getter(|_v, _t| Ok(h::wrap(&h::class("StackTrace"), Value::str("")))),
    );
    bc.getters.insert(
        "hashCode".into(),
        h::getter(|_v, t| Ok(Value::Int(h::hash_code(&t)))),
    );
    bc.getters.insert(
        "runtimeType".into(),
        h::getter(|_v, t| Ok(h::runtime_type(&t))),
    );
    bc
}

pub fn assertion_error_definition() -> BridgedClassDef {
    let mut bc = BridgedClassDef::new("AssertionError");
    bc.constructors.insert(
        "".into(),
        h::ctor(|_v, args, _n| {
            let message = args.first().cloned().unwrap_or(Value::Null);
            Ok(h::wrap(
                &h::class("AssertionError"),
                Value::List(std::rc::Rc::new(std::cell::RefCell::new(vec![message]))),
            ))
        }),
    );
    bc.methods.insert(
        "toString".into(),
        h::method(|_v, t, _a, _n| {
            let message = h::item_at(&t, 0).unwrap_or(Value::Null);
            let s = match message {
                Value::Null => "Assertion failed".to_string(),
                m => format!("Assertion failed: {}", h::display(&m)),
            };
            Ok(Value::str(s))
        }),
    );
    bc.getters.insert(
        "message".into(),
        h::getter(|_v, t| Ok(h::item_at(&t, 0).unwrap_or(Value::Null))),
    );
    bc.getters.insert(
        "stackTrace".into(),
        h::getter(|_v, _t| Ok(h::wrap(&h::class("StackTrace"), Value::str("")))),
    );
    bc.getters.insert(
        "hashCode".into(),
        h::getter(|_v, t| Ok(Value::Int(h::hash_code(&t)))),
    );
    bc.getters.insert(
        "runtimeType".into(),
        h::getter(|_v, t| Ok(h::runtime_type(&t))),
    );
    bc
}

pub fn type_error_definition() -> BridgedClassDef {
    let mut bc = BridgedClassDef::new("TypeError");
    bc.constructors.insert(
        "".into(),
        h::ctor(|_v, _args, _n| Ok(h::wrap(&h::class("TypeError"), Value::Null))),
    );
    bc.methods.insert(
        "toString".into(),
        h::method(|_v, _t, _a, _n| Ok(Value::str("TypeError"))),
    );
    bc.getters.insert(
        "stackTrace".into(),
        h::getter(|_v, _t| Ok(h::wrap(&h::class("StackTrace"), Value::str("")))),
    );
    bc.getters.insert(
        "hashCode".into(),
        h::getter(|_v, t| Ok(Value::Int(h::hash_code(&t)))),
    );
    bc.getters.insert(
        "runtimeType".into(),
        h::getter(|_v, t| Ok(h::runtime_type(&t))),
    );
    bc
}

fn argument_error_native(message: Value, invalid_value: Value, name: Value) -> Value {
    Value::List(std::rc::Rc::new(std::cell::RefCell::new(vec![
        message,
        invalid_value,
        name,
    ])))
}

pub fn argument_error_definition() -> BridgedClassDef {
    let mut bc = BridgedClassDef::new("ArgumentError");
    bc.constructors.insert(
        "".into(),
        h::ctor(|_v, args, _n| {
            let message = args.first().cloned().unwrap_or(Value::Null);
            Ok(h::wrap(
                &h::class("ArgumentError"),
                argument_error_native(message, Value::Null, Value::Null),
            ))
        }),
    );
    bc.static_methods.insert(
        "checkNotNull".into(),
        h::static_method(|_v, args, _n| {
            let argument = args.first().cloned().unwrap_or(Value::Null);
            let name = args.get(1).and_then(h::as_str).map(|s| s.to_string());
            if argument.is_null() {
                let message = match &name {
                    Some(n) => format!("Must not be null: {n}"),
                    None => "Must not be null".to_string(),
                };
                return Err(InterpError::throw_argument_error(message, Value::Null));
            }
            Ok(argument)
        }),
    );
    bc.static_methods.insert(
        "notNull".into(),
        h::static_method(|_v, args, _n| {
            let name = args.get(1).and_then(h::as_str).map(|s| s.to_string());
            let message = match &name {
                Some(n) => format!("Must not be null: {n}"),
                None => "Must not be null".to_string(),
            };
            Err(InterpError::throw_argument_error(message, Value::Null))
        }),
    );
    bc.static_methods.insert(
        "value".into(),
        h::static_method(|_v, args, _n| {
            let value = args.first().cloned().unwrap_or(Value::Null);
            let name = args.get(1).and_then(h::as_str).map(|s| s.to_string());
            let message = args.get(2).cloned().unwrap_or(Value::Null);
            let message = match message {
                Value::Null => match &name {
                    Some(n) => Value::str(format!("Invalid value ({n}): {}", h::display(&value))),
                    None => Value::str(format!("Invalid value: {}", h::display(&value))),
                },
                m => m,
            };
            Ok(h::wrap(
                &h::class("ArgumentError"),
                argument_error_native(
                    message,
                    value,
                    match &name {
                        Some(n) => Value::str(n),
                        None => Value::Null,
                    },
                ),
            ))
        }),
    );
    bc.methods.insert(
        "toString".into(),
        h::method(|_v, t, _a, _n| {
            let message = h::item_at(&t, 0).unwrap_or(Value::Null);
            Ok(Value::str(match message {
                Value::Null => "Invalid argument(s)".to_string(),
                m => format!("Invalid argument(s): {}", h::display(&m)),
            }))
        }),
    );
    bc.getters.insert(
        "message".into(),
        h::getter(|_v, t| Ok(h::item_at(&t, 0).unwrap_or(Value::Null))),
    );
    bc.getters.insert(
        "invalidValue".into(),
        h::getter(|_v, t| Ok(h::item_at(&t, 1).unwrap_or(Value::Null))),
    );
    bc.getters.insert(
        "name".into(),
        h::getter(|_v, t| Ok(h::item_at(&t, 2).unwrap_or(Value::Null))),
    );
    bc.getters.insert(
        "stackTrace".into(),
        h::getter(|_v, _t| Ok(h::wrap(&h::class("StackTrace"), Value::str("")))),
    );
    bc.getters.insert(
        "hashCode".into(),
        h::getter(|_v, t| Ok(Value::Int(h::hash_code(&t)))),
    );
    bc.getters.insert(
        "runtimeType".into(),
        h::getter(|_v, t| Ok(h::runtime_type(&t))),
    );
    bc
}

pub fn range_error_definition() -> BridgedClassDef {
    let mut bc = BridgedClassDef::new("RangeError");
    bc.constructors.insert(
        "".into(),
        h::ctor(|_v, args, _n| {
            let message = args.first().cloned().unwrap_or(Value::Null);
            Ok(h::wrap(
                &h::class("RangeError"),
                argument_error_native(message, Value::Null, Value::Null),
            ))
        }),
    );
    bc.static_methods.insert(
        "checkValueInInterval".into(),
        h::static_method(|_v, args, _n| {
            let value = h::arg_int(&args, 0, "checkValueInInterval")?;
            let min_value = h::arg_int(&args, 1, "checkValueInInterval")?;
            let max_value = h::arg_int(&args, 2, "checkValueInInterval")?;
            let name = args.get(3).and_then(h::as_str).map(|s| s.to_string());
            let message = args.get(4).and_then(h::as_str).map(|s| s.to_string());
            if value < min_value || value > max_value {
                let message = message.unwrap_or_else(|| match &name {
                    Some(n) => format!("Invalid value: {value} ({n})"),
                    None => format!("Invalid value: {value}"),
                });
                return Err(InterpError::throw_range_error(message, Value::Int(value)));
            }
            Ok(Value::Int(value))
        }),
    );
    bc.static_methods.insert(
        "checkNotNegative".into(),
        h::static_method(|_v, args, _n| {
            let value = h::arg_int(&args, 0, "checkNotNegative")?;
            let name = args.get(1).and_then(h::as_str).map(|s| s.to_string());
            let message = args.get(2).and_then(h::as_str).map(|s| s.to_string());
            if value < 0 {
                let message = message.unwrap_or_else(|| match &name {
                    Some(n) => format!("Must not be negative: {n}"),
                    None => "Must not be negative".to_string(),
                });
                return Err(InterpError::throw_range_error(message, Value::Int(value)));
            }
            Ok(Value::Int(value))
        }),
    );
    bc.static_methods.insert(
        "checkValidRange".into(),
        h::static_method(|_v, args, _n| {
            let start = h::arg_int(&args, 0, "checkValidRange")?;
            let end = args.get(1).and_then(h::as_int);
            let length = h::arg_int(&args, 2, "checkValidRange")?;
            let start_name = args.get(3).and_then(h::as_str).map(|s| s.to_string());
            let end_name = args.get(4).and_then(h::as_str).map(|s| s.to_string());
            let message = args.get(5).and_then(h::as_str).map(|s| s.to_string());
            if start < 0 {
                let message = message.clone().unwrap_or_else(|| match &start_name {
                    Some(n) => format!("Invalid value: {start} ({n})"),
                    None => format!("Invalid value: {start}"),
                });
                return Err(InterpError::throw_range_error(message, Value::Int(start)));
            }
            if let Some(end) = end {
                if end < start {
                    let message = message.clone().unwrap_or_else(|| {
                        format!(
                            "Invalid value: {end} ({} {start}, {length})",
                            end_name.as_deref().unwrap_or("end")
                        )
                    });
                    return Err(InterpError::throw_range_error(message, Value::Int(end)));
                }
                if end > length {
                    let message = message.clone().unwrap_or_else(|| {
                        format!(
                            "Invalid value: {end} ({} {start}, {length})",
                            end_name.as_deref().unwrap_or("end")
                        )
                    });
                    return Err(InterpError::throw_range_error(message, Value::Int(end)));
                }
                Ok(Value::Int(end))
            } else {
                Ok(Value::Int(start))
            }
        }),
    );
    bc.methods.insert(
        "toString".into(),
        h::method(|_v, t, _a, _n| {
            let message = h::item_at(&t, 0).unwrap_or(Value::Null);
            let invalid = h::item_at(&t, 1).unwrap_or(Value::Null);
            let s = match message {
                Value::Null => match invalid {
                    Value::Null => "RangeError".to_string(),
                    i => format!(
                        "RangeError ({}): Invalid value: {}",
                        h::display(&i),
                        h::display(&i)
                    ),
                },
                m => format!("RangeError: {}", h::display(&m)),
            };
            Ok(Value::str(s))
        }),
    );
    bc.getters.insert(
        "message".into(),
        h::getter(|_v, t| Ok(h::item_at(&t, 0).unwrap_or(Value::Null))),
    );
    bc.getters.insert(
        "invalidValue".into(),
        h::getter(|_v, t| Ok(h::item_at(&t, 1).unwrap_or(Value::Null))),
    );
    bc.getters.insert(
        "stackTrace".into(),
        h::getter(|_v, _t| Ok(h::wrap(&h::class("StackTrace"), Value::str("")))),
    );
    bc.getters.insert(
        "hashCode".into(),
        h::getter(|_v, t| Ok(Value::Int(h::hash_code(&t)))),
    );
    bc.getters.insert(
        "runtimeType".into(),
        h::getter(|_v, t| Ok(h::runtime_type(&t))),
    );
    bc
}
