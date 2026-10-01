pub mod bigint;
pub mod collection;
pub mod date_time;
pub mod duration;
pub mod errors;
pub mod exceptions;
pub mod patern;
pub mod primitives;
pub mod regexp;
pub mod stack_trace;
pub mod string;
pub mod uri;

use super::helpers as h;
use crate::callable::NativeFunction;
use crate::env::Environment;
use crate::rt::{FunctionRuntimeType, RuntimeType};
use crate::value::Value;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

fn define_bridge(env: &Rc<RefCell<Environment>>, bc: crate::class::BridgedClassDef) {
    let bc = Arc::new(bc);
    h::register_class(bc.clone());
    Environment::define_bridge(env, bc);
}

pub fn load_core(env: &Rc<RefCell<Environment>>) {
    define_bridge(env, primitives::object_definition());
    define_bridge(env, primitives::null_definition());
    define_bridge(env, primitives::bool_definition());
    define_bridge(env, primitives::num_definition());
    define_bridge(env, primitives::int_definition());
    define_bridge(env, primitives::double_definition());
    define_bridge(env, primitives::comparable_definition());
    define_bridge(env, primitives::function_definition());
    define_bridge(env, primitives::symbol_definition());
    define_bridge(env, primitives::type_definition());
    define_bridge(env, string::string_definition());
    define_bridge(env, string::string_buffer_definition());
    define_bridge(env, string::string_sink_definition());
    define_bridge(env, string::sink_definition());
    define_bridge(env, string::runes_definition());
    define_bridge(env, collection::list_definition());
    define_bridge(env, collection::map_definition());
    define_bridge(env, collection::map_entry_definition());
    define_bridge(env, collection::set_definition());
    define_bridge(env, collection::iterable_definition());
    define_bridge(env, collection::iterator_definition());
    define_bridge(env, patern::pattern_definition());
    define_bridge(env, patern::match_definition());
    define_bridge(env, duration::duration_definition());
    define_bridge(env, date_time::date_time_definition());
    define_bridge(env, stack_trace::stack_trace_definition());
    define_bridge(env, errors::error_definition());
    define_bridge(env, errors::assertion_error_definition());
    define_bridge(env, errors::type_error_definition());
    define_bridge(env, errors::argument_error_definition());
    define_bridge(env, errors::range_error_definition());
    define_bridge(env, exceptions::exception_definition());
    define_bridge(env, exceptions::format_exception_definition());
    define_bridge(env, regexp::regexp_definition());
    define_bridge(env, regexp::regexp_match_definition());
    define_bridge(env, uri::uri_definition());
    define_bridge(env, bigint::bigint_definition());

    let dynamic_fn = NativeFunction {
        name: Some("dynamic".to_string()),
        min_arity: 0,
        max_arity: Some(0),
        is_async: false,
        is_generator: false,
        closure: Box::new(|_v, _a, _n| Ok(Value::Type(RuntimeType::dynamic()))),
        callable_runtime_type: FunctionRuntimeType::untyped(),
    };
    Environment::define(env, "dynamic", Value::NativeFn(Rc::new(dynamic_fn)));

    let identical_fn = NativeFunction {
        name: Some("identical".to_string()),
        min_arity: 2,
        max_arity: Some(2),
        is_async: false,
        is_generator: false,
        closure: Box::new(|_v, args, _n| {
            let a = args.first().cloned().unwrap_or(Value::Null);
            let b = args.get(1).cloned().unwrap_or(Value::Null);
            Ok(Value::Bool(identical_values(&a, &b)))
        }),
        callable_runtime_type: FunctionRuntimeType::untyped(),
    };
    Environment::define(env, "identical", Value::NativeFn(Rc::new(identical_fn)));
}

fn identical_values(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Int(x), Value::Int(y)) => x == y,
        (Value::Double(x), Value::Double(y)) => x.to_bits() == y.to_bits(),
        (Value::Bool(x), Value::Bool(y)) => x == y,
        (Value::Str(x), Value::Str(y)) => x == y,
        (Value::Null, Value::Null) => true,
        _ => match (a.ptr(), b.ptr()) {
            (Some(x), Some(y)) => x == y,
            _ => false,
        },
    }
}
