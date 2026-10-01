//! Demonstrates embedding `d4rt_rs` in your own Rust project, inspired by
//! the APIs of `~/d4rt` (Dart) and `rquickjs` (Rust):
//!
//! - independent `Context` runtimes with full `dart:core` support
//! - registering native Rust functions callable from Dart
//! - typed value conversion via `FromValue` / `ToValue`
//! - calling Dart functions from Rust with arguments
//! - capturing `print` output instead of stdout
//! - multiple isolated contexts in parallel

use std::collections::HashMap;

use d4rt_rs::FromValue;
use d4rt_rs::ToValue;
use d4rt_rs::{Context, Value};

fn main() {
    // 1. A context behaves like a QuickJS `Context` or a `D4rt` instance.
    let mut ctx = Context::new();

    // 2. Register native functions; use FromValue/ToValue for typed args.
    ctx.register_function("add", |_v, args, _named| {
        let a =
            i64::from_value(&args[0]).map_err(|e| d4rt_rs::InterpError::runtime(e.to_string()))?;
        let b =
            i64::from_value(&args[1]).map_err(|e| d4rt_rs::InterpError::runtime(e.to_string()))?;
        Ok((a + b).to_value())
    });
    ctx.register_function("now_ts", |_v, _args, _named| {
        let secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        Ok(secs.to_value())
    });

    // 3. Capture print output instead of stdout.
    let logs: Vec<String> = Vec::new();
    let sink = std::rc::Rc::new(std::cell::RefCell::new(logs));
    let sink2 = sink.clone();
    ctx.set_print_handler(move |s| sink2.borrow_mut().push(s.to_string()));

    // 4. Run a program that uses the native functions.
    ctx.eval(
        r#"
        void main() {
          print('add(20, 22) = ${add(20, 22)}');
          print('now_ts() = ${now_ts()}');
          var fibs = [for (var i = 0; i < 10; i++) i < 2 ? i : (i - 1) * 0 + i];
        }
        "#,
    )
    .unwrap();
    println!("captured output: {:#?}", sink.borrow());

    // 5. Call a Dart function from Rust with arguments (d4rt.execute style).
    let result = ctx
        .execute(
            "int mul(int a, int b) => a * b;",
            "mul",
            vec![Value::Int(6), Value::Int(7)],
            HashMap::new(),
        )
        .unwrap();
    println!("execute('mul', [6, 7]) = {result:?}");

    // 6. Contexts are fully isolated: same code, different state.
    let mut a = Context::new();
    let mut b = Context::new();
    a.eval("var secret = 'context-a';").unwrap();
    b.eval("var secret = 'context-b';").unwrap();
    println!("a.secret = {:?}", a.eval_expression("secret").unwrap());
    println!("b.secret = {:?}", b.eval_expression("secret").unwrap());

    // 7. REPL-style: state persists across eval calls in one context.
    let mut repl = Context::new();
    repl.eval("var counter = 0;").unwrap();
    for _ in 0..3 {
        repl.eval("void main() { counter++; }").unwrap();
    }
    println!(
        "counter after 3 evals = {:?}",
        repl.eval_expression("counter").unwrap()
    );
}
