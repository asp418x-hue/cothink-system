//! Library-mode error safety: Dart code that errors must never crash the
//! host process. Every failure surfaces as an `Err(InterpError)` from the
//! `Context` API — runaway recursion is aborted with a runtime error
//! (instead of overflowing the host stack), uncaught exceptions become
//! readable messages, and the context stays usable afterwards.

use d4rt_rs::Context;

#[test]
fn infinite_recursion_returns_error_instead_of_crashing() {
    let mut c = Context::new();
    let err = c
        .eval("int f() => f();\nString main() => f();")
        .unwrap_err()
        .to_string();
    assert!(err.contains("Stack overflow"), "got: {err}");
}

#[test]
fn infinite_recursion_is_catchable_from_dart() {
    let mut c = Context::new();
    let src = "int f() => f();
               String main() {
                 try { f(); return 'no-throw'; } catch (e) { return 'caught'; }
               }";
    let v = c.eval(src).unwrap();
    assert_eq!(v.as_str(), Some("caught"));
}

#[test]
fn deep_but_finite_recursion_still_works() {
    let mut c = Context::new();
    let v = c
        .eval("int down(int n) => n == 0 ? 0 : 1 + down(n - 1);\nString main() => '${down(100)}';")
        .unwrap();
    assert_eq!(v.as_str(), Some("100"));
}

#[test]
fn uncaught_throw_surfaces_as_throw_with_value_intact() {
    let mut c = Context::new();
    let err = c
        .eval("String main() { throw Exception('boom'); }")
        .unwrap_err();
    assert!(
        matches!(err, d4rt_rs::InterpError::Throw(_)),
        "got: {err:?}"
    );
}

#[test]
fn uncaught_async_error_surfaces_as_throw() {
    let mut c = Context::new();
    let err = c
        .eval("Future<String> main() async { throw Exception('boom'); }")
        .unwrap_err();
    assert!(
        matches!(err, d4rt_rs::InterpError::Throw(_)),
        "got: {err:?}"
    );
}

#[test]
fn context_stays_usable_after_dart_error() {
    let mut c = Context::new();
    assert!(c.eval("int f() => f();\nString main() => f();").is_err());
    assert!(
        c.eval("String main() { throw Exception('boom'); }")
            .is_err()
    );
    let v = c.eval("String main() => 'still alive';").unwrap();
    assert_eq!(v.as_str(), Some("still alive"));
}

#[test]
fn recursion_guard_applies_to_async() {
    let mut c = Context::new();
    let err = c
        .eval("Future<String> main() async { return f(); }\nint f() => f();")
        .unwrap_err();
    assert!(
        matches!(err, d4rt_rs::InterpError::Throw(_)),
        "got: {err:?}"
    );
}
