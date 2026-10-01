use super::super::helpers as h;
use crate::class::BridgedClassDef;
use crate::error::InterpError;
use crate::interp::InterpreterVisitor;
use crate::rt::{RuntimeType, runtime_type_of};
use crate::value::{
    Value, fmt_double, num_add, num_div, num_mul, num_rem, num_sub, num_trunc_div, value_display,
    value_eq,
};
use num_integer::Integer;
use std::collections::HashMap;

type M = Box<
    dyn Fn(
            &mut InterpreterVisitor,
            Value,
            Vec<Value>,
            HashMap<String, Value>,
        ) -> Result<Value, InterpError>
        + Send
        + Sync,
>;
type G = Box<dyn Fn(&mut InterpreterVisitor, Value) -> Result<Value, InterpError> + Send + Sync>;
#[allow(dead_code)]
type S =
    Box<dyn Fn(&mut InterpreterVisitor, Value, Value) -> Result<(), InterpError> + Send + Sync>;
type SM = Box<
    dyn Fn(
            &mut InterpreterVisitor,
            Vec<Value>,
            HashMap<String, Value>,
        ) -> Result<Value, InterpError>
        + Send
        + Sync,
>;
#[allow(dead_code)]
type SG = Box<dyn Fn(&mut InterpreterVisitor) -> Result<Value, InterpError> + Send + Sync>;
type C = Box<
    dyn Fn(
            &mut InterpreterVisitor,
            Vec<Value>,
            HashMap<String, Value>,
        ) -> Result<Value, InterpError>
        + Send
        + Sync,
>;

fn method<F>(f: F) -> M
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

fn getter<F>(f: F) -> G
where
    F: Fn(&mut InterpreterVisitor, Value) -> Result<Value, InterpError> + Send + Sync + 'static,
{
    Box::new(f)
}

#[allow(dead_code)]
fn setter<F>(f: F) -> S
where
    F: Fn(&mut InterpreterVisitor, Value, Value) -> Result<(), InterpError> + Send + Sync + 'static,
{
    Box::new(f)
}

fn static_method<F>(f: F) -> SM
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

#[allow(dead_code)]
fn static_getter<F>(f: F) -> SG
where
    F: Fn(&mut InterpreterVisitor) -> Result<Value, InterpError> + Send + Sync + 'static,
{
    Box::new(f)
}

fn ctor<F>(f: F) -> C
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

fn runtime_type(v: &Value) -> Value {
    Value::Type(runtime_type_of(Some(v)).unwrap_or_else(|| RuntimeType::named("dynamic")))
}

pub fn object_definition() -> BridgedClassDef {
    let mut bc = BridgedClassDef::new("Object");
    bc.methods.insert(
        "==".into(),
        method(|_v, target, args, _n| {
            let other = args.first().cloned().unwrap_or(Value::Null);
            Ok(Value::Bool(value_eq(&target, &other)))
        }),
    );
    bc.methods.insert(
        "toString".into(),
        method(|_v, target, _a, _n| Ok(Value::str(value_display(&target)))),
    );
    bc.getters.insert(
        "hashCode".into(),
        getter(|_v, t| Ok(Value::Int(h::hash_code(&t)))),
    );
    bc.getters
        .insert("runtimeType".into(), getter(|_v, t| Ok(runtime_type(&t))));
    bc
}

pub fn null_definition() -> BridgedClassDef {
    let mut bc = BridgedClassDef::new("Null");
    bc.native_type = Some("Null".into());
    bc.methods.insert(
        "toString".into(),
        method(|_v, _t, _a, _n| Ok(Value::str("null"))),
    );
    bc.getters
        .insert("hashCode".into(), getter(|_v, _t| Ok(Value::Int(0))));
    bc
}

pub fn bool_definition() -> BridgedClassDef {
    let mut bc = BridgedClassDef::new("bool");
    bc.native_type = Some("bool".into());
    bc.static_methods.insert(
        "fromEnvironment".into(),
        static_method(|_v, args, named| {
            let name = h::arg_str(&args, 0, "fromEnvironment")?;
            let default = named
                .get("defaultValue")
                .and_then(h::as_bool)
                .unwrap_or(false);
            Ok(Value::Bool(
                std::env::var(name).is_ok_and(|s| s == "true") || default,
            ))
        }),
    );
    bc.static_methods.insert(
        "hasEnvironment".into(),
        static_method(|_v, args, _n| {
            let name = h::arg_str(&args, 0, "hasEnvironment")?;
            Ok(Value::Bool(std::env::var(name).is_ok()))
        }),
    );
    bc.static_methods.insert(
        "parse".into(),
        static_method(|_v, args, named| {
            let s = h::arg_str(&args, 0, "parse")?;
            let case_sensitive = named
                .get("caseSensitive")
                .and_then(h::as_bool)
                .unwrap_or(true);
            if case_sensitive && (s == "true" || s == "false") {
                Ok(Value::Bool(s == "true"))
            } else if !case_sensitive
                && (s.eq_ignore_ascii_case("true") || s.eq_ignore_ascii_case("false"))
            {
                Ok(Value::Bool(s.eq_ignore_ascii_case("true")))
            } else {
                Err(h::format_exception(format!("Invalid boolean: {s}")))
            }
        }),
    );
    bc.static_methods.insert(
        "tryParse".into(),
        static_method(|_v, args, named| {
            let s = h::arg_str(&args, 0, "tryParse")?;
            let case_sensitive = named
                .get("caseSensitive")
                .and_then(h::as_bool)
                .unwrap_or(true);
            if case_sensitive && (s == "true" || s == "false") {
                Ok(Value::Bool(s == "true"))
            } else if !case_sensitive
                && (s.eq_ignore_ascii_case("true") || s.eq_ignore_ascii_case("false"))
            {
                Ok(Value::Bool(s.eq_ignore_ascii_case("true")))
            } else {
                Ok(Value::Null)
            }
        }),
    );
    bc.methods.insert(
        "toString".into(),
        method(|_v, target, _a, _n| {
            Ok(Value::str(h::as_bool(&target).unwrap_or(false).to_string()))
        }),
    );
    bc.getters.insert(
        "hashCode".into(),
        getter(|_v, t| Ok(Value::Int(h::hash_code(&t)))),
    );
    bc.getters
        .insert("runtimeType".into(), getter(|_v, t| Ok(runtime_type(&t))));
    bc
}

fn num_to_string_as_fixed(x: f64, digits: i64) -> Result<String, InterpError> {
    if digits < 0 {
        return Err(h::err(format!(
            "toStringAsFixed: illegal fraction digits {digits}"
        )));
    }
    Ok(format!("{x:.digits$}", digits = digits as usize))
}

fn num_to_string_as_precision(x: f64, precision: i64) -> Result<String, InterpError> {
    if precision < 1 {
        return Err(h::err(format!(
            "toStringAsPrecision: illegal precision {precision}"
        )));
    }
    if x == 0.0 {
        return Ok(format!(
            "0.{}",
            "0".repeat((precision as usize).saturating_sub(1))
        ));
    }
    Ok(format!("{x:.precision$e}", precision = precision as usize))
}

fn num_to_string_as_exponential(x: f64, digits: Option<i64>) -> Result<String, InterpError> {
    match digits {
        Some(d) if d < 0 => Err(h::err(format!(
            "toStringAsExponential: illegal fraction digits {d}"
        ))),
        Some(d) => Ok(format!("{x:.d$e}", d = d as usize)),
        None => Ok(format!("{x:e}")),
    }
}

fn int_to_string_as_fixed(x: i64, digits: i64) -> Result<String, InterpError> {
    if digits < 0 {
        return Err(h::err(format!(
            "toStringAsFixed: illegal fraction digits {digits}"
        )));
    }
    if digits == 0 {
        return Ok(x.to_string());
    }
    Ok(format!("{x}.{}", "0".repeat(digits as usize)))
}

fn parse_num(s: &str) -> Option<Value> {
    let t = s.trim();
    if t.is_empty() {
        return None;
    }
    if t.contains('.') || t.contains('e') || t.contains('E') || t == "NaN" || t.contains("Infinity")
    {
        t.parse::<f64>().ok().map(Value::Double)
    } else {
        t.parse::<i64>().ok().map(Value::Int)
    }
}

pub fn num_definition() -> BridgedClassDef {
    let mut bc = BridgedClassDef::new("num");
    bc.is_subtype_of_fn = Some(Box::new(|v| matches!(v, Value::Int(_) | Value::Double(_))));
    bc.static_methods.insert(
        "parse".into(),
        static_method(|_v, args, _n| {
            let s = h::arg_str(&args, 0, "parse")?;
            parse_num(s).ok_or_else(|| h::format_exception(format!("Invalid number: {s}")))
        }),
    );
    bc.static_methods.insert(
        "tryParse".into(),
        static_method(|_v, args, _n| {
            let s = h::arg_str(&args, 0, "tryParse")?;
            Ok(parse_num(s).unwrap_or(Value::Null))
        }),
    );
    let num_methods: Vec<(&str, M)> = vec![
        (
            "abs",
            method(|_v, t, _a, _n| match t {
                Value::Int(i) => Ok(Value::Int(i.wrapping_abs())),
                Value::Double(d) => Ok(Value::Double(d.abs())),
                _ => Err(h::err("abs: expected a number")),
            }),
        ),
        (
            "ceil",
            method(|_v, t, _a, _n| match t {
                Value::Int(i) => Ok(Value::Int(i)),
                Value::Double(d) => Ok(Value::Int(d.ceil() as i64)),
                _ => Err(h::err("ceil: expected a number")),
            }),
        ),
        (
            "floor",
            method(|_v, t, _a, _n| match t {
                Value::Int(i) => Ok(Value::Int(i)),
                Value::Double(d) => Ok(Value::Int(d.floor() as i64)),
                _ => Err(h::err("floor: expected a number")),
            }),
        ),
        (
            "round",
            method(|_v, t, _a, _n| match t {
                Value::Int(i) => Ok(Value::Int(i)),
                Value::Double(d) => Ok(Value::Int(d.round() as i64)),
                _ => Err(h::err("round: expected a number")),
            }),
        ),
        (
            "truncate",
            method(|_v, t, _a, _n| match t {
                Value::Int(i) => Ok(Value::Int(i)),
                Value::Double(d) => Ok(Value::Int(d.trunc() as i64)),
                _ => Err(h::err("truncate: expected a number")),
            }),
        ),
        (
            "ceilToDouble",
            method(|_v, t, _a, _n| match t {
                Value::Int(i) => Ok(Value::Double(i as f64)),
                Value::Double(d) => Ok(Value::Double(d.ceil())),
                _ => Err(h::err("ceilToDouble: expected a number")),
            }),
        ),
        (
            "floorToDouble",
            method(|_v, t, _a, _n| match t {
                Value::Int(i) => Ok(Value::Double(i as f64)),
                Value::Double(d) => Ok(Value::Double(d.floor())),
                _ => Err(h::err("floorToDouble: expected a number")),
            }),
        ),
        (
            "roundToDouble",
            method(|_v, t, _a, _n| match t {
                Value::Int(i) => Ok(Value::Double(i as f64)),
                Value::Double(d) => Ok(Value::Double(d.round())),
                _ => Err(h::err("roundToDouble: expected a number")),
            }),
        ),
        (
            "truncateToDouble",
            method(|_v, t, _a, _n| match t {
                Value::Int(i) => Ok(Value::Double(i as f64)),
                Value::Double(d) => Ok(Value::Double(d.trunc())),
                _ => Err(h::err("truncateToDouble: expected a number")),
            }),
        ),
        (
            "toDouble",
            method(|_v, t, _a, _n| match t {
                Value::Int(i) => Ok(Value::Double(i as f64)),
                Value::Double(d) => Ok(Value::Double(d)),
                _ => Err(h::err("toDouble: expected a number")),
            }),
        ),
        (
            "toInt",
            method(|_v, t, _a, _n| match t {
                Value::Int(i) => Ok(Value::Int(i)),
                Value::Double(d) => Ok(Value::Int(d.trunc() as i64)),
                _ => Err(h::err("toInt: expected a number")),
            }),
        ),
        (
            "toString",
            method(|_v, t, _a, _n| {
                Ok(Value::str(match t {
                    Value::Int(i) => i.to_string(),
                    Value::Double(d) => fmt_double(d),
                    _ => value_display(&t),
                }))
            }),
        ),
        (
            "toStringAsFixed",
            method(|_v, t, args, _n| {
                let digits = h::arg_int(&args, 0, "toStringAsFixed")?;
                let s = match t {
                    Value::Int(i) => int_to_string_as_fixed(i, digits)?,
                    Value::Double(d) => num_to_string_as_fixed(d, digits)?,
                    _ => return Err(h::err("toStringAsFixed: expected a number")),
                };
                Ok(Value::str(s))
            }),
        ),
        (
            "toStringAsExponential",
            method(|_v, t, args, _n| {
                let digits = args.first().and_then(h::as_int);
                let s = match t {
                    Value::Int(i) => num_to_string_as_exponential(i as f64, digits)?,
                    Value::Double(d) => num_to_string_as_exponential(d, digits)?,
                    _ => return Err(h::err("toStringAsExponential: expected a number")),
                };
                Ok(Value::str(s))
            }),
        ),
        (
            "toStringAsPrecision",
            method(|_v, t, args, _n| {
                let precision = h::arg_int(&args, 0, "toStringAsPrecision")?;
                let s = match t {
                    Value::Int(i) => num_to_string_as_precision(i as f64, precision)?,
                    Value::Double(d) => num_to_string_as_precision(d, precision)?,
                    _ => return Err(h::err("toStringAsPrecision: expected a number")),
                };
                Ok(Value::str(s))
            }),
        ),
        (
            "compareTo",
            method(|_v, t, args, _n| {
                let other = args
                    .first()
                    .ok_or_else(|| h::err("compareTo requires an argument"))?;
                let (a, b) = (h::as_double(&t), h::as_double(other));
                match (a, b) {
                    (Some(a), Some(b)) => Ok(Value::Int(match a.partial_cmp(&b) {
                        Some(std::cmp::Ordering::Less) => -1,
                        Some(std::cmp::Ordering::Equal) => 0,
                        Some(std::cmp::Ordering::Greater) => 1,
                        None => 0,
                    })),
                    _ => Err(h::err("compareTo: expected a num")),
                }
            }),
        ),
        (
            "clamp",
            method(|_v, t, args, _n| {
                let (lo, hi) = (args.first(), args.get(1));
                let (Some(lo), Some(hi)) = (lo, hi) else {
                    return Err(h::err("clamp requires two arguments"));
                };
                let (x, lo, hi) = (
                    h::as_double(&t).ok_or_else(|| h::err("clamp: expected a num"))?,
                    h::as_double(lo).ok_or_else(|| h::err("clamp: expected a num"))?,
                    h::as_double(hi).ok_or_else(|| h::err("clamp: expected a num"))?,
                );
                let result = if x < lo {
                    lo
                } else if x > hi {
                    hi
                } else {
                    x
                };
                if matches!(t, Value::Int(_)) && result == result.trunc() {
                    Ok(Value::Int(result as i64))
                } else {
                    Ok(Value::Double(result))
                }
            }),
        ),
        (
            "remainder",
            method(|_v, t, args, _n| {
                let other = args
                    .first()
                    .ok_or_else(|| h::err("remainder requires an argument"))?;
                let (a, b) = (h::as_double(&t), h::as_double(other));
                match (a, b) {
                    (Some(a), Some(b)) => Ok(Value::Double(a.rem_euclid(b))),
                    _ => Err(h::err("remainder: expected a num")),
                }
            }),
        ),
        (
            "+",
            method(|_v, t, args, _n| {
                let other = args.first().cloned().unwrap_or(Value::Null);
                num_add(&t, &other).ok_or_else(|| h::err("Invalid operand for +"))
            }),
        ),
        (
            "-",
            method(|_v, t, args, _n| {
                let other = args.first().cloned().unwrap_or(Value::Null);
                num_sub(&t, &other).ok_or_else(|| h::err("Invalid operand for -"))
            }),
        ),
        (
            "*",
            method(|_v, t, args, _n| {
                let other = args.first().cloned().unwrap_or(Value::Null);
                num_mul(&t, &other).ok_or_else(|| h::err("Invalid operand for *"))
            }),
        ),
        (
            "/",
            method(|_v, t, args, _n| {
                let other = args.first().cloned().unwrap_or(Value::Null);
                num_div(&t, &other).ok_or_else(|| h::err("Invalid operand for /"))
            }),
        ),
        (
            "~/",
            method(|_v, t, args, _n| {
                let other = args.first().cloned().unwrap_or(Value::Null);
                match num_trunc_div(&t, &other) {
                    Some(Ok(v)) => Ok(v),
                    Some(Err(e)) => Err(e),
                    None => Err(h::err("Invalid operand for ~/")),
                }
            }),
        ),
        (
            "%",
            method(|_v, t, args, _n| {
                let other = args.first().cloned().unwrap_or(Value::Null);
                match num_rem(&t, &other) {
                    Some(Ok(v)) => Ok(v),
                    Some(Err(e)) => Err(e),
                    None => Err(h::err("Invalid operand for %")),
                }
            }),
        ),
        (
            "unary-",
            method(|_v, t, _a, _n| match t {
                Value::Int(i) => Ok(Value::Int(i.wrapping_neg())),
                Value::Double(d) => Ok(Value::Double(-d)),
                _ => Err(h::err("unary-: expected a number")),
            }),
        ),
    ];
    for (k, m) in num_methods {
        bc.methods.insert(k.into(), m);
    }
    bc.getters.insert(
        "hashCode".into(),
        getter(|_v, t| Ok(Value::Int(h::hash_code(&t)))),
    );
    bc.getters
        .insert("runtimeType".into(), getter(|_v, t| Ok(runtime_type(&t))));
    bc.getters.insert(
        "sign".into(),
        getter(|_v, t| match t {
            Value::Int(i) => Ok(Value::Int(if i < 0 { -1 } else { 1 })),
            Value::Double(d) => Ok(Value::Double(if d.is_nan() {
                f64::NAN
            } else if d > 0.0 || (d == 0.0 && d.is_sign_positive()) {
                1.0
            } else {
                -1.0
            })),
            _ => Err(h::err("sign: expected a number")),
        }),
    );
    bc.getters.insert(
        "isFinite".into(),
        getter(|_v, t| {
            Ok(Value::Bool(
                !matches!(t, Value::Double(d) if d.is_infinite() || d.is_nan()),
            ))
        }),
    );
    bc.getters.insert(
        "isInfinite".into(),
        getter(|_v, t| {
            Ok(Value::Bool(
                matches!(t, Value::Double(d) if d.is_infinite()),
            ))
        }),
    );
    bc.getters.insert(
        "isNaN".into(),
        getter(|_v, t| Ok(Value::Bool(matches!(t, Value::Double(d) if d.is_nan())))),
    );
    bc.getters.insert(
        "isNegative".into(),
        getter(|_v, t| match t {
            Value::Int(i) => Ok(Value::Bool(i < 0)),
            Value::Double(d) => Ok(Value::Bool(d < 0.0 || (d == 0.0 && d.is_sign_negative()))),
            _ => Err(h::err("isNegative: expected a number")),
        }),
    );
    bc
}

pub fn int_definition() -> BridgedClassDef {
    let mut bc = BridgedClassDef::new("int");
    bc.native_type = Some("int".into());
    bc.static_methods.insert(
        "parse".into(),
        static_method(|_v, args, named| {
            let source = h::arg_str(&args, 0, "parse")?;
            let radix = named.get("radix").and_then(h::as_int);
            let result = match radix {
                Some(r) if (2..=36).contains(&r) => i64::from_str_radix(source.trim(), r as u32),
                Some(r) => return Err(h::format_exception(format!("Invalid radix: {r}"))),
                None => {
                    let t = source.trim();
                    if t.starts_with("0x") || t.starts_with("0X") {
                        i64::from_str_radix(&t[2..], 16)
                    } else {
                        t.parse::<i64>()
                    }
                }
            };
            result.map(Value::Int).map_err(|_| {
                h::format_exception(format!(
                    "Invalid radix-10 number (at character 0): {source}"
                ))
            })
        }),
    );
    bc.static_methods.insert(
        "tryParse".into(),
        static_method(|_v, args, named| {
            let source = h::arg_str(&args, 0, "tryParse")?;
            let radix = named.get("radix").and_then(h::as_int);
            let result = match radix {
                Some(r) if (2..=36).contains(&r) => i64::from_str_radix(source.trim(), r as u32),
                _ => {
                    let t = source.trim();
                    if t.starts_with("0x") || t.starts_with("0X") {
                        i64::from_str_radix(&t[2..], 16)
                    } else {
                        t.parse::<i64>()
                    }
                }
            };
            Ok(result.map(Value::Int).unwrap_or(Value::Null))
        }),
    );
    bc.static_methods.insert(
        "fromEnvironment".into(),
        static_method(|_v, args, named| {
            h::require_args(&args, 1, "int.fromEnvironment")?;
            let name = h::arg_str(&args, 0, "fromEnvironment")?;
            let default = named.get("defaultValue").and_then(h::as_int).unwrap_or(0);
            Ok(Value::Int(
                std::env::var(name)
                    .ok()
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(default),
            ))
        }),
    );
    let int_methods: Vec<(&str, M)> = vec![
        (
            "abs",
            method(|_v, t, _a, _n| {
                Ok(Value::Int(
                    h::as_int(&t)
                        .ok_or_else(|| h::err("abs: expected int"))?
                        .wrapping_abs(),
                ))
            }),
        ),
        ("ceil", method(|_v, t, _a, _n| Ok(t))),
        ("floor", method(|_v, t, _a, _n| Ok(t))),
        ("round", method(|_v, t, _a, _n| Ok(t))),
        ("truncate", method(|_v, t, _a, _n| Ok(t))),
        (
            "toDouble",
            method(|_v, t, _a, _n| Ok(Value::Double(h::as_int(&t).unwrap_or(0) as f64))),
        ),
        ("toInt", method(|_v, t, _a, _n| Ok(t))),
        (
            "toString",
            method(|_v, t, _a, _n| Ok(Value::str(h::as_int(&t).unwrap_or(0).to_string()))),
        ),
        (
            "toStringAsFixed",
            method(|_v, t, args, _n| {
                let digits = h::arg_int(&args, 0, "toStringAsFixed")?;
                Ok(Value::str(int_to_string_as_fixed(
                    h::as_int(&t).unwrap_or(0),
                    digits,
                )?))
            }),
        ),
        (
            "toStringAsExponential",
            method(|_v, t, args, _n| {
                let digits = args.first().and_then(h::as_int);
                Ok(Value::str(num_to_string_as_exponential(
                    h::as_int(&t).unwrap_or(0) as f64,
                    digits,
                )?))
            }),
        ),
        (
            "toStringAsPrecision",
            method(|_v, t, args, _n| {
                let precision = h::arg_int(&args, 0, "toStringAsPrecision")?;
                Ok(Value::str(num_to_string_as_precision(
                    h::as_int(&t).unwrap_or(0) as f64,
                    precision,
                )?))
            }),
        ),
        (
            "toRadixString",
            method(|_v, t, args, _n| {
                let radix = h::arg_int(&args, 0, "toRadixString")?;
                if !(2..=36).contains(&radix) {
                    return Err(h::err(format!("toRadixString: radix {radix} out of range")));
                }
                let n = h::as_int(&t).unwrap_or(0);
                let s = if n < 0 {
                    format!(
                        "-{}",
                        unsigned_radix((n as i128).unsigned_abs() as u64, radix as u32)
                    )
                } else {
                    unsigned_radix(n as u64, radix as u32)
                };
                Ok(Value::str(s))
            }),
        ),
        (
            "compareTo",
            method(|_v, t, args, _n| {
                let other = args
                    .first()
                    .ok_or_else(|| h::err("compareTo requires an argument"))?;
                let (a, b) = (h::as_double(&t), h::as_double(other));
                match (a, b) {
                    (Some(a), Some(b)) => Ok(Value::Int(match a.partial_cmp(&b) {
                        Some(std::cmp::Ordering::Less) => -1,
                        Some(std::cmp::Ordering::Equal) => 0,
                        Some(std::cmp::Ordering::Greater) => 1,
                        None => 0,
                    })),
                    _ => Err(h::err("compareTo: expected a num")),
                }
            }),
        ),
        (
            "clamp",
            method(|_v, t, args, _n| {
                let (lo, hi) = (args.first(), args.get(1));
                let (Some(lo), Some(hi)) = (lo, hi) else {
                    return Err(h::err("clamp requires two arguments"));
                };
                let (x, lo, hi) = (
                    h::as_double(&t).ok_or_else(|| h::err("clamp: expected a num"))?,
                    h::as_double(lo).ok_or_else(|| h::err("clamp: expected a num"))?,
                    h::as_double(hi).ok_or_else(|| h::err("clamp: expected a num"))?,
                );
                let result = if x < lo {
                    lo
                } else if x > hi {
                    hi
                } else {
                    x
                };
                if result == result.trunc() && result.abs() < 9.2e18 {
                    Ok(Value::Int(result as i64))
                } else {
                    Ok(Value::Double(result))
                }
            }),
        ),
        (
            "remainder",
            method(|_v, t, args, _n| {
                let other = args
                    .first()
                    .ok_or_else(|| h::err("remainder requires an argument"))?;
                let (a, b) = (h::as_double(&t), h::as_double(other));
                match (a, b) {
                    (Some(a), Some(b)) => Ok(Value::Double(a.rem_euclid(b))),
                    _ => Err(h::err("remainder: expected a num")),
                }
            }),
        ),
        (
            "gcd",
            method(|_v, t, args, _n| {
                let other = h::arg_int(&args, 0, "gcd")?;
                let a = h::as_int(&t).unwrap_or(0);
                Ok(Value::Int(
                    a.unsigned_abs().gcd(&other.unsigned_abs()) as i64
                ))
            }),
        ),
        (
            "modInverse",
            method(|_v, t, args, _n| {
                let modulus = h::arg_int(&args, 0, "modInverse")?;
                let a = h::as_int(&t).unwrap_or(0);
                match mod_inverse(a, modulus) {
                    Some(m) => Ok(Value::Int(m)),
                    None => Err(h::err("modInverse: inverse does not exist")),
                }
            }),
        ),
        (
            "modPow",
            method(|_v, t, args, _n| {
                let exponent = h::arg_int(&args, 0, "modPow")?;
                let modulus = h::arg_int(&args, 1, "modPow")?;
                let base = h::as_int(&t).unwrap_or(0);
                if modulus == 0 {
                    return Err(h::err("modPow: modulus cannot be zero"));
                }
                let mut result = 1i128;
                let mut base = (base % modulus) as i128;
                let mut exp = exponent;
                let modulus = modulus as i128;
                while exp > 0 {
                    if exp & 1 == 1 {
                        result = (result * base) % modulus;
                    }
                    base = (base * base) % modulus;
                    exp >>= 1;
                }
                let result = ((result % modulus) + modulus) % modulus;
                Ok(Value::Int(result as i64))
            }),
        ),
        (
            "toSigned",
            method(|_v, t, args, _n| {
                let width = h::arg_int(&args, 0, "toSigned")?;
                if width <= 0 || width >= 64 {
                    return Ok(t);
                }
                let n = h::as_int(&t).unwrap_or(0);
                let shift = 64 - width;
                Ok(Value::Int((n << shift) >> shift))
            }),
        ),
        (
            "toUnsigned",
            method(|_v, t, args, _n| {
                let width = h::arg_int(&args, 0, "toUnsigned")?;
                if width <= 0 || width >= 64 {
                    return Ok(t);
                }
                let n = h::as_int(&t).unwrap_or(0);
                let mask = if width == 64 {
                    u64::MAX
                } else {
                    (1u64 << width) - 1
                };
                Ok(Value::Int((n as u64 & mask) as i64))
            }),
        ),
        (
            "+",
            method(|_v, t, args, _n| {
                let other = args.first().cloned().unwrap_or(Value::Null);
                num_add(&t, &other).ok_or_else(|| h::err("Invalid operand for +"))
            }),
        ),
        (
            "-",
            method(|_v, t, args, _n| {
                let other = args.first().cloned().unwrap_or(Value::Null);
                num_sub(&t, &other).ok_or_else(|| h::err("Invalid operand for -"))
            }),
        ),
        (
            "*",
            method(|_v, t, args, _n| {
                let other = args.first().cloned().unwrap_or(Value::Null);
                num_mul(&t, &other).ok_or_else(|| h::err("Invalid operand for *"))
            }),
        ),
        (
            "/",
            method(|_v, t, args, _n| {
                let other = args.first().cloned().unwrap_or(Value::Null);
                num_div(&t, &other).ok_or_else(|| h::err("Invalid operand for /"))
            }),
        ),
        (
            "~/",
            method(|_v, t, args, _n| {
                let other = args.first().cloned().unwrap_or(Value::Null);
                match num_trunc_div(&t, &other) {
                    Some(Ok(v)) => Ok(v),
                    Some(Err(e)) => Err(e),
                    None => Err(h::err("Invalid operand for ~/")),
                }
            }),
        ),
        (
            "%",
            method(|_v, t, args, _n| {
                let other = args.first().cloned().unwrap_or(Value::Null);
                match num_rem(&t, &other) {
                    Some(Ok(v)) => Ok(v),
                    Some(Err(e)) => Err(e),
                    None => Err(h::err("Invalid operand for %")),
                }
            }),
        ),
        (
            "<<",
            method(|_v, t, args, _n| {
                let shift = h::arg_int(&args, 0, "<<")?;
                Ok(Value::Int(
                    h::as_int(&t).unwrap_or(0).wrapping_shl(shift as u32),
                ))
            }),
        ),
        (
            ">>",
            method(|_v, t, args, _n| {
                let shift = h::arg_int(&args, 0, ">>")?;
                Ok(Value::Int(
                    h::as_int(&t).unwrap_or(0).wrapping_shr(shift as u32),
                ))
            }),
        ),
        (
            "&",
            method(|_v, t, args, _n| {
                let other = h::arg_int(&args, 0, "&")?;
                Ok(Value::Int(h::as_int(&t).unwrap_or(0) & other))
            }),
        ),
        (
            "|",
            method(|_v, t, args, _n| {
                let other = h::arg_int(&args, 0, "|")?;
                Ok(Value::Int(h::as_int(&t).unwrap_or(0) | other))
            }),
        ),
        (
            "^",
            method(|_v, t, args, _n| {
                let other = h::arg_int(&args, 0, "^")?;
                Ok(Value::Int(h::as_int(&t).unwrap_or(0) ^ other))
            }),
        ),
        (
            "unary-",
            method(|_v, t, _a, _n| Ok(Value::Int(h::as_int(&t).unwrap_or(0).wrapping_neg()))),
        ),
        (
            "~",
            method(|_v, t, _a, _n| Ok(Value::Int(!h::as_int(&t).unwrap_or(0)))),
        ),
    ];
    for (k, m) in int_methods {
        bc.methods.insert(k.into(), m);
    }
    bc.getters.insert(
        "hashCode".into(),
        getter(|_v, t| Ok(Value::Int(h::hash_code(&t)))),
    );
    bc.getters
        .insert("runtimeType".into(), getter(|_v, t| Ok(runtime_type(&t))));
    bc.getters.insert(
        "bitLength".into(),
        getter(|_v, t| {
            let n = h::as_int(&t).unwrap_or(0);
            let mag = if n < 0 { !n } else { n };
            Ok(Value::Int(if mag == 0 {
                0
            } else {
                64 - mag.leading_zeros() as i64
            }))
        }),
    );
    bc.getters.insert(
        "sign".into(),
        getter(|_v, t| {
            Ok(Value::Int(if h::as_int(&t).unwrap_or(0) < 0 {
                -1
            } else {
                1
            }))
        }),
    );
    bc.getters.insert(
        "isEven".into(),
        getter(|_v, t| Ok(Value::Bool(h::as_int(&t).unwrap_or(0) % 2 == 0))),
    );
    bc.getters.insert(
        "isOdd".into(),
        getter(|_v, t| Ok(Value::Bool(h::as_int(&t).unwrap_or(0) % 2 != 0))),
    );
    bc.getters
        .insert("isFinite".into(), getter(|_v, _t| Ok(Value::Bool(true))));
    bc.getters
        .insert("isInfinite".into(), getter(|_v, _t| Ok(Value::Bool(false))));
    bc.getters
        .insert("isNaN".into(), getter(|_v, _t| Ok(Value::Bool(false))));
    bc.getters.insert(
        "isNegative".into(),
        getter(|_v, t| Ok(Value::Bool(h::as_int(&t).unwrap_or(0) < 0))),
    );
    bc
}

fn unsigned_radix(mut n: u64, radix: u32) -> String {
    if n == 0 {
        return "0".to_string();
    }
    let digits = b"0123456789abcdefghijklmnopqrstuvwxyz";
    let mut out = Vec::new();
    while n > 0 {
        out.push(digits[(n % radix as u64) as usize]);
        n /= radix as u64;
    }
    out.reverse();
    String::from_utf8(out).unwrap_or_default()
}

fn mod_inverse(a: i64, m: i64) -> Option<i64> {
    if m == 0 {
        return None;
    }
    let (mut t, mut new_t) = (0i128, 1i128);
    let (mut r, mut new_r) = (m as i128, (a % m) as i128);
    while new_r != 0 {
        let q = r / new_r;
        (t, new_t) = (new_t, t - q * new_t);
        (r, new_r) = (new_r, r - q * new_r);
    }
    if r > 1 {
        return None;
    }
    if t < 0 {
        t += m as i128;
    }
    Some(t as i64)
}

pub fn double_definition() -> BridgedClassDef {
    let mut bc = BridgedClassDef::new("double");
    bc.native_type = Some("double".into());
    bc.static_methods.insert(
        "parse".into(),
        static_method(|_v, args, _n| {
            let s = h::arg_str(&args, 0, "parse")?;
            s.trim()
                .parse::<f64>()
                .map(Value::Double)
                .map_err(|_| h::format_exception(format!("Invalid double: {s}")))
        }),
    );
    bc.static_methods.insert(
        "tryParse".into(),
        static_method(|_v, args, _n| {
            let s = h::arg_str(&args, 0, "tryParse")?;
            Ok(s.trim()
                .parse::<f64>()
                .map(Value::Double)
                .unwrap_or(Value::Null))
        }),
    );
    bc.static_getters.insert(
        "infinity".into(),
        static_getter(|_v| Ok(Value::Double(f64::INFINITY))),
    );
    bc.static_getters.insert(
        "negativeInfinity".into(),
        static_getter(|_v| Ok(Value::Double(f64::NEG_INFINITY))),
    );
    bc.static_getters.insert(
        "nan".into(),
        static_getter(|_v| Ok(Value::Double(f64::NAN))),
    );
    bc.static_getters.insert(
        "maxFinite".into(),
        static_getter(|_v| Ok(Value::Double(f64::MAX))),
    );
    bc.static_getters.insert(
        "minPositive".into(),
        static_getter(|_v| Ok(Value::Double(f64::MIN_POSITIVE))),
    );
    let double_methods: Vec<(&str, M)> = vec![
        (
            "abs",
            method(|_v, t, _a, _n| Ok(Value::Double(h::as_double(&t).unwrap_or(0.0).abs()))),
        ),
        (
            "ceil",
            method(|_v, t, _a, _n| Ok(Value::Int(h::as_double(&t).unwrap_or(0.0).ceil() as i64))),
        ),
        (
            "floor",
            method(|_v, t, _a, _n| Ok(Value::Int(h::as_double(&t).unwrap_or(0.0).floor() as i64))),
        ),
        (
            "round",
            method(|_v, t, _a, _n| Ok(Value::Int(h::as_double(&t).unwrap_or(0.0).round() as i64))),
        ),
        (
            "truncate",
            method(|_v, t, _a, _n| Ok(Value::Int(h::as_double(&t).unwrap_or(0.0).trunc() as i64))),
        ),
        (
            "ceilToDouble",
            method(|_v, t, _a, _n| Ok(Value::Double(h::as_double(&t).unwrap_or(0.0).ceil()))),
        ),
        (
            "floorToDouble",
            method(|_v, t, _a, _n| Ok(Value::Double(h::as_double(&t).unwrap_or(0.0).floor()))),
        ),
        (
            "roundToDouble",
            method(|_v, t, _a, _n| Ok(Value::Double(h::as_double(&t).unwrap_or(0.0).round()))),
        ),
        (
            "truncateToDouble",
            method(|_v, t, _a, _n| Ok(Value::Double(h::as_double(&t).unwrap_or(0.0).trunc()))),
        ),
        ("toDouble", method(|_v, t, _a, _n| Ok(t))),
        (
            "toInt",
            method(|_v, t, _a, _n| Ok(Value::Int(h::as_double(&t).unwrap_or(0.0).trunc() as i64))),
        ),
        (
            "toString",
            method(|_v, t, _a, _n| Ok(Value::str(fmt_double(h::as_double(&t).unwrap_or(0.0))))),
        ),
        (
            "toStringAsFixed",
            method(|_v, t, args, _n| {
                let digits = h::arg_int(&args, 0, "toStringAsFixed")?;
                Ok(Value::str(num_to_string_as_fixed(
                    h::as_double(&t).unwrap_or(0.0),
                    digits,
                )?))
            }),
        ),
        (
            "toStringAsExponential",
            method(|_v, t, args, _n| {
                let digits = args.first().and_then(h::as_int);
                Ok(Value::str(num_to_string_as_exponential(
                    h::as_double(&t).unwrap_or(0.0),
                    digits,
                )?))
            }),
        ),
        (
            "toStringAsPrecision",
            method(|_v, t, args, _n| {
                let precision = h::arg_int(&args, 0, "toStringAsPrecision")?;
                Ok(Value::str(num_to_string_as_precision(
                    h::as_double(&t).unwrap_or(0.0),
                    precision,
                )?))
            }),
        ),
        (
            "compareTo",
            method(|_v, t, args, _n| {
                let other = args
                    .first()
                    .ok_or_else(|| h::err("compareTo requires an argument"))?;
                let (a, b) = (h::as_double(&t), h::as_double(other));
                match (a, b) {
                    (Some(a), Some(b)) => Ok(Value::Int(match a.partial_cmp(&b) {
                        Some(std::cmp::Ordering::Less) => -1,
                        Some(std::cmp::Ordering::Equal) => 0,
                        Some(std::cmp::Ordering::Greater) => 1,
                        None => 0,
                    })),
                    _ => Err(h::err("compareTo: expected a num")),
                }
            }),
        ),
        (
            "clamp",
            method(|_v, t, args, _n| {
                let (lo, hi) = (args.first(), args.get(1));
                let (Some(lo), Some(hi)) = (lo, hi) else {
                    return Err(h::err("clamp requires two arguments"));
                };
                let (x, lo, hi) = (
                    h::as_double(&t).ok_or_else(|| h::err("clamp: expected a num"))?,
                    h::as_double(lo).ok_or_else(|| h::err("clamp: expected a num"))?,
                    h::as_double(hi).ok_or_else(|| h::err("clamp: expected a num"))?,
                );
                Ok(Value::Double(if x < lo {
                    lo
                } else if x > hi {
                    hi
                } else {
                    x
                }))
            }),
        ),
        (
            "remainder",
            method(|_v, t, args, _n| {
                let other = args
                    .first()
                    .ok_or_else(|| h::err("remainder requires an argument"))?;
                let (a, b) = (h::as_double(&t), h::as_double(other));
                match (a, b) {
                    (Some(a), Some(b)) => Ok(Value::Double(a.rem_euclid(b))),
                    _ => Err(h::err("remainder: expected a num")),
                }
            }),
        ),
        (
            "+",
            method(|_v, t, args, _n| {
                let other = args.first().cloned().unwrap_or(Value::Null);
                num_add(&t, &other).ok_or_else(|| h::err("Invalid operand for +"))
            }),
        ),
        (
            "-",
            method(|_v, t, args, _n| {
                let other = args.first().cloned().unwrap_or(Value::Null);
                num_sub(&t, &other).ok_or_else(|| h::err("Invalid operand for -"))
            }),
        ),
        (
            "*",
            method(|_v, t, args, _n| {
                let other = args.first().cloned().unwrap_or(Value::Null);
                num_mul(&t, &other).ok_or_else(|| h::err("Invalid operand for *"))
            }),
        ),
        (
            "/",
            method(|_v, t, args, _n| {
                let other = args.first().cloned().unwrap_or(Value::Null);
                num_div(&t, &other).ok_or_else(|| h::err("Invalid operand for /"))
            }),
        ),
        (
            "~/",
            method(|_v, t, args, _n| {
                let other = args.first().cloned().unwrap_or(Value::Null);
                match num_trunc_div(&t, &other) {
                    Some(Ok(v)) => Ok(v),
                    Some(Err(e)) => Err(e),
                    None => Err(h::err("Invalid operand for ~/")),
                }
            }),
        ),
        (
            "%",
            method(|_v, t, args, _n| {
                let other = args.first().cloned().unwrap_or(Value::Null);
                match num_rem(&t, &other) {
                    Some(Ok(v)) => Ok(v),
                    Some(Err(e)) => Err(e),
                    None => Err(h::err("Invalid operand for %")),
                }
            }),
        ),
        (
            "unary-",
            method(|_v, t, _a, _n| Ok(Value::Double(-h::as_double(&t).unwrap_or(0.0)))),
        ),
    ];
    for (k, m) in double_methods {
        bc.methods.insert(k.into(), m);
    }
    bc.getters.insert(
        "hashCode".into(),
        getter(|_v, t| Ok(Value::Int(h::hash_code(&t)))),
    );
    bc.getters
        .insert("runtimeType".into(), getter(|_v, t| Ok(runtime_type(&t))));
    bc.getters.insert(
        "sign".into(),
        getter(|_v, t| {
            let d = h::as_double(&t).unwrap_or(0.0);
            Ok(Value::Double(if d.is_nan() {
                f64::NAN
            } else if d > 0.0 || (d == 0.0 && d.is_sign_positive()) {
                1.0
            } else {
                -1.0
            }))
        }),
    );
    bc.getters.insert(
        "isFinite".into(),
        getter(|_v, t| Ok(Value::Bool(h::as_double(&t).unwrap_or(0.0).is_finite()))),
    );
    bc.getters.insert(
        "isInfinite".into(),
        getter(|_v, t| Ok(Value::Bool(h::as_double(&t).unwrap_or(0.0).is_infinite()))),
    );
    bc.getters.insert(
        "isNaN".into(),
        getter(|_v, t| Ok(Value::Bool(h::as_double(&t).unwrap_or(0.0).is_nan()))),
    );
    bc.getters.insert(
        "isNegative".into(),
        getter(|_v, t| {
            let d = h::as_double(&t).unwrap_or(0.0);
            Ok(Value::Bool(d < 0.0 || (d == 0.0 && d.is_sign_negative())))
        }),
    );
    bc
}

pub fn comparable_definition() -> BridgedClassDef {
    let mut bc = BridgedClassDef::new("Comparable");
    bc.static_methods.insert(
        "compare".into(),
        static_method(|_v, args, _n| {
            if args.len() != 2 {
                return Err(h::err(
                    "Comparable.compare expects two Comparable arguments.",
                ));
            }
            let a = args[0].clone();
            let b = args[1].clone();
            Ok(Value::Int(compare_values(&a, &b)?))
        }),
    );
    bc.methods.insert(
        "toString".into(),
        method(|_v, t, _a, _n| Ok(Value::str(value_display(&t)))),
    );
    bc.methods.insert(
        "compareTo".into(),
        method(|_v, t, args, _n| {
            if args.len() != 1 {
                return Err(h::err(
                    "Comparable.compareTo requires a Comparable argument.",
                ));
            }
            Ok(Value::Int(compare_values(&t, &args[0])?))
        }),
    );
    bc.getters.insert(
        "hashCode".into(),
        getter(|_v, t| Ok(Value::Int(h::hash_code(&t)))),
    );
    bc.getters
        .insert("runtimeType".into(), getter(|_v, t| Ok(runtime_type(&t))));
    bc
}

fn compare_values(a: &Value, b: &Value) -> Result<i64, InterpError> {
    if let (Some(x), Some(y)) = (h::as_double(a), h::as_double(b)) {
        return Ok(match x.partial_cmp(&y) {
            Some(std::cmp::Ordering::Less) => -1,
            Some(std::cmp::Ordering::Equal) => 0,
            Some(std::cmp::Ordering::Greater) => 1,
            None => 0,
        });
    }
    if let (Some(x), Some(y)) = (h::as_str(a), h::as_str(b)) {
        return Ok(match x.cmp(y) {
            std::cmp::Ordering::Less => -1,
            std::cmp::Ordering::Equal => 0,
            std::cmp::Ordering::Greater => 1,
        });
    }
    Err(h::err("Comparable.compare: values are not comparable"))
}

pub fn function_definition() -> BridgedClassDef {
    let mut bc = BridgedClassDef::new("Function");
    bc.native_type = Some("Function".into());
    bc.is_subtype_of_fn = Some(Box::new(|v| {
        matches!(v, Value::Function(_) | Value::NativeFn(_))
    }));
    bc.static_methods.insert(
        "apply".into(),
        static_method(|_v, args, named| {
            if args.is_empty() {
                return Err(h::err(
                    "Function.apply requires a Callable as the first argument.",
                ));
            }
            let function = args[0].clone();
            let call_args = args
                .get(1)
                .and_then(h::as_list)
                .map(|l| l.borrow().clone())
                .unwrap_or_default();
            let call_named = args
                .get(2)
                .and_then(h::as_map)
                .map(|m| {
                    m.borrow()
                        .entries
                        .iter()
                        .map(|(k, v)| (h::as_str(k).unwrap_or_default().to_string(), v.clone()))
                        .collect()
                })
                .unwrap_or(named);
            _v.call_value(&function, call_args, call_named)
        }),
    );
    bc.methods.insert(
        "call".into(),
        method(|_v, t, args, _named| h::call_closure(_v, &t, args)),
    );
    bc.methods.insert(
        "hashCode".into(),
        method(|_v, t, _a, _n| Ok(Value::Int(h::hash_code(&t)))),
    );
    bc.methods.insert(
        "toString".into(),
        method(|_v, t, _a, _n| Ok(Value::str(value_display(&t)))),
    );
    bc.getters.insert(
        "hashCode".into(),
        getter(|_v, t| Ok(Value::Int(h::hash_code(&t)))),
    );
    bc.getters
        .insert("runtimeType".into(), getter(|_v, t| Ok(runtime_type(&t))));
    bc
}

pub fn symbol_definition() -> BridgedClassDef {
    let mut bc = BridgedClassDef::new("Symbol");
    bc.constructors.insert(
        "".into(),
        ctor(|_v, args, _n| {
            h::require_args(&args, 1, "Symbol")?;
            let s = h::arg_str(&args, 0, "Symbol")?;
            Ok(h::wrap(&h::class("Symbol"), Value::str(s)))
        }),
    );
    bc.methods.insert(
        "==".into(),
        method(|_v, t, args, _n| {
            let other = args.first().cloned().unwrap_or(Value::Null);
            Ok(Value::Bool(value_eq(&t, &other)))
        }),
    );
    bc.methods.insert(
        "toString".into(),
        method(|_v, t, _a, _n| {
            Ok(Value::str(format!(
                "Symbol(\"{}\")",
                h::as_str(&t).unwrap_or_default()
            )))
        }),
    );
    bc.static_getters.insert(
        "empty".into(),
        static_getter(|_v| Ok(h::wrap(&h::class("Symbol"), Value::str("")))),
    );
    bc.static_getters.insert(
        "unaryMinus".into(),
        static_getter(|_v| Ok(h::wrap(&h::class("Symbol"), Value::str("-")))),
    );
    bc.getters.insert(
        "hashCode".into(),
        getter(|_v, t| Ok(Value::Int(h::hash_code(&t)))),
    );
    bc.getters
        .insert("runtimeType".into(), getter(|_v, t| Ok(runtime_type(&t))));
    bc
}

pub fn type_definition() -> BridgedClassDef {
    let mut bc = BridgedClassDef::new("Type");
    bc.native_type = Some("Type".into());
    bc.is_subtype_of_fn = Some(Box::new(|v| matches!(v, Value::Type(_))));
    bc.methods.insert(
        "==".into(),
        method(|_v, t, args, _n| {
            let other = args.first().cloned().unwrap_or(Value::Null);
            let eq = match (&t, &other) {
                (Value::Type(a), Value::Type(b)) => a.name() == b.name(),
                _ => false,
            };
            Ok(Value::Bool(eq))
        }),
    );
    bc.methods.insert(
        "toString".into(),
        method(|_v, t, _a, _n| {
            Ok(Value::str(match t {
                Value::Type(ty) => ty.name(),
                _ => value_display(&t),
            }))
        }),
    );
    bc.getters.insert(
        "hashCode".into(),
        getter(|_v, t| Ok(Value::Int(h::hash_code(&t)))),
    );
    bc.getters
        .insert("runtimeType".into(), getter(|_v, t| Ok(runtime_type(&t))));
    bc
}
