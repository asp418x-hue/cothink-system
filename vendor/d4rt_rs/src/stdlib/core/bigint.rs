use super::super::helpers as h;
use crate::class::BridgedClassDef;
use crate::value::Value;
use num_integer::Integer;
use num_traits::{Signed, ToPrimitive};

fn as_big(v: &Value) -> num_bigint::BigInt {
    h::as_bigint(v).unwrap_or_default()
}

fn wrap_big(b: num_bigint::BigInt) -> Value {
    h::wrap(&h::class("BigInt"), Value::BigInt(b))
}

fn mod_inverse(a: &num_bigint::BigInt, m: &num_bigint::BigInt) -> Option<num_bigint::BigInt> {
    if m.sign() == num_bigint::Sign::NoSign {
        return None;
    }
    let (mut t, mut new_t) = (num_bigint::BigInt::from(0), num_bigint::BigInt::from(1));
    let (mut r, mut new_r) = (m.clone(), a % m);
    while new_r != num_bigint::BigInt::from(0) {
        let q = &r / &new_r;
        let next_t = t - &q * &new_t;
        let next_r = r - &q * &new_r;
        t = new_t;
        new_t = next_t;
        r = new_r;
        new_r = next_r;
    }
    if r != num_bigint::BigInt::from(1) {
        return None;
    }
    if t.sign() == num_bigint::Sign::Minus {
        t += m;
    }
    Some(t % m)
}

pub fn bigint_definition() -> BridgedClassDef {
    let mut bc = BridgedClassDef::new("BigInt");
    bc.static_methods.insert(
        "from".into(),
        h::static_method(|_v, args, _n| {
            let v = args.first().cloned().unwrap_or(Value::Null);
            match v {
                Value::Int(i) => Ok(wrap_big(num_bigint::BigInt::from(i))),
                Value::Double(d) => Ok(wrap_big(num_bigint::BigInt::from(d as i64))),
                Value::BigInt(b) => Ok(wrap_big(b)),
                other => Err(h::err(format!(
                    "BigInt.from: cannot convert {}",
                    h::display(&other)
                ))),
            }
        }),
    );
    bc.static_methods.insert(
        "parse".into(),
        h::static_method(|_v, args, named| {
            let source = h::arg_str(&args, 0, "parse")?;
            let radix = named.get("radix").and_then(h::as_int);
            let result = match radix {
                Some(r) if (2..=36).contains(&r) => {
                    let t = source.trim();
                    let sign = t.strip_prefix('-');
                    let digits = sign
                        .unwrap_or(t)
                        .strip_prefix('+')
                        .unwrap_or(sign.unwrap_or(t));
                    num_bigint::BigInt::parse_bytes(digits.as_bytes(), r as u32)
                        .map(|n| if t.starts_with('-') { -n } else { n })
                }
                _ => {
                    let t = source.trim();
                    if t.starts_with("0x") || t.starts_with("0X") {
                        num_bigint::BigInt::parse_bytes(&t.as_bytes()[2..], 16)
                    } else {
                        num_bigint::BigInt::parse_bytes(t.as_bytes(), 10)
                    }
                }
            };
            result
                .map(wrap_big)
                .ok_or_else(|| h::format_exception(format!("Invalid BigInt: {source}")))
        }),
    );
    bc.static_methods.insert(
        "tryParse".into(),
        h::static_method(|_v, args, named| {
            let source = h::arg_str(&args, 0, "tryParse")?;
            let radix = named.get("radix").and_then(h::as_int);
            let result = match radix {
                Some(r) if (2..=36).contains(&r) => {
                    let t = source.trim();
                    let digits = t.strip_prefix(['-', '+']).unwrap_or(t);
                    num_bigint::BigInt::parse_bytes(digits.as_bytes(), r as u32)
                        .map(|n| if t.starts_with('-') { -n } else { n })
                }
                _ => {
                    let t = source.trim();
                    if t.starts_with("0x") || t.starts_with("0X") {
                        num_bigint::BigInt::parse_bytes(&t.as_bytes()[2..], 16)
                    } else {
                        num_bigint::BigInt::parse_bytes(t.as_bytes(), 10)
                    }
                }
            };
            Ok(result.map(wrap_big).unwrap_or(Value::Null))
        }),
    );
    bc.static_getters.insert(
        "zero".into(),
        h::static_getter(|_v| Ok(wrap_big(0u8.into()))),
    );
    bc.static_getters.insert(
        "one".into(),
        h::static_getter(|_v| Ok(wrap_big(1u8.into()))),
    );
    bc.static_getters.insert(
        "two".into(),
        h::static_getter(|_v| Ok(wrap_big(2u8.into()))),
    );
    bc.methods.insert(
        "+".into(),
        h::method(|_v, t, args, _n| {
            Ok(wrap_big(
                as_big(&t) + as_big(&args.first().cloned().unwrap_or(Value::Null)),
            ))
        }),
    );
    bc.methods.insert(
        "-".into(),
        h::method(|_v, t, args, _n| {
            Ok(wrap_big(
                as_big(&t) - as_big(&args.first().cloned().unwrap_or(Value::Null)),
            ))
        }),
    );
    bc.methods.insert(
        "*".into(),
        h::method(|_v, t, args, _n| {
            Ok(wrap_big(
                as_big(&t) * as_big(&args.first().cloned().unwrap_or(Value::Null)),
            ))
        }),
    );
    bc.methods.insert(
        "~/".into(),
        h::method(|_v, t, args, _n| {
            let other = as_big(&args.first().cloned().unwrap_or(Value::Null));
            if other.sign() == num_bigint::Sign::NoSign {
                return Err(h::err("BigInt ~/ zero"));
            }
            Ok(wrap_big(as_big(&t) / other))
        }),
    );
    bc.methods.insert(
        "%".into(),
        h::method(|_v, t, args, _n| {
            let other = as_big(&args.first().cloned().unwrap_or(Value::Null));
            Ok(wrap_big(as_big(&t) % other))
        }),
    );
    bc.methods.insert(
        "remainder".into(),
        h::method(|_v, t, args, _n| {
            Ok(wrap_big(
                as_big(&t) % as_big(&args.first().cloned().unwrap_or(Value::Null)),
            ))
        }),
    );
    bc.methods.insert(
        "pow".into(),
        h::method(|_v, t, args, _n| {
            let e = h::arg_int(&args, 0, "pow")?;
            if e < 0 {
                return Err(h::err("BigInt.pow: negative exponent"));
            }
            Ok(wrap_big(as_big(&t).pow(e as u32)))
        }),
    );
    bc.methods.insert(
        "modPow".into(),
        h::method(|_v, t, args, _n| {
            let e = as_big(&args.first().cloned().unwrap_or(Value::Null));
            let m = as_big(&args.get(1).cloned().unwrap_or(Value::Null));
            if m.sign() == num_bigint::Sign::NoSign {
                return Err(h::err("BigInt.modPow: modulus is zero"));
            }
            Ok(wrap_big(as_big(&t).modpow(&e, &m)))
        }),
    );
    bc.methods.insert(
        "modInverse".into(),
        h::method(|_v, t, args, _n| {
            let m = as_big(&args.first().cloned().unwrap_or(Value::Null));
            match mod_inverse(&as_big(&t), &m) {
                Some(r) => Ok(wrap_big(r)),
                None => Err(h::err("BigInt.modInverse: inverse does not exist")),
            }
        }),
    );
    bc.methods.insert(
        "gcd".into(),
        h::method(|_v, t, args, _n| {
            Ok(wrap_big(as_big(&t).gcd(&as_big(
                &args.first().cloned().unwrap_or(Value::Null),
            ))))
        }),
    );
    bc.methods.insert(
        "abs".into(),
        h::method(|_v, t, _a, _n| Ok(wrap_big(as_big(&t).abs()))),
    );
    bc.methods.insert(
        "compareTo".into(),
        h::method(|_v, t, args, _n| {
            Ok(Value::Int(
                match as_big(&t).cmp(&as_big(&args.first().cloned().unwrap_or(Value::Null))) {
                    std::cmp::Ordering::Less => -1,
                    std::cmp::Ordering::Equal => 0,
                    std::cmp::Ordering::Greater => 1,
                },
            ))
        }),
    );
    bc.methods.insert(
        "toString".into(),
        h::method(|_v, t, _a, _n| Ok(Value::str(as_big(&t).to_string()))),
    );
    bc.methods.insert(
        "toRadixString".into(),
        h::method(|_v, t, args, _n| {
            let radix = h::arg_int(&args, 0, "toRadixString")?;
            if !(2..=36).contains(&radix) {
                return Err(h::err(format!("toRadixString: radix {radix} out of range")));
            }
            Ok(Value::str(as_big(&t).to_str_radix(radix as u32)))
        }),
    );
    bc.methods.insert(
        "toInt".into(),
        h::method(|_v, t, _a, _n| Ok(Value::Int(as_big(&t).to_i64().unwrap_or(0)))),
    );
    bc.methods.insert(
        "toDouble".into(),
        h::method(|_v, t, _a, _n| Ok(Value::Double(as_big(&t).to_f64().unwrap_or(0.0)))),
    );
    bc.methods.insert(
        "toUnsigned".into(),
        h::method(|_v, t, args, _n| {
            let width = h::arg_int(&args, 0, "toUnsigned")?;
            if width <= 0 {
                return Ok(wrap_big(as_big(&t)));
            }
            let mask = (num_bigint::BigInt::from(1u8) << width) - 1u8;
            Ok(wrap_big(as_big(&t) & mask))
        }),
    );
    bc.methods.insert(
        "toSigned".into(),
        h::method(|_v, t, args, _n| {
            let width = h::arg_int(&args, 0, "toSigned")?;
            if width <= 0 {
                return Ok(wrap_big(as_big(&t)));
            }
            let n = as_big(&t);
            let two = num_bigint::BigInt::from(2u8);
            let mod_val = two.pow(width as u32);
            let half = &mod_val / 2u8;
            let mut r = ((&n % &mod_val) + &mod_val) % &mod_val;
            if r >= half {
                r -= &mod_val;
            }
            Ok(wrap_big(r))
        }),
    );
    bc.methods.insert(
        "unary-".into(),
        h::method(|_v, t, _a, _n| Ok(wrap_big(-as_big(&t)))),
    );
    bc.methods.insert(
        "~".into(),
        h::method(|_v, t, _a, _n| Ok(wrap_big(!as_big(&t)))),
    );
    bc.methods.insert(
        "&".into(),
        h::method(|_v, t, args, _n| {
            Ok(wrap_big(
                as_big(&t) & as_big(&args.first().cloned().unwrap_or(Value::Null)),
            ))
        }),
    );
    bc.methods.insert(
        "|".into(),
        h::method(|_v, t, args, _n| {
            Ok(wrap_big(
                as_big(&t) | as_big(&args.first().cloned().unwrap_or(Value::Null)),
            ))
        }),
    );
    bc.methods.insert(
        "^".into(),
        h::method(|_v, t, args, _n| {
            Ok(wrap_big(
                as_big(&t) ^ as_big(&args.first().cloned().unwrap_or(Value::Null)),
            ))
        }),
    );
    bc.methods.insert(
        "<<".into(),
        h::method(|_v, t, args, _n| {
            let shift = h::arg_int(&args, 0, "<<")?;
            if shift < 0 {
                return Err(h::err("BigInt <<: negative shift"));
            }
            Ok(wrap_big(as_big(&t) << shift))
        }),
    );
    bc.methods.insert(
        ">>".into(),
        h::method(|_v, t, args, _n| {
            let shift = h::arg_int(&args, 0, ">>")?;
            if shift < 0 {
                return Err(h::err("BigInt >>: negative shift"));
            }
            Ok(wrap_big(as_big(&t) >> shift))
        }),
    );
    bc.methods.insert(
        "==".into(),
        h::method(|_v, t, args, _n| {
            let other = args.first().cloned().unwrap_or(Value::Null);
            Ok(Value::Bool(as_big(&t) == as_big(&other)))
        }),
    );
    let cmp =
        |op: &'static str, pred: fn(std::cmp::Ordering) -> bool| -> crate::stdlib::helpers::M {
            h::method(move |_v, t, args, _n| {
                let other = args.first().cloned().unwrap_or(Value::Null);
                if h::as_bigint(&other).is_none() {
                    return Err(h::err(format!(
                        "BigInt comparison requires another BigInt ({op})"
                    )));
                }
                let ord = as_big(&t).cmp(&as_big(&other));
                Ok(Value::Bool(pred(ord)))
            })
        };
    bc.methods
        .insert("<".into(), cmp("<", |o| o == std::cmp::Ordering::Less));
    bc.methods
        .insert("<=".into(), cmp("<=", |o| o != std::cmp::Ordering::Greater));
    bc.methods
        .insert(">".into(), cmp(">", |o| o == std::cmp::Ordering::Greater));
    bc.methods
        .insert(">=".into(), cmp(">=", |o| o != std::cmp::Ordering::Less));
    bc.getters.insert(
        "sign".into(),
        h::getter(|_v, t| {
            Ok(Value::Int(match as_big(&t).sign() {
                num_bigint::Sign::Minus => -1,
                num_bigint::Sign::NoSign => 0,
                num_bigint::Sign::Plus => 1,
            }))
        }),
    );
    bc.getters.insert(
        "isEven".into(),
        h::getter(|_v, t| Ok(Value::Bool(&as_big(&t) % 2u8 == 0u8.into()))),
    );
    bc.getters.insert(
        "isOdd".into(),
        h::getter(|_v, t| Ok(Value::Bool(&as_big(&t) % 2u8 != 0u8.into()))),
    );
    bc.getters.insert(
        "isNegative".into(),
        h::getter(|_v, t| Ok(Value::Bool(as_big(&t).sign() == num_bigint::Sign::Minus))),
    );
    bc.getters.insert(
        "bitLength".into(),
        h::getter(|_v, t| {
            let n = as_big(&t);
            let mag = if n.sign() == num_bigint::Sign::Minus {
                !&n
            } else {
                n
            };
            Ok(Value::Int(mag.bits() as i64))
        }),
    );
    bc.getters.insert(
        "isValidInt".into(),
        h::getter(|_v, t| Ok(Value::Bool(as_big(&t).to_i64().is_some()))),
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
