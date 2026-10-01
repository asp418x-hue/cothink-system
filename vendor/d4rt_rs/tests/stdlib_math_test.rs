//! dart:math coverage: constants, functions, Random, Point, Rectangle,
//! plus import handling (`import 'dart:math' as math show ...`).

use d4rt_rs::{Context, Value};

fn d(v: &Value) -> f64 {
    match v {
        Value::Double(x) => *x,
        Value::Int(x) => *x as f64,
        other => panic!("expected number, got {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// Import handling
// ---------------------------------------------------------------------------

#[test]
fn import_dart_math_unprefixed() {
    let mut ctx = Context::new();
    let v = ctx.eval("import 'dart:math'; main() => sqrt(16);").unwrap();
    assert_eq!(v, Value::Int(4));
}

#[test]
fn import_dart_math_prefixed() {
    let mut ctx = Context::new();
    let v = ctx
        .eval("import 'dart:math' as m; main() => m.max(3, 7);")
        .unwrap();
    assert_eq!(v, Value::Int(7));
}

#[test]
fn import_show_hide_clauses() {
    let mut ctx = Context::new();
    // show: only pi is visible
    ctx.eval("import 'dart:math' show pi; void main() {}")
        .unwrap();
    assert!(matches!(ctx.eval_expression("pi"), Ok(Value::Double(_))));
    assert!(ctx.eval_expression("sqrt").is_err());
    // hide: everything visible except sqrt
    let mut ctx2 = Context::new();
    ctx2.eval("import 'dart:math' hide sqrt; void main() {}")
        .unwrap();
    assert!(matches!(ctx2.eval_expression("pi"), Ok(Value::Double(_))));
    assert!(ctx2.eval_expression("sqrt").is_err());
}

#[test]
fn import_prefix_with_show() {
    let mut ctx = Context::new();
    let v = ctx
        .eval("import 'dart:math' as m show pi; main() => m.pi;")
        .unwrap();
    assert!(matches!(v, Value::Double(_)));
    let mut ctx2 = Context::new();
    assert!(
        ctx2.eval("import 'dart:math' as m show pi; main() => m.sqrt(4);")
            .is_err()
    );
}

// ---------------------------------------------------------------------------
// Constants and functions
// ---------------------------------------------------------------------------

#[test]
fn math_constants() {
    let mut ctx = Context::new();
    ctx.eval("import 'dart:math'; void main() {}").unwrap();
    let pi = ctx.eval_expression("pi").unwrap();
    assert!((d(&pi) - std::f64::consts::PI).abs() < 1e-12);
    let e = ctx.eval_expression("e").unwrap();
    assert!((d(&e) - std::f64::consts::E).abs() < 1e-12);
    assert!(matches!(ctx.eval_expression("nan"), Ok(Value::Double(x)) if x.is_nan()));
    assert!(matches!(ctx.eval_expression("infinity"), Ok(Value::Double(x)) if x.is_infinite()));
}

#[test]
fn math_functions() {
    let mut ctx = Context::new();
    ctx.eval("import 'dart:math'; void main() {}").unwrap();
    assert_eq!(ctx.eval_expression("sqrt(16)").unwrap(), Value::Int(4));
    assert!((d(&ctx.eval_expression("sqrt(2)").unwrap()) - 2f64.sqrt()).abs() < 1e-12);
    assert!((d(&ctx.eval_expression("sin(pi / 2)").unwrap()) - 1.0).abs() < 1e-12);
    assert!((d(&ctx.eval_expression("cos(0)").unwrap()) - 1.0).abs() < 1e-12);
    assert!((d(&ctx.eval_expression("exp(0)").unwrap()) - 1.0).abs() < 1e-12);
    assert!((d(&ctx.eval_expression("log(e)").unwrap()) - 1.0).abs() < 1e-12);
    assert!((d(&ctx.eval_expression("pow(2, 10)").unwrap()) - 1024.0).abs() < 1e-9);
    assert!(
        (d(&ctx.eval_expression("atan2(1, 1)").unwrap()) - std::f64::consts::FRAC_PI_4).abs()
            < 1e-12
    );
    assert!(matches!(ctx.eval_expression("sqrt(-1)"), Ok(Value::Double(x)) if x.is_nan()));
    assert!(matches!(ctx.eval_expression("log(-1)"), Ok(Value::Double(x)) if x.is_nan()));
}

#[test]
fn math_min_max() {
    let mut ctx = Context::new();
    ctx.eval("import 'dart:math'; void main() {}").unwrap();
    assert_eq!(ctx.eval_expression("min(3, 7)").unwrap(), Value::Int(3));
    assert_eq!(ctx.eval_expression("max(3, 7)").unwrap(), Value::Int(7));
    assert_eq!(
        ctx.eval_expression("max(3.5, 7)").unwrap(),
        Value::Double(7.0)
    );
    assert_eq!(ctx.eval_expression("min(-2, 2)").unwrap(), Value::Int(-2));
}

// ---------------------------------------------------------------------------
// Random
// ---------------------------------------------------------------------------

#[test]
fn random_with_seed_is_deterministic() {
    let mut ctx = Context::new();
    ctx.eval("import 'dart:math'; void main() {}").unwrap();
    let a = ctx.eval_expression("Random(42).nextInt(100)").unwrap();
    let b = ctx.eval_expression("Random(42).nextInt(100)").unwrap();
    assert_eq!(a, b);
    let r1 = ctx.eval_expression("Random(7).nextInt(1000)").unwrap();
    let r2 = ctx.eval_expression("Random(7).nextInt(1000)").unwrap();
    assert_eq!(r1, r2);
}

#[test]
fn random_ranges() {
    let mut ctx = Context::new();
    ctx.eval("import 'dart:math'; void main() {}").unwrap();
    let r = ctx.eval_expression("Random(1).nextInt(10)").unwrap();
    let i = match r {
        Value::Int(i) => i,
        other => panic!("expected int, got {other:?}"),
    };
    assert!((0..10).contains(&i));
    let r = ctx.eval_expression("Random(1).nextInt(10)").unwrap();
    assert!((0..10).contains(&match r {
        Value::Int(i) => i,
        other => panic!("expected int, got {other:?}"),
    }));
    for _ in 0..20 {
        let rd = d(&ctx.eval_expression("Random(3).nextDouble()").unwrap());
        assert!((0.0..1.0).contains(&rd));
    }
    let b = ctx.eval_expression("Random(3).nextBool()").unwrap();
    assert!(matches!(b, Value::Bool(_)));
}

// ---------------------------------------------------------------------------
// Point
// ---------------------------------------------------------------------------

#[test]
fn point_basics() {
    let mut ctx = Context::new();
    ctx.eval("import 'dart:math'; void main() {}").unwrap();
    assert_eq!(ctx.eval_expression("Point(3, 4).x").unwrap(), Value::Int(3));
    assert_eq!(ctx.eval_expression("Point(3, 4).y").unwrap(), Value::Int(4));
    let m = d(&ctx.eval_expression("Point(3, 4).magnitude").unwrap());
    assert!((m - 5.0).abs() < 1e-12);
    let dist = d(&ctx
        .eval_expression("Point(0, 0).distanceTo(Point(3, 4))")
        .unwrap());
    assert!((dist - 5.0).abs() < 1e-12);
    let sq = d(&ctx
        .eval_expression("Point(0, 0).squaredDistanceTo(Point(3, 4))")
        .unwrap());
    assert!((sq - 25.0).abs() < 1e-12);
}

#[test]
fn point_operators() {
    let mut ctx = Context::new();
    ctx.eval("import 'dart:math'; void main() {}").unwrap();
    assert_eq!(
        ctx.eval_expression("(Point(1, 2) + Point(3, 4)).x")
            .unwrap(),
        Value::Int(4)
    );
    assert_eq!(
        ctx.eval_expression("(Point(3, 4) - Point(1, 2)).y")
            .unwrap(),
        Value::Int(2)
    );
    assert_eq!(
        ctx.eval_expression("(Point(2, 3) * 2).x").unwrap(),
        Value::Int(4)
    );
    assert_eq!(
        ctx.eval_expression("Point(1, 2) == Point(1, 2)").unwrap(),
        Value::Bool(true)
    );
    assert_eq!(
        ctx.eval_expression("Point(1, 2) == Point(2, 1)").unwrap(),
        Value::Bool(false)
    );
}

// ---------------------------------------------------------------------------
// Rectangle
// ---------------------------------------------------------------------------

#[test]
fn rectangle_basics() {
    let mut ctx = Context::new();
    ctx.eval("import 'dart:math'; void main() {}").unwrap();
    let _r = ctx.eval_expression("Rectangle(10, 20, 30, 40)").unwrap();
    assert_eq!(
        ctx.eval_expression("Rectangle(10, 20, 30, 40).left")
            .unwrap(),
        Value::Int(10)
    );
    assert_eq!(
        ctx.eval_expression("Rectangle(10, 20, 30, 40).top")
            .unwrap(),
        Value::Int(20)
    );
    assert_eq!(
        ctx.eval_expression("Rectangle(10, 20, 30, 40).right")
            .unwrap(),
        Value::Int(40)
    );
    assert_eq!(
        ctx.eval_expression("Rectangle(10, 20, 30, 40).bottom")
            .unwrap(),
        Value::Int(60)
    );
    assert_eq!(
        ctx.eval_expression("Rectangle(10, 20, 30, 40).width")
            .unwrap(),
        Value::Int(30)
    );
    assert_eq!(
        ctx.eval_expression("Rectangle(10, 20, 30, 40).height")
            .unwrap(),
        Value::Int(40)
    );
    assert_eq!(
        ctx.eval_expression("Rectangle(10, 20, 30, 40).topLeft == Point(10, 20)")
            .unwrap(),
        Value::Bool(true)
    );
}

#[test]
fn rectangle_contains_and_intersects() {
    let mut ctx = Context::new();
    ctx.eval("import 'dart:math'; void main() {}").unwrap();
    assert_eq!(
        ctx.eval_expression("Rectangle(0, 0, 10, 10).contains(Point(5, 5))")
            .unwrap(),
        Value::Bool(true)
    );
    assert_eq!(
        ctx.eval_expression("Rectangle(0, 0, 10, 10).contains(Point(15, 5))")
            .unwrap(),
        Value::Bool(false)
    );
    assert_eq!(
        ctx.eval_expression("Rectangle(0, 0, 10, 10).intersects(Rectangle(5, 5, 10, 10))")
            .unwrap(),
        Value::Bool(true)
    );
    assert_eq!(
        ctx.eval_expression("Rectangle(0, 0, 4, 4).intersects(Rectangle(5, 5, 10, 10))")
            .unwrap(),
        Value::Bool(false)
    );
    assert_eq!(
        ctx.eval_expression("Rectangle(0, 0, 10, 10).intersection(Rectangle(5, 5, 10, 10)).left")
            .unwrap(),
        Value::Int(5)
    );
    let no_int = ctx
        .eval_expression("Rectangle(0, 0, 2, 2).intersection(Rectangle(9, 9, 2, 2))")
        .unwrap();
    assert!(matches!(no_int, Value::Null));
}

#[test]
fn mutable_rectangle() {
    let mut ctx = Context::new();
    ctx.eval("import 'dart:math'; void main() {}").unwrap();
    ctx.eval("var r = MutableRectangle(1, 2, 3, 4); void main() { r.left = 10; }")
        .unwrap();
    assert_eq!(ctx.eval_expression("r.left").unwrap(), Value::Int(10));
}

#[test]
fn random_secure() {
    let mut ctx = Context::new();
    ctx.eval("import 'dart:math'; void main() {}").unwrap();
    let a = ctx
        .eval_expression("Random.secure().nextInt(1000000)")
        .unwrap();
    let b = ctx
        .eval_expression("Random.secure().nextInt(1000000)")
        .unwrap();
    let Value::Int(ai) = a else {
        panic!("expected int, got {a:?}")
    };
    let Value::Int(bi) = b else {
        panic!("expected int, got {b:?}")
    };
    assert!((0..1000000).contains(&ai), "nextInt out of range: {ai}");
    assert!((0..1000000).contains(&bi), "nextInt out of range: {bi}");
}
