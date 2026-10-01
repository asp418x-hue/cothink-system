use std::cell::RefCell;
use std::collections::HashMap;
use std::io::Read;
use std::rc::Rc;
use std::time::Duration;

use d4rt_rs::Context;
use d4rt_rs::Value;
use d4rt_rs::helpers::{as_bool, as_int, as_str};
use d4rt_rs::permission::{FilesystemPermission, NetworkPermission, Permission, ProcessPermission};

/// A `Context` with every capability granted (the pre-permission behavior).
fn permissive_ctx() -> Context {
    let mut c = Context::new();
    c.grant(Permission::Filesystem(FilesystemPermission::Any));
    c.grant(Permission::Network(NetworkPermission::Any));
    c.grant(Permission::Process(ProcessPermission::Any));
    c.grant(Permission::Isolate);
    c.grant(Permission::Env);
    c
}

fn eval_expr(src: &str) -> Value {
    permissive_ctx().eval_expression(src).unwrap()
}

fn r#async_isolate(c: &mut Context, body: &str, ret: &str) -> Value {
    let src =
        format!("import 'dart:isolate';\nFuture<dynamic> main() async {{ {body} return {ret}; }}");
    c.eval(&src)
        .unwrap_or_else(|e| panic!("eval failed: {:?} (debug: {e:?})", e.message()))
}

fn r#async_io(c: &mut Context, body: &str, ret: &str) -> Value {
    let src = format!("import 'dart:io';\nFuture<dynamic> main() async {{ {body} return {ret}; }}");
    c.eval(&src)
        .unwrap_or_else(|e| panic!("eval failed: {:?} (debug: {e:?})", e.message()))
}

fn num(v: &Value) -> i64 {
    match v {
        Value::Int(i) => *i,
        other => panic!("expected int, got {other:?}"),
    }
}

fn str(v: &Value) -> String {
    match v {
        Value::Str(s) => s.to_string(),
        other => panic!("expected string, got {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// Context isolation
// ---------------------------------------------------------------------------

#[test]
fn contexts_do_not_share_state() {
    let mut a = permissive_ctx();
    let mut b = permissive_ctx();
    a.eval("var secret = 42;").unwrap();
    assert!(b.eval_expression("secret").is_err());
    b.eval("var secret = 'mine';").unwrap();
    assert_eq!(a.eval_expression("secret").unwrap(), Value::Int(42));
}

#[test]
fn same_function_name_in_two_contexts() {
    let mut a = permissive_ctx();
    let mut b = permissive_ctx();
    a.eval("int f() => 1;").unwrap();
    b.eval("int f() => 2;").unwrap();
    assert_eq!(a.eval_expression("f()").unwrap(), Value::Int(1));
    assert_eq!(b.eval_expression("f()").unwrap(), Value::Int(2));
}

#[test]
fn each_context_has_own_core_stdlib() {
    let mut ctx = permissive_ctx();
    assert_eq!(
        ctx.eval_expression(r#""a".toUpperCase()"#).unwrap(),
        Value::str("A")
    );
    assert_eq!(
        ctx.eval_expression("Duration(seconds: 90).inMinutes")
            .unwrap(),
        Value::Int(1)
    );
}

#[test]
fn receive_ports_do_not_leak_between_contexts() {
    let mut a = permissive_ctx();
    let v = r#async_isolate(
        &mut a,
        "final rp = ReceivePort();
         rp.sendPort.send('a');
         final m = await rp.first;
         rp.close();
         m;",
        "m",
    );
    assert_eq!(str(&v), "a");

    let mut b = permissive_ctx();
    let v = r#async_isolate(
        &mut b,
        "final rp = ReceivePort();
         rp.sendPort.send('b');
         final m = await rp.first;
         rp.close();
         m;",
        "m",
    );
    assert_eq!(str(&v), "b");

    // Ports created by `a` must not be reachable from `b` after `a` is gone.
    drop(a);
    let mut c = permissive_ctx();
    let v = r#async_isolate(
        &mut c,
        "final rp = ReceivePort();
         rp.sendPort.send('c');
         final m = await rp.first;
         rp.close();
         m;",
        "m",
    );
    assert_eq!(str(&v), "c");
}

#[test]
fn dropped_context_releases_sockets() {
    let port = {
        let mut a = permissive_ctx();
        let v = r#async_io(
            &mut a,
            "final ss = await ServerSocket.bind(InternetAddress.anyIPv4, 0);
             ss.port;",
            "ss.port",
        );
        num(&v)
    };

    // `a` is dropped: its listener must have been closed, so binding the
    // exact same OS port must succeed now.
    let mut b = permissive_ctx();
    let v = r#async_io(
        &mut b,
        &format!(
            "final ss = await ServerSocket.bind(InternetAddress.anyIPv4, {port});
             ss.port;"
        ),
        "ss.port",
    );
    assert_eq!(num(&v), port);
}

#[test]
fn dropped_context_closes_sockets() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();

    let mut a = permissive_ctx();
    r#async_io(
        &mut a,
        &format!("await Socket.connect('127.0.0.1', {port});"),
        "null",
    );

    let (mut stream, _) = listener.accept().unwrap();
    drop(a);

    // Dropping the context must close the client socket (EOF on our side).
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    let mut buf = [0u8; 1];
    let n = stream.read(&mut buf).unwrap();
    assert_eq!(n, 0, "expected EOF after context drop");
}

// ---------------------------------------------------------------------------
// Native function registration
// ---------------------------------------------------------------------------

#[test]
fn register_function_basic() {
    let mut ctx = permissive_ctx();
    ctx.register_function("double_it", |_v, args, _named| {
        let n = as_int(&args[0]).unwrap_or(0);
        Ok(Value::Int(n * 2))
    });
    assert_eq!(
        ctx.eval_expression("double_it(21)").unwrap(),
        Value::Int(42)
    );
}

#[test]
fn register_function_with_named_args() {
    let mut ctx = permissive_ctx();
    ctx.register_function("greet", |_v, args, named| {
        let name = as_str(&args[0]).unwrap_or("");
        let excited = named.get("loud").and_then(as_bool).unwrap_or(false);
        let s = if excited {
            format!("{name}!!")
        } else {
            name.to_string()
        };
        Ok(Value::str(s))
    });
    assert_eq!(
        ctx.eval_expression(r#"greet("hi", loud: true)"#).unwrap(),
        Value::str("hi!!")
    );
    assert_eq!(
        ctx.eval_expression(r#"greet("yo", loud: false)"#).unwrap(),
        Value::str("yo")
    );
}

#[test]
fn register_function_calls_back_into_dart() {
    let mut ctx = permissive_ctx();
    ctx.eval("int square(int x) => x * x;").unwrap();
    let _square_fn = ctx.get("square").unwrap();
    ctx.register_function("apply_twice", move |v, args, _named| {
        let f = args[0].clone();
        let x = as_int(&args[1]).unwrap_or(0);
        let r1 = v.call_value(&f, vec![Value::Int(x)], HashMap::new())?;
        v.call_value(&f, vec![r1], HashMap::new())
    });
    assert_eq!(
        ctx.eval_expression("apply_twice(square, 3)").unwrap(),
        Value::Int(81)
    );
}

#[test]
fn register_function_can_throw() {
    let mut ctx = permissive_ctx();
    ctx.register_function("must_throw", |_v, _args, _named| {
        Err(d4rt_rs::InterpError::runtime("boom from rust"))
    });
    assert!(ctx.eval_expression("must_throw()").is_err());
}

#[test]
fn registered_function_visible_after_reeval() {
    let mut ctx = permissive_ctx();
    ctx.register_function("ping", |_v, _args, _named| Ok(Value::str("pong")));
    ctx.eval("var seen = ping();").unwrap();
    assert_eq!(ctx.eval_expression("seen").unwrap(), Value::str("pong"));
}

// ---------------------------------------------------------------------------
// get / set
// ---------------------------------------------------------------------------

#[test]
fn get_and_set_top_level() {
    let mut ctx = permissive_ctx();
    ctx.set("base", Value::Int(100));
    assert_eq!(ctx.eval_expression("base + 1").unwrap(), Value::Int(101));
    ctx.eval("var doubled = base * 2;").unwrap();
    assert_eq!(ctx.get("doubled").unwrap(), Value::Int(200));
}

// ---------------------------------------------------------------------------
// print capture
// ---------------------------------------------------------------------------

#[test]
fn print_goes_to_handler() {
    let mut ctx = permissive_ctx();
    let out = Rc::new(RefCell::new(Vec::new()));
    let out2 = out.clone();
    ctx.set_print_handler(move |s| out2.borrow_mut().push(s.to_string()));
    ctx.eval("void main() { print('a'); print(1 + 2); print(true); }")
        .unwrap();
    assert_eq!(*out.borrow(), vec!["a", "3", "true"]);
}

// ---------------------------------------------------------------------------
// Evaluation
// ---------------------------------------------------------------------------

#[test]
fn eval_runs_main() {
    let mut ctx = permissive_ctx();
    let out = Rc::new(RefCell::new(Vec::new()));
    let out2 = out.clone();
    ctx.set_print_handler(move |s| out2.borrow_mut().push(s.to_string()));
    ctx.eval("void main() { print('from main'); }").unwrap();
    assert_eq!(*out.borrow(), vec!["from main"]);
}

#[test]
fn eval_without_main_is_ok() {
    let mut ctx = permissive_ctx();
    ctx.eval("int f() => 7;").unwrap();
    assert_eq!(ctx.eval_expression("f()").unwrap(), Value::Int(7));
}

#[test]
fn eval_error_propagates() {
    let mut ctx = permissive_ctx();
    assert!(ctx.eval("void main() { undefined_thing(); }").is_err());
    assert!(ctx.eval("void main() { 1 / 0; }").is_ok());
}

// ---------------------------------------------------------------------------
// stdlib behavior exercised through the public API
// ---------------------------------------------------------------------------

#[test]
fn stdlib_basics() {
    assert_eq!(eval_expr("'ab' + 'cd'"), Value::str("abcd"));
    assert_eq!(eval_expr("[1, 2, 3].length"), Value::Int(3));
    assert_eq!(
        eval_expr("[1, 2, 3].map((x) => x * 2).toList()[1]"),
        Value::Int(4)
    );
    assert_eq!(eval_expr("{'k': 1}['k']"), Value::Int(1));
    assert_eq!(
        eval_expr("BigInt.parse('100').pow(2) == BigInt.from(10000)"),
        Value::Bool(true)
    );
    assert_eq!(
        eval_expr("RegExp(r'\\d+').hasMatch('a1b')"),
        Value::Bool(true)
    );
    assert_eq!(eval_expr("Duration(hours: 2).inMinutes"), Value::Int(120));
}

#[test]
fn stdlib_core_new_classes() {
    assert_eq!(eval_expr("DateTime(2023, 5, 15).weekday"), Value::Int(1));
    assert_eq!(
        eval_expr("DateTime.parse('2024-02-29').day"),
        Value::Int(29)
    );
    assert_eq!(eval_expr("Uri.parse('https://x/y').host"), Value::str("x"));
    assert_eq!(
        eval_expr("FormatException('m', 's', 2).toString()"),
        Value::str("FormatException: m (at offset 2)")
    );
}

#[test]
fn whole_program() {
    let mut ctx = permissive_ctx();
    let out = Rc::new(RefCell::new(Vec::new()));
    let out2 = out.clone();
    ctx.set_print_handler(move |s| out2.borrow_mut().push(s.to_string()));
    ctx.eval(
        r#"
        int fib(int n) => n < 2 ? n : fib(n - 1) + fib(n - 2);
        void main() {
          for (var i = 0; i < 8; i++) { print(fib(i)); }
        }
        "#,
    )
    .unwrap();
    assert_eq!(*out.borrow(), vec!["0", "1", "1", "2", "3", "5", "8", "13"]);
}

#[test]
fn many_contexts_in_parallel_use() {
    let mut contexts: Vec<Context> = (0..8)
        .map(|i| {
            let mut c = permissive_ctx();
            c.eval(&format!("int seed() => {i};")).unwrap();
            c
        })
        .collect();
    for (i, ctx) in contexts.iter_mut().enumerate() {
        assert_eq!(ctx.eval_expression("seed()").unwrap(), Value::Int(i as i64));
    }
}

// ---------------------------------------------------------------------------
// Process-global host configuration (package libraries, main script path)
// ---------------------------------------------------------------------------

#[test]
fn package_libraries_register_from_any_thread() {
    let mut bc = d4rt_rs::class::BridgedClassDef::new("PkgThing");
    bc.static_getters.insert(
        "answer".to_string(),
        d4rt_rs::stdlib::helpers::static_getter(|_v| Ok(Value::Int(42))),
    );
    let uri = "package:cross_thread_test/lib.dart".to_string();
    std::thread::spawn(move || {
        d4rt_rs::stdlib::register_package_library(&uri, vec![std::sync::Arc::new(bc)]);
    })
    .join()
    .unwrap();

    // The registration happened on another thread; the Context here must
    // still resolve the import.
    let mut ctx = permissive_ctx();
    let v = ctx
        .eval(
            "import 'package:cross_thread_test/lib.dart';
             void main() { return PkgThing.answer; }",
        )
        .unwrap();
    assert_eq!(v, Value::Int(42));
}

#[test]
fn main_script_path_is_process_global() {
    std::thread::spawn(|| {
        d4rt_rs::stdlib::set_main_script_path("/tmp/global_script.dart");
    })
    .join()
    .unwrap();
    assert_eq!(
        d4rt_rs::stdlib::main_script_path().as_deref(),
        Some("/tmp/global_script.dart")
    );
}

#[test]
fn module_library_registration_is_per_context() {
    let make_bc = |name: &str, answer: i64| {
        let mut bc = d4rt_rs::class::BridgedClassDef::new(name);
        bc.static_getters.insert(
            "answer".to_string(),
            d4rt_rs::stdlib::helpers::static_getter(move |_v| Ok(Value::Int(answer))),
        );
        std::sync::Arc::new(bc)
    };

    // The same URI resolves to different classes in different sandboxes.
    let mut ctx_a = permissive_ctx();
    ctx_a.register_module_library("local_mod", vec![make_bc("ThingA", 1)]);
    let mut ctx_b = permissive_ctx();
    ctx_b.register_module_library("local_mod", vec![make_bc("ThingB", 2)]);
    assert_eq!(
        ctx_a
            .eval("import 'local_mod'; int main() => ThingA.answer;")
            .unwrap(),
        Value::Int(1)
    );
    assert_eq!(
        ctx_b
            .eval("import 'local_mod'; int main() => ThingB.answer;")
            .unwrap(),
        Value::Int(2)
    );

    // A third context sees neither registration.
    let mut ctx_c = permissive_ctx();
    let err = ctx_c
        .eval("import 'local_mod'; int main() => ThingA.answer;")
        .unwrap_err();
    assert!(err.to_string().contains("local_mod"), "{err:?}");
}

#[test]
fn global_dart_module_registration_from_any_thread() {
    // The module is registered process-wide; every context resolves it.
    std::thread::spawn(|| {
        d4rt_rs::stdlib::register_dart_module("./global_mylib.dart", "int global_answer() => 7;");
    })
    .join()
    .unwrap();
    let mut ctx = permissive_ctx();
    let v = ctx
        .eval("import './global_mylib.dart'; int main() => global_answer();")
        .unwrap();
    assert_eq!(v, Value::Int(7));
    assert_eq!(
        d4rt_rs::stdlib::dart_module_source("./global_mylib.dart").as_deref(),
        Some("int global_answer() => 7;")
    );
}

#[test]
fn per_context_dart_module_shadows_global_one() {
    // Without a local module, the global registration is used...
    std::thread::spawn(|| {
        d4rt_rs::stdlib::register_dart_module("./shadow_mylib.dart", "int f() => 1;");
    })
    .join()
    .unwrap();
    let mut plain = permissive_ctx();
    assert_eq!(
        plain
            .eval("import './shadow_mylib.dart'; int main() => f();")
            .unwrap(),
        Value::Int(1)
    );

    // ...and a per-Context registration wins for that context only.
    let mut local = permissive_ctx();
    local.register_dart_module("./shadow_mylib.dart", "int f() => 2;");
    assert_eq!(
        local
            .eval("import './shadow_mylib.dart'; int main() => f();")
            .unwrap(),
        Value::Int(2)
    );
    // A fresh context still resolves the global source.
    let mut plain2 = permissive_ctx();
    assert_eq!(
        plain2
            .eval("import './shadow_mylib.dart'; int main() => f();")
            .unwrap(),
        Value::Int(1)
    );
    // The global source is unchanged.
    assert_eq!(
        d4rt_rs::stdlib::dart_module_source("./shadow_mylib.dart").as_deref(),
        Some("int f() => 1;")
    );
}

#[test]
fn context_registration_does_not_pollute_global_registry() {
    let uri = "package:no_leak_test/lib.dart".to_string();
    let mut bc = d4rt_rs::class::BridgedClassDef::new("LeakThing");
    bc.static_getters.insert(
        "answer".to_string(),
        d4rt_rs::stdlib::helpers::static_getter(|_v| Ok(Value::Int(99))),
    );

    // A witness context born before the registration must not see it.
    let witness = permissive_ctx();

    {
        let mut ctx = permissive_ctx();
        ctx.register_package_library(&uri, vec![std::sync::Arc::new(bc)]);
        assert_eq!(
            ctx.eval("import 'package:no_leak_test/lib.dart'; int main() => LeakThing.answer;")
                .unwrap(),
            Value::Int(99)
        );
    }
    // After the registering context is dropped, the registration must be
    // gone: neither a context that predates it nor a fresh one resolves it.
    for fresh in [witness, permissive_ctx()] {
        let mut fresh = fresh;
        let err = fresh
            .eval("import 'package:no_leak_test/lib.dart'; int main() => LeakThing.answer;")
            .unwrap_err();
        assert!(
            err.to_string().contains(&uri),
            "expected unresolved import for {uri}, got: {err:?}"
        );
    }
}

#[test]
fn per_context_registration_shadows_global_one() {
    let uri = "package:shadow_test/lib.dart".to_string();
    let mut bc = d4rt_rs::class::BridgedClassDef::new("GlobalThing");
    bc.static_getters.insert(
        "answer".to_string(),
        d4rt_rs::stdlib::helpers::static_getter(|_v| Ok(Value::Int(0))),
    );
    d4rt_rs::stdlib::register_package_library(&uri, vec![std::sync::Arc::new(bc)]);

    // Without a local registration the global one is used...
    let mut plain = permissive_ctx();
    assert_eq!(
        plain
            .eval("import 'package:shadow_test/lib.dart'; int main() => GlobalThing.answer;")
            .unwrap(),
        Value::Int(0)
    );

    // ...and a Context-local registration shadows it for that context only.
    let mut local = permissive_ctx();
    let mut local_bc = d4rt_rs::class::BridgedClassDef::new("GlobalThing");
    local_bc.static_getters.insert(
        "answer".to_string(),
        d4rt_rs::stdlib::helpers::static_getter(|_v| Ok(Value::Int(7))),
    );
    local.register_package_library(&uri, vec![std::sync::Arc::new(local_bc)]);
    assert_eq!(
        local
            .eval("import 'package:shadow_test/lib.dart'; int main() => GlobalThing.answer;")
            .unwrap(),
        Value::Int(7)
    );
    // The global registry is untouched for other contexts.
    assert_eq!(
        plain
            .eval("import 'package:shadow_test/lib.dart'; int main() => GlobalThing.answer;")
            .unwrap(),
        Value::Int(0)
    );
}

// ---------------------------------------------------------------------------
// Sending a Context to one other thread
// ---------------------------------------------------------------------------

#[test]
fn context_can_move_to_another_thread() {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut ctx = d4rt_rs::SendableContext::new(permissive_ctx()).into_inner();
        ctx.eval("int f() => 6 * 7;").unwrap();
        let _ = tx.send(format!("{:?}", ctx.eval_expression("f()").unwrap()));
    });
    assert_eq!(rx.recv().unwrap(), "42");
}

#[test]
fn moved_context_keeps_its_own_state() {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut ctx = d4rt_rs::SendableContext::new(permissive_ctx()).into_inner();
        ctx.eval("var secret = 'on-thread-b';").unwrap();
        let _ = tx.send(format!("{:?}", ctx.eval_expression("secret").unwrap()));
    });
    // The main thread's contexts must not see the other thread's state.
    assert!(permissive_ctx().eval_expression("secret").is_err());
    assert_eq!(rx.recv().unwrap(), "on-thread-b");
}

#[test]
fn moved_context_uses_socket_resources() {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut ctx = d4rt_rs::SendableContext::new(permissive_ctx()).into_inner();
        let v = r#async_io(
            &mut ctx,
            "final ss = await ServerSocket.bind(InternetAddress.anyIPv4, 0);
             ss.port;",
            "ss.port",
        );
        let _ = tx.send(num(&v));
    });
    let port = rx.recv().unwrap();
    assert!(port > 0);

    // The bound port must be released once the context (dropped at thread
    // exit) is cleaned up; allow a moment for the listener thread to exit.
    std::thread::sleep(Duration::from_millis(200));
    let mut c = permissive_ctx();
    let v = r#async_io(
        &mut c,
        &format!("final ss = await ServerSocket.bind(InternetAddress.anyIPv4, {port}); ss.port;"),
        "ss.port",
    );
    assert_eq!(num(&v), port);
}
