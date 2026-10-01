//! A Dart interpreter and runtime environment in Rust, embeddable as a
//! library. The public API follows the QuickJS-for-Rust pattern
//! ([`rquickjs`]): an independent [`Context`] runtime with typed value
//! conversion ([`FromValue`]/[`ToValue`]), native function registration,
//! and bridged native classes — while execution semantics, permissions,
//! and module loading are modeled after the [`d4rt`] Dart interpreter.
//!
//! [`rquickjs`]: https://docs.rs/rquickjs
//! [`d4rt`]: https://github.com/andres-arana/d4rt
//!
//! # Quick start
//!
//! ```
//! use d4rt_rs::Context;
//!
//! let mut ctx = Context::new();
//! ctx.eval("int fib(int n) => n < 2 ? n : fib(n - 1) + fib(n - 2);").unwrap();
//! assert_eq!(ctx.eval_expression("fib(10)").unwrap(), d4rt_rs::Value::Int(55));
//! ```
//!
//! # Feature flags
//!
//! The interpreter core and `dart:core` are always compiled. The rest of
//! the standard library is split into optional bundles, so embedded
//! runtimes can ship only what they use:
//!
//! | Feature | Dart libraries gated |
//! |---------|----------------------|
//! | `stdlib` (default) | All of the below |
//! | `async` | `dart:async` — Future/Stream/Timer/Completer |
//! | `math` | `dart:math` — Random, Point, Rectangle, Trigonometry... |
//! | `convert` | `dart:convert` — utf8/base64/json/ascii encoders |
//! | `collection` | `dart:collection` — LinkedHashMap/LinkedHashSet/Queue |
//! | `typed_data` | `dart:typed_data` — ByteData/Endian/Int32List... |
//! | `io` | `dart:io` — File, Socket, HttpServer/Client, Process... |
//! | `isolate` | `dart:isolate` — ReceivePort/Isolate.run/spawn |
//!
//! `async`/`math`/`convert`/`collection`/`typed_data` each depend on
//! `core`; `io` pulls in `core`, `async` and `convert`; `isolate` pulls in
//! `core` and `async`. The stream machinery lives in the interpreter core
//! itself — the `async` feature only gates the library surface visible to
//! Dart code.
//!
//! With `default-features = false` the runtime is a bare interpreter with
//! `dart:core` only:
//!
//! ```toml
//! [dependencies]
//! d4rt_rs = { version = "0.1", default-features = false, features = ["math", "convert"] }
//! ```
//!
//! # The [`Context`]
//!
//! Every [`Context`] is an independent runtime: its own global environment
//! with a freshly loaded `dart:core`, its own permissions, and its own
//! module registries. State never leaks between contexts; move a context
//! to another thread with [`SendableContext`].
//!
//! ```
//! use d4rt_rs::Context;
//!
//! let mut repl = Context::new();
//! repl.eval("var counter = 0;").unwrap();
//! repl.eval("void main() { counter++; }").unwrap();
//! assert_eq!(repl.eval_expression("counter").unwrap(), d4rt_rs::Value::Int(1));
//! ```
//!
//! Async top-level `main` functions are awaited to completion by
//! [`Context::eval`] and [`Context::execute`]; use
//! [`Context::execute_async`] for fine-grained control, or
//! [`Context::pump_to_completion`] to keep stepping an already-started
//! future.
//!
//! # Native functions
//!
//! [`Context::register_function`] installs a native callback callable from
//! Dart. Arguments arrive as [`Value`]s and are decoded with
//! [`FromValue`]; results are produced with [`ToValue`]:
//!
//! ```
//! use d4rt_rs::{Context, FromValue, ToValue, Value};
//!
//! let mut ctx = Context::new();
//! ctx.register_function("greet", |_v, args, named| {
//!     let name = String::from_value(&args[0])?;
//!     let loud = bool::from_value(named.get("loud").unwrap_or(&Value::Bool(false)))?;
//!     Ok(if loud { format!("{name}!!") } else { name }.to_value())
//! });
//! assert_eq!(ctx.eval_expression(r#"greet("hi", loud: true)"#).unwrap(), Value::str("hi!!"));
//! ```
//!
//! Native functions receive the interpreter visitor as their first
//! argument and can call back into Dart functions with
//! [`crate::interp::InterpreterVisitor::call_value`]. Bridged native
//! *classes* (Rust types visible from Dart, e.g. `BigInt`, `DateTime`,
//! `RegExp`) are defined with [`crate::class::BridgedClassDef`] and the
//! helpers in [`crate::stdlib::helpers`], then registered with
//! [`Context::register_bridged_class`].
//!
//! # Sandboxing
//!
//! By default a [`Context`] denies every dangerous operation —
//! filesystem, network, process, isolate, and environment access. Grant
//! capabilities up front with [`Context::grant`] (or [`SandboxOptions`]),
//! check them with [`Context::has_permission`], and withdraw them with
//! [`Context::revoke`]. Denied operations raise a Dart-catchable runtime
//! error; see the [`permission`] module for the matching rules.
//!
//! ```
//! use d4rt_rs::{Context, SandboxOptions};
//! use d4rt_rs::permission::{FilesystemPermission, Permission};
//!
//! let mut ctx = Context::new_with(SandboxOptions {
//!     filesystem: Some(FilesystemPermission::ReadPath("/tmp".into())),
//!     ..Default::default()
//! });
//! assert!(ctx.has_permission(&Permission::Filesystem(
//!     FilesystemPermission::ReadPath("/tmp/x".into()),
//! )));
//! ```
//!
//! # Modules
//!
//! Three registration surfaces exist, resolved in this order at import
//! time:
//!
//! 1. Per-`Context` module libraries — [`Context::register_module_library`]
//!    (bridged classes under a URI, shadowing the global registry).
//! 2. Process-global module libraries — [`crate::stdlib::register_package_library`].
//! 3. Virtual `dart:` modules — [`Context::register_dart_module`] (a
//!    per-`Context` `dart:` source) falling back to
//!    [`crate::stdlib::register_dart_module`] (process-global).
//!
//! With filesystem permissions granted, `import`/`export` also load real
//! `.dart` files.
//!
//! # Concurrency
//!
//! A [`Context`] uses `Rc`/`RefCell` internally and is neither `Send` nor
//! `Sync`. [`SendableContext`] is an `unsafe`-free wrapper that soundly
//! permits moving a context **once** to one other thread, where it must
//! stay; wrap it with [`SendableContext::new`] on the sender thread and
//! unwrap with [`SendableContext::into_inner`] on the receiver.

pub mod ast;
pub mod async_state;
pub mod callable;
pub mod class;
pub mod context;
pub mod convert;
pub mod env;
pub mod error;
pub mod interp;
pub mod log;
pub mod permission;
pub mod rt;
pub mod state;
pub mod stdlib;
pub mod value;

pub use context::{Context, NativeFn, SandboxOptions, SendableContext};
pub use convert::{FromValue, ToValue};
pub use error::InterpError;
pub use value::Value;

pub use crate::stdlib::helpers;
