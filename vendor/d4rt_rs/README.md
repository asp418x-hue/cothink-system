# d4rt_rs

[![Crates.io](https://img.shields.io/crates/v/d4rt_rs)](https://crates.io/crates/d4rt_rs)
[![docs.rs](https://img.shields.io/docsrs/d4rt_rs)](https://docs.rs/d4rt_rs)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](#license)
[![Ask DeepWiki](https://deepwiki.com/badge.svg)](https://deepwiki.com/tachibana-shin/d4rt_rs)

A **Dart interpreter and runtime environment written in Rust**, embeddable
as a library. The public API takes inspiration from two projects:

- **[`~/d4rt`](https://github.com/andres-arana/d4rt)** — a Dart interpreter
  with bridged native types, module loading, permissions, and REPL-style
  evaluation.
- **[rquickjs](https://github.com/DelSkayn/rquickjs)** — the ergonomic
  QuickJS bindings for Rust, with `Context`, function registration, and
  typed value conversion (`FromValue`/`ToValue`).

`d4rt_rs` combines the two: a QuickJS-style `Context` runtime with
d4rt-style execution, bridged classes, per-`Context` sandboxing, and
optional `dart:*` libraries behind Cargo feature flags.

The standard library bundles math, async/streams, JSON/UTF-8 conversion,
collections, typed data, isolates, process, and `dart:io` (files, sockets,
HTTP server/client) — so it is well suited to **embedding a scripting
engine, running untrusted Dart models at the edge, interactive REPLs, or
testing tooling**.

## Usage

Add to your `Cargo.toml`:

```toml
[dependencies]
d4rt_rs = "0.1"
```

The crate compiles with the full standard library by default. See
[Feature flags](#feature-flags) to slim it down.

### Running Dart code

```rust
use d4rt_rs::Context;

fn main() {
    let mut ctx = Context::new();
    ctx.eval("int fib(int n) => n < 2 ? n : fib(n - 1) + fib(n - 2);").unwrap();
    assert_eq!(ctx.eval_expression("fib(10)").unwrap(), d4rt_rs::Value::Int(55));
}
```

Async top-level `main` functions are awaited to completion; top-level
state persists across `eval` calls (REPL-style).

### Registering native functions

```rust
use d4rt_rs::{Context, FromValue, ToValue, Value};

let mut ctx = Context::new();

ctx.register_function("add", |_v, args, _named| {
    let a = i64::from_value(&args[0])?;
    let b = i64::from_value(&args[1])?;
    Ok((a + b).to_value())
});

ctx.register_function("greet", |_v, args, named| {
    let name = String::from_value(&args[0])?;
    let loud = bool::from_value(named.get("loud").unwrap_or(&Value::Bool(false)))?;
    Ok(if loud { format!("{name}!!") } else { name }.to_value())
});

assert_eq!(ctx.eval_expression("add(20, 22)").unwrap(), Value::Int(42));
assert_eq!(ctx.eval_expression(r#"greet("hi", loud: true)"#).unwrap(), Value::str("hi!!"));
```

Native functions can call back into Dart code through the visitor:

```rust
ctx.register_function("apply_twice", |v, args, _named| {
    let f = args[0].clone();
    let x = i64::from_value(&args[1])?;
    let r1 = v.call_value(&f, vec![Value::Int(x)], std::collections::HashMap::new())?;
    v.call_value(&f, vec![r1], std::collections::HashMap::new())
});
```

### Calling Dart functions from Rust (`execute`)

```rust
let mut ctx = Context::new();
let result = ctx
    .execute(
        "int mul(int a, int b) => a * b;",
        "mul",
        vec![Value::Int(6), Value::Int(7)],
        std::collections::HashMap::new(),
    )
    .unwrap();
assert_eq!(result, Value::Int(42));
```

### Isolated runtimes

Each `Context` is fully independent — its own globals and its own
`dart:core`:

```rust
let mut a = Context::new();
let mut b = Context::new();
a.eval("var secret = 'a';").unwrap();
assert!(b.eval_expression("secret").is_err());
```

### Capturing `print` output

```rust
let mut ctx = Context::new();
ctx.set_print_handler(|s| my_log(s));
ctx.eval("void main() { print('hello'); }").unwrap();
```

### Sandboxing

By default a `Context` denies every dangerous operation (filesystem,
network, process, isolate, environment). Grant capabilities up front:

```rust
use d4rt_rs::{Context, SandboxOptions};
use d4rt_rs::permission::{FilesystemPermission, ProcessPermission, Permission};

let mut ctx = Context::new_with(SandboxOptions {
    filesystem: Some(FilesystemPermission::ReadPath("/etc".into())),
    process: Some(ProcessPermission::Any),
    ..Default::default()
});

assert!(ctx.has_permission(&Permission::Filesystem(
    FilesystemPermission::ReadPath("/etc/motd".into()),
)));
assert!(!ctx.has_permission(&Permission::Network(
    d4rt_rs::permission::NetworkPermission::Any,
)));
```

Permissions can be checked and withdrawn at runtime with
`Context::has_permission` / `Context::revoke`; denied operations raise a
Dart-catchable runtime error.

### Bridging native types

Bridged classes (native Rust types visible from Dart) are built with
`BridgedClassDef` and the helpers in `d4rt_rs::stdlib::helpers`, then
registered with `Context::register_bridged_class`. The stdlib itself uses
this for `BigInt`, `DateTime`, `Duration`, `RegExp`, `Uri`, sockets, and
more (see `src/stdlib/` for worked examples).

### Modules

`import`/`export` resolve in this order: per-`Context` module libraries
(`Context::register_module_library`), process-global libraries
(`d4rt_rs::stdlib::register_package_library`), virtual `dart:` modules
(`Context::register_dart_module`, with a process-global fallback in
`d4rt_rs::stdlib::register_dart_module`), then real `.dart` files on disk
(when filesystem permissions allow).

### Command-line interpreter

```sh
cargo install d4rt_rs
d4rt_rs tests/smoke.dart
```

The `main.rs` binary is a small, permissively-configured frontend to the
same `Context` API (see below).

## Features

The interpreter core and `dart:core` are always compiled. The rest of the
standard library ships behind optional feature flags:

| Feature | Dart libraries gated |
|---------|----------------------|
| `stdlib` (default) | All of the below |
| `async` | `dart:async` — Future/Stream/Timer/Completer |
| `math` | `dart:math` — Random, Point, Rectangle, trig... |
| `convert` | `dart:convert` — utf8/base64/json/ascii |
| `collection` | `dart:collection` — LinkedHashMap/LinkedHashSet/Queue |
| `typed_data` | `dart:typed_data` — ByteData/Endian/*List buffers |
| `io` | `dart:io` — File, Socket, HttpServer/Client, Process... |
| `isolate` | `dart:isolate` — ReceivePort/Isolate.run/spawn |

Feature dependencies: `async`, `math`, `convert`, `collection`,
`typed_data` need `core`; `io` needs `core` + `async` + `convert`;
`isolate` needs `core` + `async`.

```toml
[dependencies]
d4rt_rs = { version = "0.1", default-features = false, features = ["math", "convert"] }
```

## What is supported

The interpreter implements a large subset of Dart 3:

- Functions (positional/named/optional parameters, defaults, closures),
  classes, inheritance, `mixin`s, enums, records, extensions.
- Collections, sync generators (`sync*`), iterables, `??`/`?.`, pattern
  matching.
- `dart:core` (int, double, String, List, Map, Set, BigInt, DateTime,
  Duration, RegExp, Uri, FormatException, ...).
- Async/await, streams, futures, timers, completers (`dart:async`).
- Permissions, module loading, `dart:isolate`, and `dart:io`.

Top-level `Function RuntimeType`, records, and some reflection corners
are still incomplete; see `tests/` for the precise conformance surface.

## Development

```sh
cargo build
cargo run --example embed
cargo test                 # full suite, default features
cargo test --no-default-features
cargo clippy --all-features
cargo doc --all-features --no-deps
cargo run -- tests/smoke.dart
```

Dart conformance is exercised by both hand-written tests
(`tests/stdlib_*.rs`) and interpreter-driven reference suites
(`tests/stdlib_dart_*_ref_test.rs`). `cargo clippy` lints the library
and binaries only; `cargo clippy --all-targets` additionally sweeps the
per-file test crates.

## License

MIT
