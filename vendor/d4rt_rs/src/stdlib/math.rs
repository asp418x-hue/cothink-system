use super::helpers as h;
use crate::callable::NativeFunction;
use crate::env::Environment;
use crate::error::InterpError;
use crate::rt::FunctionRuntimeType;
use crate::value::Value;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

fn define_bridge(env: &Rc<RefCell<Environment>>, bc: crate::class::BridgedClassDef) {
    let bc = Arc::new(bc);
    h::register_class(bc.clone());
    Environment::define_bridge(env, bc);
}

fn define_native(
    env: &Rc<RefCell<Environment>>,
    name: &'static str,
    min_arity: usize,
    max_arity: Option<usize>,
    closure: impl Fn(
        &mut crate::interp::InterpreterVisitor,
        Vec<Value>,
        HashMap<String, Value>,
    ) -> Result<Value, InterpError>
    + 'static,
) {
    Environment::define(
        env,
        name,
        Value::NativeFn(Rc::new(NativeFunction {
            name: Some(name.to_string()),
            min_arity,
            max_arity,
            is_async: false,
            is_generator: false,
            closure: Box::new(closure),
            callable_runtime_type: FunctionRuntimeType::untyped(),
        })),
    );
}

pub fn load(env: &Rc<RefCell<Environment>>) {
    let consts: &[(&str, f64)] = &[
        ("pi", std::f64::consts::PI),
        ("e", std::f64::consts::E),
        ("sqrt2", std::f64::consts::SQRT_2),
        ("sqrt1_2", std::f64::consts::FRAC_1_SQRT_2),
        ("ln2", std::f64::consts::LN_2),
        ("ln10", std::f64::consts::LN_10),
        ("log2e", std::f64::consts::LOG2_E),
        ("log10e", std::f64::consts::LOG10_E),
        ("infinity", f64::INFINITY),
        ("negativeInfinity", f64::NEG_INFINITY),
        ("nan", f64::NAN),
        ("maxFinite", f64::MAX),
        ("minPositive", f64::MIN_POSITIVE),
    ];
    for (name, v) in consts {
        Environment::define(env, name, Value::Double(*v));
    }

    let unary = |name: &'static str, f: fn(f64) -> f64| {
        define_native(env, name, 1, Some(1), move |_v, args, _n| {
            let x = h::arg_double(&args, 0, name)?;
            Ok(Value::Double(f(x)))
        });
    };
    unary("sqrt", f64::sqrt);
    unary("sin", f64::sin);
    unary("cos", f64::cos);
    unary("tan", f64::tan);
    unary("asin", f64::asin);
    unary("acos", f64::acos);
    unary("atan", f64::atan);
    unary("exp", f64::exp);
    unary("log", f64::ln);
    unary("log2", f64::log2);
    unary("log10", f64::log10);

    let binary = |name: &'static str, f: fn(f64, f64) -> f64| {
        define_native(env, name, 2, Some(2), move |_v, args, _n| {
            let a = h::arg_double(&args, 0, name)?;
            let b = h::arg_double(&args, 1, name)?;
            Ok(Value::Double(f(a, b)))
        });
    };
    binary("atan2", f64::atan2);
    binary("pow", f64::powf);

    let min_max = |name: &'static str, f: fn(f64, f64) -> f64| {
        define_native(env, name, 2, Some(2), move |_v, args, _n| {
            let a = h::arg_double(&args, 0, name)?;
            let b = h::arg_double(&args, 1, name)?;
            let r = f(a, b);
            if r == r.trunc() && !r.is_infinite() {
                Ok(Value::Int(r as i64))
            } else {
                Ok(Value::Double(r))
            }
        });
    };
    min_max("min", f64::min);
    min_max("max", f64::max);

    define_bridge(env, random_definition());
    define_bridge(env, point_definition());
    define_bridge(env, rectangle_definition());
    define_bridge(env, mutable_rectangle_definition());
}

fn point_coords(v: &Value) -> Option<(Value, Value)> {
    let l = h::as_list(v)?;
    let l = l.borrow();
    Some((l.first().cloned()?, l.get(1).cloned()?))
}

fn rectangle_vals(v: &Value) -> Option<(Value, Value, Value, Value)> {
    let l = h::as_list(v)?;
    let l = l.borrow();
    Some((
        l.first().cloned()?,
        l.get(1).cloned()?,
        l.get(2).cloned()?,
        l.get(3).cloned()?,
    ))
}

pub fn random_definition() -> crate::class::BridgedClassDef {
    let mut bc = crate::class::BridgedClassDef::new("Random");
    bc.native_type = Some("d4rt::math::Random".to_string());
    bc.constructors.insert(
        "".into(),
        h::ctor(|_v, args, named| {
            let seed = args.first().cloned().or_else(|| named.get("seed").cloned());
            let seed = match seed {
                Some(s) => h::as_int(&s).ok_or_else(|| h::err("Random seed must be an int"))?,
                None => std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_nanos() as i64)
                    .unwrap_or(0),
            };
            let state = splitmix64(seed as u64);
            Ok(h::wrap(
                &h::class("Random"),
                Value::List(Rc::new(RefCell::new(vec![Value::Int(state as i64)]))),
            ))
        }),
    );
    bc.constructors.insert(
        "secure".into(),
        h::ctor(|_v, _a, _n| {
            let seed = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos() as i64)
                .unwrap_or(0);
            let state = splitmix64(seed as u64);
            Ok(h::wrap(
                &h::class("Random"),
                Value::List(Rc::new(RefCell::new(vec![Value::Int(state as i64)]))),
            ))
        }),
    );
    bc.methods.insert(
        "nextInt".into(),
        h::method(|_v, t, args, _n| {
            let max = h::arg_int(&args, 0, "nextInt")?;
            if max <= 0 {
                return Err(h::err("nextInt: max must be positive"));
            }
            let mut state = rand_state(&t)? as u64;
            state = splitmix64(state);
            let v = (state % (max as u64)) as i64;
            rand_set_state(&t, state as i64)?;
            Ok(Value::Int(v))
        }),
    );
    bc.methods.insert(
        "nextDouble".into(),
        h::method(|_v, t, _args, _n| {
            let mut state = rand_state(&t)? as u64;
            state = splitmix64(state);
            let v = (state >> 11) as f64 * (1.0 / (1u64 << 53) as f64);
            rand_set_state(&t, state as i64)?;
            Ok(Value::Double(v))
        }),
    );
    bc.methods.insert(
        "nextBool".into(),
        h::method(|_v, t, _args, _n| {
            let mut state = rand_state(&t)? as u64;
            state = splitmix64(state);
            let v = (state & 1) == 1;
            rand_set_state(&t, state as i64)?;
            Ok(Value::Bool(v))
        }),
    );
    bc
}

fn rand_state(t: &Value) -> Result<i64, InterpError> {
    let l = h::as_list(t).ok_or_else(|| h::err("Invalid Random state"))?;
    let l = l.borrow();
    h::as_int(&l[0]).ok_or_else(|| h::err("Invalid Random state"))
}

fn rand_set_state(t: &Value, state: i64) -> Result<(), InterpError> {
    let l = h::as_list(t).ok_or_else(|| h::err("Invalid Random state"))?;
    l.borrow_mut()[0] = Value::Int(state);
    Ok(())
}

fn splitmix64(mut x: u64) -> u64 {
    x = x.wrapping_add(0x9E3779B97F4A7C15);
    let mut z = x;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D049BB133111EB);
    z ^ (z >> 31)
}

pub fn point_definition() -> crate::class::BridgedClassDef {
    let mut bc = crate::class::BridgedClassDef::new("Point");
    bc.native_type = Some("d4rt::math::Point".to_string());
    bc.is_subtype_of_fn = Some(Box::new(|v| {
        h::as_list(v).is_some() && h::as_list(v).unwrap().borrow().len() == 2
    }));
    bc.constructors.insert(
        "".into(),
        h::ctor(|_v, args, _n| {
            h::require_args(&args, 2, "Point")?;
            Ok(h::wrap(
                &h::class("Point"),
                Value::List(Rc::new(RefCell::new(vec![
                    args[0].clone(),
                    args[1].clone(),
                ]))),
            ))
        }),
    );
    bc.getters.insert(
        "x".into(),
        h::getter(|_v, t| {
            point_coords(&t)
                .map(|(x, _)| x)
                .ok_or_else(|| h::err("Point has no 'x'"))
        }),
    );
    bc.getters.insert(
        "y".into(),
        h::getter(|_v, t| {
            point_coords(&t)
                .map(|(_, y)| y)
                .ok_or_else(|| h::err("Point has no 'y'"))
        }),
    );
    bc.getters.insert(
        "magnitude".into(),
        h::getter(|_v, t| {
            let (x, y) = point_coords(&t).ok_or_else(|| h::err("Invalid Point"))?;
            let dx = h::as_double(&x).ok_or_else(|| h::err("Invalid Point x"))?;
            let dy = h::as_double(&y).ok_or_else(|| h::err("Invalid Point y"))?;
            Ok(Value::Double((dx * dx + dy * dy).sqrt()))
        }),
    );
    bc.methods.insert(
        "distanceTo".into(),
        h::method(|_v, t, args, _n| {
            h::require_args(&args, 1, "distanceTo")?;
            let (ax, ay) = point_coords(&t).ok_or_else(|| h::err("Invalid Point"))?;
            let (bx, by) = point_coords(&args[0]).ok_or_else(|| h::err("Invalid Point"))?;
            let dx = h::as_double(&ax).ok_or_else(|| h::err("Invalid Point x"))?
                - h::as_double(&bx).ok_or_else(|| h::err("Invalid Point x"))?;
            let dy = h::as_double(&ay).ok_or_else(|| h::err("Invalid Point y"))?
                - h::as_double(&by).ok_or_else(|| h::err("Invalid Point y"))?;
            Ok(Value::Double((dx * dx + dy * dy).sqrt()))
        }),
    );
    bc.methods.insert(
        "squaredDistanceTo".into(),
        h::method(|_v, t, args, _n| {
            h::require_args(&args, 1, "squaredDistanceTo")?;
            let (ax, ay) = point_coords(&t).ok_or_else(|| h::err("Invalid Point"))?;
            let (bx, by) = point_coords(&args[0]).ok_or_else(|| h::err("Invalid Point"))?;
            let dx = h::as_double(&ax).ok_or_else(|| h::err("Invalid Point x"))?
                - h::as_double(&bx).ok_or_else(|| h::err("Invalid Point x"))?;
            let dy = h::as_double(&ay).ok_or_else(|| h::err("Invalid Point y"))?
                - h::as_double(&by).ok_or_else(|| h::err("Invalid Point y"))?;
            Ok(Value::Double(dx * dx + dy * dy))
        }),
    );
    bc.methods.insert(
        "+".into(),
        h::method(|_v, t, args, _n| {
            h::require_args(&args, 1, "operator +")?;
            let (ax, ay) = point_coords(&t).ok_or_else(|| h::err("Invalid Point"))?;
            let (bx, by) = point_coords(&args[0]).ok_or_else(|| h::err("Invalid Point"))?;
            let x = num_add(&ax, &bx).ok_or_else(|| h::err("Cannot add Point coordinates"))?;
            let y = num_add(&ay, &by).ok_or_else(|| h::err("Cannot add Point coordinates"))?;
            Ok(h::wrap(
                &h::class("Point"),
                Value::List(Rc::new(RefCell::new(vec![x, y]))),
            ))
        }),
    );
    bc.methods.insert(
        "-".into(),
        h::method(|_v, t, args, _n| {
            h::require_args(&args, 1, "operator -")?;
            let (ax, ay) = point_coords(&t).ok_or_else(|| h::err("Invalid Point"))?;
            let (bx, by) = point_coords(&args[0]).ok_or_else(|| h::err("Invalid Point"))?;
            let x = num_sub(&ax, &bx).ok_or_else(|| h::err("Cannot subtract Point coordinates"))?;
            let y = num_sub(&ay, &by).ok_or_else(|| h::err("Cannot subtract Point coordinates"))?;
            Ok(h::wrap(
                &h::class("Point"),
                Value::List(Rc::new(RefCell::new(vec![x, y]))),
            ))
        }),
    );
    bc.methods.insert(
        "*".into(),
        h::method(|_v, t, args, _n| {
            h::require_args(&args, 1, "operator *")?;
            let (ax, ay) = point_coords(&t).ok_or_else(|| h::err("Invalid Point"))?;
            let factor = args[0].clone();
            let x = num_mul(&ax, &factor).ok_or_else(|| h::err("Cannot scale Point"))?;
            let y = num_mul(&ay, &factor).ok_or_else(|| h::err("Cannot scale Point"))?;
            Ok(h::wrap(
                &h::class("Point"),
                Value::List(Rc::new(RefCell::new(vec![x, y]))),
            ))
        }),
    );
    bc.methods.insert(
        "==".into(),
        h::method(|_v, t, args, _n| {
            let other = args.first().cloned().unwrap_or(Value::Null);
            let eq = match &other {
                Value::Bridged(_) => {
                    let (ax, ay) = point_coords(&t).ok_or_else(|| h::err("Invalid Point"))?;
                    let (bx, by) = point_coords(&other).ok_or_else(|| h::err("Invalid Point"))?;
                    crate::value::value_eq(&ax, &bx) && crate::value::value_eq(&ay, &by)
                }
                _ => false,
            };
            Ok(Value::Bool(eq))
        }),
    );
    bc.methods.insert(
        "hashCode".into(),
        h::method(|_v, t, _args, _n| {
            let (x, y) = point_coords(&t).ok_or_else(|| h::err("Invalid Point"))?;
            let mut result = 17;
            result = 37 * result + h::hash_code(&x);
            result = 37 * result + h::hash_code(&y);
            Ok(Value::Int(result))
        }),
    );
    bc.methods.insert(
        "toString".into(),
        h::method(|_v, t, _args, _n| {
            let (x, y) = point_coords(&t).ok_or_else(|| h::err("Invalid Point"))?;
            Ok(h::str(format!(
                "Point({}, {})",
                h::display(&x),
                h::display(&y)
            )))
        }),
    );
    bc
}

fn point_bridge() -> Arc<crate::class::BridgedClassDef> {
    h::class("Point")
}

pub fn rectangle_definition() -> crate::class::BridgedClassDef {
    let mut bc = crate::class::BridgedClassDef::new("Rectangle");
    bc.native_type = Some("d4rt::math::Rectangle".to_string());
    bc.is_subtype_of_fn = Some(Box::new(|v| {
        h::as_list(v).is_some_and(|l| l.borrow().len() == 4)
    }));
    bc.constructors.insert(
        "".into(),
        h::ctor(|_v, args, _n| {
            h::require_args(&args, 4, "Rectangle")?;
            Ok(h::wrap(
                &h::class("Rectangle"),
                Value::List(Rc::new(RefCell::new(args.to_vec()))),
            ))
        }),
    );
    let rect_getter = |t: &Value, name: &str, i: usize| -> Result<Value, InterpError> {
        let r = rectangle_vals(t).ok_or_else(|| h::err(format!("Rectangle has no '{name}'")))?;
        Ok([r.0, r.1, r.2, r.3][i].clone())
    };
    let rl = rect_getter;
    let rt = rect_getter;
    let rw = rect_getter;
    let rh = rect_getter;
    bc.getters
        .insert("left".into(), h::getter(move |_v, t| rl(&t, "left", 0)));
    bc.getters
        .insert("top".into(), h::getter(move |_v, t| rt(&t, "top", 1)));
    bc.getters
        .insert("width".into(), h::getter(move |_v, t| rw(&t, "width", 2)));
    bc.getters
        .insert("height".into(), h::getter(move |_v, t| rh(&t, "height", 3)));
    bc.getters.insert(
        "right".into(),
        h::getter(|_v, t| {
            let (l, _, w, _) = rectangle_vals(&t).ok_or_else(|| h::err("Invalid Rectangle"))?;
            num_add(&l, &w).ok_or_else(|| h::err("Cannot compute right"))
        }),
    );
    bc.getters.insert(
        "bottom".into(),
        h::getter(|_v, t| {
            let (_, t_, _, hgt) = rectangle_vals(&t).ok_or_else(|| h::err("Invalid Rectangle"))?;
            num_add(&t_, &hgt).ok_or_else(|| h::err("Cannot compute bottom"))
        }),
    );
    bc.getters.insert(
        "isEmpty".into(),
        h::getter(|_v, t| {
            let (_, _, w, hgt) = rectangle_vals(&t).ok_or_else(|| h::err("Invalid Rectangle"))?;
            let w = h::as_double(&w).ok_or_else(|| h::err("Invalid Rectangle width"))?;
            let h = h::as_double(&hgt).ok_or_else(|| h::err("Invalid Rectangle height"))?;
            Ok(Value::Bool(w <= 0.0 || h <= 0.0))
        }),
    );
    bc.getters.insert(
        "topLeft".into(),
        h::getter(|_v, t| {
            let (l, t_, _, _) = rectangle_vals(&t).ok_or_else(|| h::err("Invalid Rectangle"))?;
            Ok(h::wrap(
                &point_bridge(),
                Value::List(Rc::new(RefCell::new(vec![l, t_]))),
            ))
        }),
    );
    bc.getters.insert(
        "topRight".into(),
        h::getter(|_v, t| {
            let (l, t_, w, _) = rectangle_vals(&t).ok_or_else(|| h::err("Invalid Rectangle"))?;
            let r = num_add(&l, &w).ok_or_else(|| h::err("Cannot compute right"))?;
            Ok(h::wrap(
                &point_bridge(),
                Value::List(Rc::new(RefCell::new(vec![r, t_]))),
            ))
        }),
    );
    bc.getters.insert(
        "bottomLeft".into(),
        h::getter(|_v, t| {
            let (l, t_, _, hgt) = rectangle_vals(&t).ok_or_else(|| h::err("Invalid Rectangle"))?;
            let b = num_add(&t_, &hgt).ok_or_else(|| h::err("Cannot compute bottom"))?;
            Ok(h::wrap(
                &point_bridge(),
                Value::List(Rc::new(RefCell::new(vec![l, b]))),
            ))
        }),
    );
    bc.getters.insert(
        "bottomRight".into(),
        h::getter(|_v, t| {
            let (l, t_, w, hgt) = rectangle_vals(&t).ok_or_else(|| h::err("Invalid Rectangle"))?;
            let r = num_add(&l, &w).ok_or_else(|| h::err("Cannot compute right"))?;
            let b = num_add(&t_, &hgt).ok_or_else(|| h::err("Cannot compute bottom"))?;
            Ok(h::wrap(
                &point_bridge(),
                Value::List(Rc::new(RefCell::new(vec![r, b]))),
            ))
        }),
    );
    bc.methods.insert(
        "contains".into(),
        h::method(|_v, t, args, _n| {
            h::require_args(&args, 1, "contains")?;
            Ok(Value::Bool(rect_contains(&t, &args[0])?))
        }),
    );
    bc.methods.insert(
        "containsPoint".into(),
        h::method(|_v, t, args, _n| {
            h::require_args(&args, 1, "containsPoint")?;
            Ok(Value::Bool(rect_contains(&t, &args[0])?))
        }),
    );
    bc.methods.insert(
        "intersects".into(),
        h::method(|_v, t, args, _n| {
            h::require_args(&args, 1, "intersects")?;
            let (l1, t1, w1, h1) = rectangle_vals(&t).ok_or_else(|| h::err("Invalid Rectangle"))?;
            let (l2, t2, w2, h2) =
                rectangle_vals(&args[0]).ok_or_else(|| h::err("Invalid Rectangle"))?;
            let (l1, t1, w1, h1) = (
                num_double(&l1).ok_or_else(|| h::err("Invalid Rectangle"))?,
                num_double(&t1).ok_or_else(|| h::err("Invalid Rectangle"))?,
                num_double(&w1).ok_or_else(|| h::err("Invalid Rectangle"))?,
                num_double(&h1).ok_or_else(|| h::err("Invalid Rectangle"))?,
            );
            let (l2, t2, w2, h2) = (
                num_double(&l2).ok_or_else(|| h::err("Invalid Rectangle"))?,
                num_double(&t2).ok_or_else(|| h::err("Invalid Rectangle"))?,
                num_double(&w2).ok_or_else(|| h::err("Invalid Rectangle"))?,
                num_double(&h2).ok_or_else(|| h::err("Invalid Rectangle"))?,
            );
            let r1 = l1 + w1;
            let b1 = t1 + h1;
            let r2 = l2 + w2;
            let b2 = t2 + h2;
            Ok(Value::Bool(l1 < r2 && l2 < r1 && t1 < b2 && t2 < b1))
        }),
    );
    bc.methods.insert(
        "intersection".into(),
        h::method(|_v, t, args, _n| {
            h::require_args(&args, 1, "intersection")?;
            let (l1, t1, w1, h1) = rectangle_vals(&t).ok_or_else(|| h::err("Invalid Rectangle"))?;
            let (l2, t2, w2, h2) =
                rectangle_vals(&args[0]).ok_or_else(|| h::err("Invalid Rectangle"))?;
            let (l1, t1, w1, h1) = (
                num_double(&l1).ok_or_else(|| h::err("Invalid Rectangle"))?,
                num_double(&t1).ok_or_else(|| h::err("Invalid Rectangle"))?,
                num_double(&w1).ok_or_else(|| h::err("Invalid Rectangle"))?,
                num_double(&h1).ok_or_else(|| h::err("Invalid Rectangle"))?,
            );
            let (l2, t2, w2, h2) = (
                num_double(&l2).ok_or_else(|| h::err("Invalid Rectangle"))?,
                num_double(&t2).ok_or_else(|| h::err("Invalid Rectangle"))?,
                num_double(&w2).ok_or_else(|| h::err("Invalid Rectangle"))?,
                num_double(&h2).ok_or_else(|| h::err("Invalid Rectangle"))?,
            );
            let r1 = l1 + w1;
            let b1 = t1 + h1;
            let r2 = l2 + w2;
            let b2 = t2 + h2;
            let left = l1.max(l2);
            let top = t1.max(t2);
            let right = r1.min(r2);
            let bottom = b1.min(b2);
            if left >= right || top >= bottom {
                return Ok(Value::Null);
            }
            Ok(h::wrap(
                &h::class("Rectangle"),
                Value::List(Rc::new(RefCell::new(vec![
                    Value::Double(left),
                    Value::Double(top),
                    Value::Double(right - left),
                    Value::Double(bottom - top),
                ]))),
            ))
        }),
    );
    bc.methods.insert(
        "boundingBox".into(),
        h::method(|_v, t, args, _n| {
            h::require_args(&args, 1, "boundingBox")?;
            let (l1, t1, w1, h1) = rectangle_vals(&t).ok_or_else(|| h::err("Invalid Rectangle"))?;
            let (l2, t2, w2, h2) =
                rectangle_vals(&args[0]).ok_or_else(|| h::err("Invalid Rectangle"))?;
            let (l1, t1, w1, h1) = (
                num_double(&l1).ok_or_else(|| h::err("Invalid Rectangle"))?,
                num_double(&t1).ok_or_else(|| h::err("Invalid Rectangle"))?,
                num_double(&w1).ok_or_else(|| h::err("Invalid Rectangle"))?,
                num_double(&h1).ok_or_else(|| h::err("Invalid Rectangle"))?,
            );
            let (l2, t2, w2, h2) = (
                num_double(&l2).ok_or_else(|| h::err("Invalid Rectangle"))?,
                num_double(&t2).ok_or_else(|| h::err("Invalid Rectangle"))?,
                num_double(&w2).ok_or_else(|| h::err("Invalid Rectangle"))?,
                num_double(&h2).ok_or_else(|| h::err("Invalid Rectangle"))?,
            );
            let left = l1.min(l2);
            let top = t1.min(t2);
            let right = (l1 + w1).max(l2 + w2);
            let bottom = (t1 + h1).max(t2 + h2);
            Ok(h::wrap(
                &h::class("Rectangle"),
                Value::List(Rc::new(RefCell::new(vec![
                    Value::Double(left),
                    Value::Double(top),
                    Value::Double(right - left),
                    Value::Double(bottom - top),
                ]))),
            ))
        }),
    );
    bc.methods.insert(
        "toString".into(),
        h::method(|_v, t, _args, _n| {
            let (l, t_, w, h) = rectangle_vals(&t).ok_or_else(|| h::err("Invalid Rectangle"))?;
            Ok(h::str(format!(
                "Rectangle({}, {}, {}, {})",
                h::display(&l),
                h::display(&t_),
                h::display(&w),
                h::display(&h)
            )))
        }),
    );
    bc
}

fn num_double(v: &Value) -> Option<f64> {
    h::as_double(v)
}

fn rect_contains(t: &Value, p: &Value) -> Result<bool, InterpError> {
    let (l, t_, w, hgt) = rectangle_vals(t).ok_or_else(|| h::err("Invalid Rectangle"))?;
    let (px, py) = point_coords(p).ok_or_else(|| h::err("Invalid Point"))?;
    let l = h::as_double(&l).ok_or_else(|| h::err("Invalid Rectangle left"))?;
    let t = h::as_double(&t_).ok_or_else(|| h::err("Invalid Rectangle top"))?;
    let w = h::as_double(&w).ok_or_else(|| h::err("Invalid Rectangle width"))?;
    let h = h::as_double(&hgt).ok_or_else(|| h::err("Invalid Rectangle height"))?;
    let px = h::as_double(&px).ok_or_else(|| h::err("Invalid Point x"))?;
    let py = h::as_double(&py).ok_or_else(|| h::err("Invalid Point y"))?;
    Ok(px >= l && px <= l + w && py >= t && py <= t + h)
}

fn num_add(a: &Value, b: &Value) -> Option<Value> {
    crate::value::num_add(a, b)
}
fn num_sub(a: &Value, b: &Value) -> Option<Value> {
    crate::value::num_sub(a, b)
}
fn num_mul(a: &Value, b: &Value) -> Option<Value> {
    crate::value::num_mul(a, b)
}

pub fn mutable_rectangle_definition() -> crate::class::BridgedClassDef {
    let mut bc = crate::class::BridgedClassDef::new("MutableRectangle");
    bc.native_type = Some("d4rt::math::MutableRectangle".to_string());
    bc.is_subtype_of_fn = Some(Box::new(|v| {
        h::as_list(v).is_some_and(|l| l.borrow().len() == 4)
    }));
    bc.constructors.insert(
        "".into(),
        h::ctor(|_v, args, _n| {
            h::require_args(&args, 4, "MutableRectangle")?;
            Ok(h::wrap(
                &h::class("MutableRectangle"),
                Value::List(Rc::new(RefCell::new(args.to_vec()))),
            ))
        }),
    );
    let set_i = |v: &Value, i: usize, new: &Value| -> Result<(), InterpError> {
        let l = h::as_list(v).ok_or_else(|| h::err("Invalid MutableRectangle"))?;
        let mut l = l.borrow_mut();
        if l.len() != 4 {
            return Err(h::err("Invalid MutableRectangle"));
        }
        l[i] = new.clone();
        Ok(())
    };
    let get_i = |v: &Value, i: usize| -> Option<Value> {
        let l = h::as_list(v)?;
        l.borrow().get(i).cloned()
    };
    bc.getters.insert(
        "left".into(),
        h::getter(move |_v, t| get_i(&t, 0).ok_or_else(|| h::err("Invalid MutableRectangle"))),
    );
    bc.getters.insert(
        "top".into(),
        h::getter(move |_v, t| get_i(&t, 1).ok_or_else(|| h::err("Invalid MutableRectangle"))),
    );
    bc.getters.insert(
        "width".into(),
        h::getter(move |_v, t| get_i(&t, 2).ok_or_else(|| h::err("Invalid MutableRectangle"))),
    );
    bc.getters.insert(
        "height".into(),
        h::getter(move |_v, t| get_i(&t, 3).ok_or_else(|| h::err("Invalid MutableRectangle"))),
    );
    bc.getters.insert(
        "right".into(),
        h::getter(move |_v, t| {
            let l = get_i(&t, 0).ok_or_else(|| h::err("Invalid MutableRectangle"))?;
            let w = get_i(&t, 2).ok_or_else(|| h::err("Invalid MutableRectangle"))?;
            num_add(&l, &w).ok_or_else(|| h::err("Cannot compute right"))
        }),
    );
    bc.getters.insert(
        "bottom".into(),
        h::getter(move |_v, t| {
            let t_ = get_i(&t, 1).ok_or_else(|| h::err("Invalid MutableRectangle"))?;
            let hgt = get_i(&t, 3).ok_or_else(|| h::err("Invalid MutableRectangle"))?;
            num_add(&t_, &hgt).ok_or_else(|| h::err("Cannot compute bottom"))
        }),
    );
    bc.setters
        .insert("left".into(), h::setter(move |_v, t, v| set_i(&t, 0, &v)));
    bc.setters
        .insert("top".into(), h::setter(move |_v, t, v| set_i(&t, 1, &v)));
    bc.setters
        .insert("width".into(), h::setter(move |_v, t, v| set_i(&t, 2, &v)));
    bc.setters
        .insert("height".into(), h::setter(move |_v, t, v| set_i(&t, 3, &v)));
    bc.methods.insert(
        "toString".into(),
        h::method(move |_v, t, _args, _n| {
            Ok(h::str(format!(
                "MutableRectangle({}, {}, {}, {})",
                h::display(&get_i(&t, 0).unwrap_or(Value::Null)),
                h::display(&get_i(&t, 1).unwrap_or(Value::Null)),
                h::display(&get_i(&t, 2).unwrap_or(Value::Null)),
                h::display(&get_i(&t, 3).unwrap_or(Value::Null)),
            )))
        }),
    );
    bc
}
