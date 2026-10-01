use super::super::helpers as h;
use crate::class::BridgedClassDef;
use crate::value::Value;

pub fn exception_definition() -> BridgedClassDef {
    let mut bc = BridgedClassDef::new("Exception");
    bc.constructors.insert(
        "".into(),
        h::ctor(|_v, args, _n| {
            let message = args.first().cloned().unwrap_or(Value::Null);
            Ok(h::wrap(
                &h::class("Exception"),
                Value::List(std::rc::Rc::new(std::cell::RefCell::new(vec![message]))),
            ))
        }),
    );
    bc.methods.insert(
        "toString".into(),
        h::method(|_v, t, _a, _n| {
            let message = h::item_at(&t, 0).unwrap_or(Value::Null);
            Ok(Value::str(match message {
                Value::Null => "Exception".to_string(),
                m => format!("Exception: {}", h::display(&m)),
            }))
        }),
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

pub fn format_exception_definition() -> BridgedClassDef {
    let mut bc = BridgedClassDef::new("FormatException");
    bc.constructors.insert(
        "".into(),
        h::ctor(|_v, args, named| {
            let message = args
                .first()
                .map(|v| h::as_str(v).unwrap_or_default().to_string())
                .unwrap_or_default();
            let source = args
                .get(1)
                .cloned()
                .or_else(|| named.get("source").cloned())
                .unwrap_or(Value::Null);
            let offset = args
                .get(2)
                .and_then(h::as_int)
                .or_else(|| named.get("offset").and_then(h::as_int))
                .map(Value::Int)
                .unwrap_or(Value::Null);
            Ok(h::wrap(
                &h::class("FormatException"),
                Value::List(std::rc::Rc::new(std::cell::RefCell::new(vec![
                    Value::str(message),
                    source,
                    offset,
                ]))),
            ))
        }),
    );
    bc.methods.insert(
        "toString".into(),
        h::method(|_v, t, _a, _n| {
            let msg = h::item_at(&t, 0).unwrap_or(Value::Null);
            let message = h::as_str(&msg).unwrap_or_default();
            let offset = h::item_at(&t, 2).and_then(|v| h::as_int(&v));
            let s = match offset {
                Some(o) => format!("FormatException: {message} (at offset {o})"),
                None => format!("FormatException: {message}"),
            };
            Ok(Value::str(s))
        }),
    );
    bc.getters.insert(
        "message".into(),
        h::getter(|_v, t| Ok(h::item_at(&t, 0).unwrap_or(Value::Null))),
    );
    bc.getters.insert(
        "source".into(),
        h::getter(|_v, t| Ok(h::item_at(&t, 1).unwrap_or(Value::Null))),
    );
    bc.getters.insert(
        "offset".into(),
        h::getter(|_v, t| Ok(h::item_at(&t, 2).unwrap_or(Value::Null))),
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
