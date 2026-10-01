use super::super::helpers as h;
use crate::class::BridgedClassDef;
use crate::value::Value;

pub fn stack_trace_definition() -> BridgedClassDef {
    let mut bc = BridgedClassDef::new("StackTrace");
    bc.static_getters.insert(
        "current".into(),
        h::static_getter(|_v| {
            Ok(h::wrap(
                &h::class("StackTrace"),
                Value::str("d4rt stack trace"),
            ))
        }),
    );
    bc.static_getters.insert(
        "empty".into(),
        h::static_getter(|_v| Ok(h::wrap(&h::class("StackTrace"), Value::str("")))),
    );
    bc.methods.insert(
        "toString".into(),
        h::method(|_v, t, _a, _n| Ok(Value::str(h::as_str(&t).unwrap_or_default()))),
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
