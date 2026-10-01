use super::super::helpers as h;
use crate::class::BridgedClassDef;
use crate::value::Value;

const MICROS_PER_DAY: i64 = 86_400_000_000;
const MICROS_PER_HOUR: i64 = 3_600_000_000;
const MICROS_PER_MINUTE: i64 = 60_000_000;
const MICROS_PER_SECOND: i64 = 1_000_000;
const MICROS_PER_MILLI: i64 = 1_000;

/// Comparison operator name paired with its predicate over a micros delta.
type DurationCmp = (&'static str, fn(i64) -> bool);

fn as_micros(v: &Value) -> i64 {
    h::as_int(v).unwrap_or(0)
}

fn duration_to_string(micros: i64) -> String {
    if micros < 0 {
        return format!("-{}", duration_to_string(micros.saturating_neg()));
    }
    let two_digits = |n: i64| -> String {
        if n >= 10 {
            n.to_string()
        } else {
            format!("0{n}")
        }
    };
    let three_digits = |n: i64| -> String {
        if n >= 100 {
            n.to_string()
        } else {
            format!("0{}", two_digits(n))
        }
    };
    let in_days = micros / MICROS_PER_DAY;
    let in_hours = micros / MICROS_PER_HOUR;
    let in_minutes = micros / MICROS_PER_MINUTE;
    let in_seconds = micros / MICROS_PER_SECOND;
    let in_millis = micros / MICROS_PER_MILLI;
    let two_digit_minutes = two_digits(in_minutes % 60);
    let two_digit_seconds = two_digits(in_seconds % 60);
    let three_digit_millis = three_digits(in_millis % 1000);
    let three_digit_micros = three_digits(micros % 1000);
    if in_days > 0 {
        format!(
            "{in_days}:{}:{}:{}.{}{}",
            two_digits(in_hours % 24),
            two_digit_minutes,
            two_digit_seconds,
            three_digit_millis,
            three_digit_micros
        )
    } else {
        format!(
            "{}:{two_digit_minutes}:{two_digit_seconds}.{three_digit_millis}{three_digit_micros}",
            in_hours % 24
        )
    }
}

pub fn duration_definition() -> BridgedClassDef {
    let mut bc = BridgedClassDef::new("Duration");
    bc.constructors.insert(
        "".into(),
        h::ctor(|_v, _args, named| {
            let days = named.get("days").and_then(h::as_int).unwrap_or(0);
            let hours = named.get("hours").and_then(h::as_int).unwrap_or(0);
            let minutes = named.get("minutes").and_then(h::as_int).unwrap_or(0);
            let seconds = named.get("seconds").and_then(h::as_int).unwrap_or(0);
            let milliseconds = named.get("milliseconds").and_then(h::as_int).unwrap_or(0);
            let microseconds = named.get("microseconds").and_then(h::as_int).unwrap_or(0);
            let total = days * MICROS_PER_DAY
                + hours * MICROS_PER_HOUR
                + minutes * MICROS_PER_MINUTE
                + seconds * MICROS_PER_SECOND
                + milliseconds * MICROS_PER_MILLI
                + microseconds;
            Ok(h::wrap(&h::class("Duration"), Value::Int(total)))
        }),
    );
    bc.static_getters.insert(
        "hoursPerDay".into(),
        h::static_getter(|_v| Ok(Value::Int(24))),
    );
    bc.static_getters.insert(
        "microsecondsPerSecond".into(),
        h::static_getter(|_v| Ok(Value::Int(1_000_000))),
    );
    bc.static_getters.insert(
        "millisecondsPerSecond".into(),
        h::static_getter(|_v| Ok(Value::Int(1000))),
    );
    bc.static_getters.insert(
        "secondsPerMinute".into(),
        h::static_getter(|_v| Ok(Value::Int(60))),
    );
    bc.static_getters.insert(
        "minutesPerHour".into(),
        h::static_getter(|_v| Ok(Value::Int(60))),
    );
    bc.static_getters.insert(
        "secondsPerHour".into(),
        h::static_getter(|_v| Ok(Value::Int(3600))),
    );
    bc.static_getters.insert(
        "zero".into(),
        h::static_getter(|_v| Ok(h::wrap(&h::class("Duration"), Value::Int(0)))),
    );
    bc.methods.insert(
        "+".into(),
        h::method(|_v, t, args, _n| {
            let other = as_micros(&args.first().cloned().unwrap_or(Value::Null));
            Ok(h::wrap(
                &h::class("Duration"),
                Value::Int(as_micros(&t).saturating_add(other)),
            ))
        }),
    );
    bc.methods.insert(
        "-".into(),
        h::method(|_v, t, args, _n| {
            let other = as_micros(&args.first().cloned().unwrap_or(Value::Null));
            Ok(h::wrap(
                &h::class("Duration"),
                Value::Int(as_micros(&t).saturating_sub(other)),
            ))
        }),
    );
    bc.methods.insert(
        "*".into(),
        h::method(|_v, t, args, _n| {
            let factor = h::as_double(&args.first().cloned().unwrap_or(Value::Null))
                .ok_or_else(|| h::err("*: expected a num"))?;
            Ok(h::wrap(
                &h::class("Duration"),
                Value::Int((as_micros(&t) as f64 * factor).round() as i64),
            ))
        }),
    );
    bc.methods.insert(
        "~/".into(),
        h::method(|_v, t, args, _n| {
            let other = h::arg_int(&args, 0, "~/")?;
            if other == 0 {
                return Err(h::err("Duration ~/ zero"));
            }
            Ok(h::wrap(
                &h::class("Duration"),
                Value::Int(as_micros(&t) / other),
            ))
        }),
    );
    bc.methods.insert(
        "unary-".into(),
        h::method(|_v, t, _a, _n| {
            Ok(h::wrap(
                &h::class("Duration"),
                Value::Int(as_micros(&t).saturating_neg()),
            ))
        }),
    );
    bc.methods.insert(
        "abs".into(),
        h::method(|_v, t, _a, _n| {
            Ok(h::wrap(
                &h::class("Duration"),
                Value::Int(as_micros(&t).abs()),
            ))
        }),
    );
    bc.methods.insert(
        "compareTo".into(),
        h::method(|_v, t, args, _n| {
            let other = as_micros(&args.first().cloned().unwrap_or(Value::Null));
            let a = as_micros(&t);
            Ok(Value::Int(if a < other {
                -1
            } else if a > other {
                1
            } else {
                0
            }))
        }),
    );
    let cmp_ops: [DurationCmp; 4] = [
        ("<", |o| o < 0),
        (">", |o| o > 0),
        ("<=", |o| o <= 0),
        (">=", |o| o >= 0),
    ];
    for (op, pred) in cmp_ops {
        bc.methods.insert(
            op.into(),
            h::method(move |_v, t, args, _n| {
                let other = as_micros(&args.first().cloned().unwrap_or(Value::Null));
                Ok(Value::Bool(pred(as_micros(&t) - other)))
            }),
        );
    }
    bc.methods.insert(
        "==".into(),
        h::method(|_v, t, args, _n| {
            let other = as_micros(&args.first().cloned().unwrap_or(Value::Null));
            Ok(Value::Bool(as_micros(&t) == other))
        }),
    );
    bc.methods.insert(
        "toString".into(),
        h::method(|_v, t, _a, _n| Ok(Value::str(duration_to_string(as_micros(&t))))),
    );
    bc.getters.insert(
        "inDays".into(),
        h::getter(|_v, t| Ok(Value::Int(as_micros(&t) / MICROS_PER_DAY))),
    );
    bc.getters.insert(
        "inHours".into(),
        h::getter(|_v, t| Ok(Value::Int(as_micros(&t) / MICROS_PER_HOUR))),
    );
    bc.getters.insert(
        "inMinutes".into(),
        h::getter(|_v, t| Ok(Value::Int(as_micros(&t) / MICROS_PER_MINUTE))),
    );
    bc.getters.insert(
        "inSeconds".into(),
        h::getter(|_v, t| Ok(Value::Int(as_micros(&t) / MICROS_PER_SECOND))),
    );
    bc.getters.insert(
        "inMilliseconds".into(),
        h::getter(|_v, t| Ok(Value::Int(as_micros(&t) / MICROS_PER_MILLI))),
    );
    bc.getters.insert(
        "inMicroseconds".into(),
        h::getter(|_v, t| Ok(Value::Int(as_micros(&t)))),
    );
    bc.getters.insert(
        "isNegative".into(),
        h::getter(|_v, t| Ok(Value::Bool(as_micros(&t) < 0))),
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
