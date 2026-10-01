use super::super::helpers as h;
use crate::class::BridgedClassDef;
use crate::error::InterpError;
use crate::value::Value;
use chrono::{Datelike, Local, TimeZone, Timelike, Utc};
use std::cell::RefCell;
use std::rc::Rc;

fn list(items: Vec<Value>) -> Value {
    Value::List(Rc::new(RefCell::new(items)))
}

fn as_dt(v: &Value) -> (i64, bool) {
    let l = h::as_list(v).unwrap_or_else(|| Rc::new(RefCell::new(vec![])));
    let items = l.borrow();
    let micros = items.first().and_then(h::as_int).unwrap_or(0);
    let is_utc = items.get(1).and_then(h::as_bool).unwrap_or(false);
    (micros, is_utc)
}

fn wrap_dt(micros: i64, is_utc: bool) -> Value {
    h::wrap(
        &h::class("DateTime"),
        list(vec![Value::Int(micros), Value::Bool(is_utc)]),
    )
}

fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = if m > 2 { m - 3 } else { m + 9 };
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

#[allow(clippy::too_many_arguments)]
fn from_components(
    year: i64,
    month: i64,
    day: i64,
    hour: i64,
    minute: i64,
    second: i64,
    millisecond: i64,
    microsecond: i64,
    is_utc: bool,
) -> Result<i64, InterpError> {
    let mut y = year;
    let mut m = month;
    if m <= 0 {
        y += (m - 1) / 12;
        m = ((m - 1) % 12) + 12;
    } else if m > 12 {
        y += (m - 1) / 12;
        m = ((m - 1) % 12) + 1;
    }
    let days = days_from_civil(y, m, day);
    let ce_days = days + 719163;
    let date = chrono::NaiveDate::from_num_days_from_ce_opt(ce_days as i32)
        .ok_or_else(|| h::err(format!("DateTime: invalid date {year}-{month}-{day}")))?;
    let total = hour * 3_600_000_000
        + minute * 60_000_000
        + second * 1_000_000
        + millisecond * 1_000
        + microsecond;
    let naive = date
        .and_hms_opt(0, 0, 0)
        .unwrap()
        .checked_add_signed(chrono::Duration::microseconds(total))
        .ok_or_else(|| h::err("DateTime: out of range"))?;
    if is_utc {
        Ok(Utc.from_utc_datetime(&naive).timestamp_micros())
    } else {
        let dt = Local
            .from_local_datetime(&naive)
            .earliest()
            .unwrap_or_else(|| Local.from_local_datetime(&naive).latest().unwrap());
        Ok(dt.timestamp_micros())
    }
}

fn two_digits(n: i64) -> String {
    if n < 10 {
        format!("0{n}")
    } else {
        n.to_string()
    }
}

fn local_dt(micros: i64) -> chrono::DateTime<chrono::Local> {
    Local
        .timestamp_micros(micros)
        .single()
        .unwrap_or_else(|| panic!("DateTime: invalid local time for {micros}"))
}

fn dt_of(micros: i64, is_utc: bool) -> chrono::DateTime<chrono::FixedOffset> {
    if is_utc {
        chrono::DateTime::from_timestamp_micros(micros)
            .unwrap()
            .fixed_offset()
    } else {
        local_dt(micros).fixed_offset()
    }
}

fn micros_to_parts(micros: i64, is_utc: bool) -> (i64, i64, i64, i64, i64, i64, i64, i64) {
    let dt = dt_of(micros, is_utc);
    (
        dt.year() as i64,
        dt.month() as i64,
        dt.day() as i64,
        dt.hour() as i64,
        dt.minute() as i64,
        dt.second() as i64,
        dt.timestamp_subsec_millis() as i64,
        (dt.timestamp_subsec_micros() % 1000) as i64,
    )
}

fn format_datetime(micros: i64, is_utc: bool, iso: bool) -> String {
    let (y, m, d, hh, mm, ss, ms, us) = micros_to_parts(micros, is_utc);
    let sep = if iso { 'T' } else { ' ' };
    let frac = if us == 0 {
        three_digits(ms)
    } else {
        format!("{}{}", three_digits(ms), three_digits(us))
    };
    let mut out = format!(
        "{y:04}-{}-{}{}{}:{}:{}.{}",
        two_digits(m),
        two_digits(d),
        sep,
        two_digits(hh),
        two_digits(mm),
        two_digits(ss),
        frac
    );
    if is_utc {
        out.push('Z');
    }
    out
}

fn three_digits(n: i64) -> String {
    if n < 10 {
        format!("00{n}")
    } else if n < 100 {
        format!("0{n}")
    } else {
        n.to_string()
    }
}

fn local_offset_seconds() -> i64 {
    Local::now().offset().local_minus_utc() as i64
}

fn parse_datetime(s: &str) -> Option<(i64, bool)> {
    let t = s.trim();
    if t.is_empty() {
        return None;
    }
    let (date_part, rest) = t.split_at(t.find([' ', 'T', 't']).unwrap_or(t.len()));
    let mut date_chars = date_part.chars();
    let year: i64 = date_chars.next()?.to_digit(10)? as i64 * 1000
        + date_chars.next()?.to_digit(10)? as i64 * 100
        + date_chars.next()?.to_digit(10)? as i64 * 10
        + date_chars.next()?.to_digit(10)? as i64;
    if date_chars.next()? != '-' {
        return None;
    }
    let month: i64 =
        (date_chars.next()?.to_digit(10)? * 10 + date_chars.next()?.to_digit(10)?) as i64;
    if date_chars.next()? != '-' {
        return None;
    }
    let day: i64 =
        (date_chars.next()?.to_digit(10)? * 10 + date_chars.next()?.to_digit(10)?) as i64;
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }

    let mut hour = 0i64;
    let mut minute = 0i64;
    let mut second = 0i64;
    let mut micros = 0i64;
    let mut offset_seconds = 0i64;
    let mut timezone_given = false;

    if !rest.is_empty() {
        let time_part = &rest[1..];
        let (time, zone) = match time_part.find(['Z', 'z', '+', '-']) {
            Some(i) => (&time_part[..i], Some(&time_part[i..])),
            None => (time_part, None),
        };
        let mut tc = time.chars();
        hour = (tc.next()?.to_digit(10)? * 10 + tc.next()?.to_digit(10)?) as i64;
        if tc.next()? != ':' {
            return None;
        }
        minute = (tc.next()?.to_digit(10)? * 10 + tc.next()?.to_digit(10)?) as i64;
        let rest2: String = tc.collect();
        if !rest2.is_empty() {
            let mut rest2 = rest2.chars();
            if rest2.next()? != ':' {
                return None;
            }
            second = (rest2.next()?.to_digit(10)? * 10 + rest2.next()?.to_digit(10)?) as i64;
            let frac: String = rest2.collect();
            if let Some(stripped) = frac.strip_prefix('.') {
                let digits: String = stripped
                    .chars()
                    .take_while(|c| c.is_ascii_digit())
                    .collect();
                let mut scaled = 0i64;
                for c in digits.chars().take(6) {
                    scaled = scaled * 10 + c.to_digit(10)? as i64;
                }
                for _ in digits.chars().take(6).count()..6 {
                    scaled *= 10;
                }
                micros = scaled;
            }
        }
        if let Some(zone) = zone {
            timezone_given = true;
            if zone != "Z" && zone != "z" {
                let (sign, digits) = zone.split_at(1);
                let negative = sign == "-";
                let (hh, mm) = if digits.len() == 2 {
                    (digits.parse::<i64>().ok()?, 0)
                } else if digits.len() == 4 {
                    (
                        digits[0..2].parse::<i64>().ok()?,
                        digits[2..4].parse::<i64>().ok()?,
                    )
                } else if digits.len() == 5 {
                    (
                        digits[0..2].parse::<i64>().ok()?,
                        digits[3..5].parse::<i64>().ok()?,
                    )
                } else {
                    return None;
                };
                offset_seconds = hh * 3600 + mm * 60;
                if negative {
                    offset_seconds = -offset_seconds;
                }
            }
        }
    }

    let micros_from_components = from_components(
        year,
        month,
        day,
        hour,
        minute,
        second,
        micros / 1000,
        micros % 1000,
        true,
    )
    .ok()?;
    if timezone_given {
        Some((micros_from_components - offset_seconds * 1_000_000, true))
    } else {
        let micros_local = from_components(
            year,
            month,
            day,
            hour,
            minute,
            second,
            micros / 1000,
            micros % 1000,
            false,
        )
        .ok()?;
        Some((micros_local, false))
    }
}

pub fn date_time_definition() -> BridgedClassDef {
    let mut bc = BridgedClassDef::new("DateTime");
    bc.constructors.insert(
        "".into(),
        h::ctor(|_v, args, _n| {
            if args.is_empty() {
                return Ok(wrap_dt(Local::now().timestamp_micros(), false));
            }
            let year = h::arg_int(&args, 0, "DateTime")?;
            let month = args.get(1).and_then(h::as_int).unwrap_or(1);
            let day = args.get(2).and_then(h::as_int).unwrap_or(1);
            let hour = args.get(3).and_then(h::as_int).unwrap_or(0);
            let minute = args.get(4).and_then(h::as_int).unwrap_or(0);
            let second = args.get(5).and_then(h::as_int).unwrap_or(0);
            let millisecond = args.get(6).and_then(h::as_int).unwrap_or(0);
            let microsecond = args.get(7).and_then(h::as_int).unwrap_or(0);
            let micros = from_components(
                year,
                month,
                day,
                hour,
                minute,
                second,
                millisecond,
                microsecond,
                false,
            )?;
            Ok(wrap_dt(micros, false))
        }),
    );
    bc.constructors.insert(
        "now".into(),
        h::ctor(|_v, _args, _n| Ok(wrap_dt(Local::now().timestamp_micros(), false))),
    );
    bc.constructors.insert(
        "utc".into(),
        h::ctor(|_v, args, _n| {
            let year = h::arg_int(&args, 0, "DateTime.utc")?;
            let month = args.get(1).and_then(h::as_int).unwrap_or(1);
            let day = args.get(2).and_then(h::as_int).unwrap_or(1);
            let hour = args.get(3).and_then(h::as_int).unwrap_or(0);
            let minute = args.get(4).and_then(h::as_int).unwrap_or(0);
            let second = args.get(5).and_then(h::as_int).unwrap_or(0);
            let millisecond = args.get(6).and_then(h::as_int).unwrap_or(0);
            let microsecond = args.get(7).and_then(h::as_int).unwrap_or(0);
            let micros = from_components(
                year,
                month,
                day,
                hour,
                minute,
                second,
                millisecond,
                microsecond,
                true,
            )?;
            Ok(wrap_dt(micros, true))
        }),
    );
    bc.constructors.insert(
        "fromMillisecondsSinceEpoch".into(),
        h::ctor(|_v, args, named| {
            let ms = h::arg_int(&args, 0, "fromMillisecondsSinceEpoch")?;
            let is_utc = named.get("isUtc").and_then(h::as_bool).unwrap_or(false);
            Ok(wrap_dt(ms.saturating_mul(1000), is_utc))
        }),
    );
    bc.constructors.insert(
        "fromMicrosecondsSinceEpoch".into(),
        h::ctor(|_v, args, named| {
            let us = h::arg_int(&args, 0, "fromMicrosecondsSinceEpoch")?;
            let is_utc = named.get("isUtc").and_then(h::as_bool).unwrap_or(false);
            Ok(wrap_dt(us, is_utc))
        }),
    );
    bc.static_methods.insert(
        "parse".into(),
        h::static_method(|_v, args, _n| {
            let s = h::arg_str(&args, 0, "parse")?;
            let (micros, is_utc) = parse_datetime(s)
                .ok_or_else(|| h::format_exception(format!("Invalid date format: {s}")))?;
            Ok(wrap_dt(micros, is_utc))
        }),
    );
    bc.static_methods.insert(
        "tryParse".into(),
        h::static_method(|_v, args, _n| {
            let s = h::arg_str(&args, 0, "tryParse")?;
            Ok(parse_datetime(s)
                .map(|(micros, is_utc)| wrap_dt(micros, is_utc))
                .unwrap_or(Value::Null))
        }),
    );
    let day_consts: &[(&str, i64)] = &[
        ("monday", 1),
        ("tuesday", 2),
        ("wednesday", 3),
        ("thursday", 4),
        ("friday", 5),
        ("saturday", 6),
        ("sunday", 7),
    ];
    for (name, v) in day_consts {
        bc.static_getters.insert(
            (*name).into(),
            h::static_getter(move |_v| Ok(Value::Int(*v))),
        );
    }
    let month_consts: &[(&str, i64)] = &[
        ("january", 1),
        ("february", 2),
        ("march", 3),
        ("april", 4),
        ("may", 5),
        ("june", 6),
        ("july", 7),
        ("august", 8),
        ("september", 9),
        ("october", 10),
        ("november", 11),
        ("december", 12),
    ];
    for (name, v) in month_consts {
        bc.static_getters.insert(
            (*name).into(),
            h::static_getter(move |_v| Ok(Value::Int(*v))),
        );
    }
    bc.static_getters.insert(
        "daysPerWeek".into(),
        h::static_getter(|_v| Ok(Value::Int(7))),
    );
    bc.static_getters.insert(
        "monthsPerYear".into(),
        h::static_getter(|_v| Ok(Value::Int(12))),
    );
    bc.methods.insert(
        "add".into(),
        h::method(|_v, t, args, _n| {
            let (micros, is_utc) = as_dt(&t);
            let other = h::as_int(&args.first().cloned().unwrap_or(Value::Null)).unwrap_or(0);
            Ok(wrap_dt(micros.saturating_add(other), is_utc))
        }),
    );
    bc.methods.insert(
        "subtract".into(),
        h::method(|_v, t, args, _n| {
            let (micros, is_utc) = as_dt(&t);
            let other = h::as_int(&args.first().cloned().unwrap_or(Value::Null)).unwrap_or(0);
            Ok(wrap_dt(micros.saturating_sub(other), is_utc))
        }),
    );
    bc.methods.insert(
        "difference".into(),
        h::method(|_v, t, args, _n| {
            let (micros, _) = as_dt(&t);
            let other = as_dt(&args.first().cloned().unwrap_or(Value::Null)).0;
            Ok(h::wrap(
                &h::class("Duration"),
                Value::Int(micros.saturating_sub(other)),
            ))
        }),
    );
    bc.methods.insert(
        "isBefore".into(),
        h::method(|_v, t, args, _n| {
            let (micros, _) = as_dt(&t);
            let other = as_dt(&args.first().cloned().unwrap_or(Value::Null)).0;
            Ok(Value::Bool(micros < other))
        }),
    );
    bc.methods.insert(
        "isAfter".into(),
        h::method(|_v, t, args, _n| {
            let (micros, _) = as_dt(&t);
            let other = as_dt(&args.first().cloned().unwrap_or(Value::Null)).0;
            Ok(Value::Bool(micros > other))
        }),
    );
    bc.methods.insert(
        "isAtSameMomentAs".into(),
        h::method(|_v, t, args, _n| {
            let (micros, _) = as_dt(&t);
            let other = as_dt(&args.first().cloned().unwrap_or(Value::Null)).0;
            Ok(Value::Bool(micros == other))
        }),
    );
    bc.methods.insert(
        "compareTo".into(),
        h::method(|_v, t, args, _n| {
            let (micros, _) = as_dt(&t);
            let other = as_dt(&args.first().cloned().unwrap_or(Value::Null)).0;
            Ok(Value::Int(if micros < other {
                -1
            } else if micros > other {
                1
            } else {
                0
            }))
        }),
    );
    bc.methods.insert(
        "toLocal".into(),
        h::method(|_v, t, _a, _n| {
            let (micros, _) = as_dt(&t);
            Ok(wrap_dt(micros, false))
        }),
    );
    bc.methods.insert(
        "toUtc".into(),
        h::method(|_v, t, _a, _n| {
            let (micros, _) = as_dt(&t);
            Ok(wrap_dt(micros, true))
        }),
    );
    bc.methods.insert(
        "toString".into(),
        h::method(|_v, t, _a, _n| {
            let (micros, is_utc) = as_dt(&t);
            Ok(Value::str(format_datetime(micros, is_utc, false)))
        }),
    );
    bc.methods.insert(
        "toIso8601String".into(),
        h::method(|_v, t, _a, _n| {
            let (micros, is_utc) = as_dt(&t);
            Ok(Value::str(format_datetime(micros, is_utc, true)))
        }),
    );
    bc.getters.insert(
        "year".into(),
        h::getter(|_v, t| {
            let (micros, is_utc) = as_dt(&t);
            Ok(Value::Int(micros_to_parts(micros, is_utc).0))
        }),
    );
    bc.getters.insert(
        "month".into(),
        h::getter(|_v, t| {
            let (micros, is_utc) = as_dt(&t);
            Ok(Value::Int(micros_to_parts(micros, is_utc).1))
        }),
    );
    bc.getters.insert(
        "day".into(),
        h::getter(|_v, t| {
            let (micros, is_utc) = as_dt(&t);
            Ok(Value::Int(micros_to_parts(micros, is_utc).2))
        }),
    );
    bc.getters.insert(
        "hour".into(),
        h::getter(|_v, t| {
            let (micros, is_utc) = as_dt(&t);
            Ok(Value::Int(micros_to_parts(micros, is_utc).3))
        }),
    );
    bc.getters.insert(
        "minute".into(),
        h::getter(|_v, t| {
            let (micros, is_utc) = as_dt(&t);
            Ok(Value::Int(micros_to_parts(micros, is_utc).4))
        }),
    );
    bc.getters.insert(
        "second".into(),
        h::getter(|_v, t| {
            let (micros, is_utc) = as_dt(&t);
            Ok(Value::Int(micros_to_parts(micros, is_utc).5))
        }),
    );
    bc.getters.insert(
        "millisecond".into(),
        h::getter(|_v, t| {
            let (micros, is_utc) = as_dt(&t);
            Ok(Value::Int(micros_to_parts(micros, is_utc).6))
        }),
    );
    bc.getters.insert(
        "microsecond".into(),
        h::getter(|_v, t| {
            let (micros, is_utc) = as_dt(&t);
            Ok(Value::Int(micros_to_parts(micros, is_utc).7))
        }),
    );
    bc.getters.insert(
        "weekday".into(),
        h::getter(|_v, t| {
            let (micros, is_utc) = as_dt(&t);
            let dt = dt_of(micros, is_utc);
            let sunday_based = dt.weekday().num_days_from_sunday() as i64;
            Ok(Value::Int(if sunday_based == 0 { 7 } else { sunday_based }))
        }),
    );
    bc.getters.insert(
        "millisecondsSinceEpoch".into(),
        h::getter(|_v, t| Ok(Value::Int(as_dt(&t).0 / 1000))),
    );
    bc.getters.insert(
        "microsecondsSinceEpoch".into(),
        h::getter(|_v, t| Ok(Value::Int(as_dt(&t).0))),
    );
    bc.getters.insert(
        "timeZoneName".into(),
        h::getter(|_v, t| {
            let (_, is_utc) = as_dt(&t);
            if is_utc {
                return Ok(Value::str("UTC"));
            }
            let offset = local_offset_seconds();
            let sign = if offset < 0 { "-" } else { "+" };
            let abs = offset.abs();
            Ok(Value::str(format!(
                "{sign}{:02}:{:02}",
                abs / 3600,
                (abs % 3600) / 60
            )))
        }),
    );
    bc.getters.insert(
        "timeZoneOffset".into(),
        h::getter(|_v, t| {
            let (_, is_utc) = as_dt(&t);
            let offset = if is_utc { 0 } else { local_offset_seconds() };
            Ok(h::wrap(
                &h::class("Duration"),
                Value::Int(offset * 1_000_000),
            ))
        }),
    );
    bc.getters.insert(
        "isUtc".into(),
        h::getter(|_v, t| Ok(Value::Bool(as_dt(&t).1))),
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
