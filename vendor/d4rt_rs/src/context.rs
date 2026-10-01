use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

use crate::ast::CompilationUnit;
use crate::callable::NativeFunction;
use crate::class::{BridgedClassDef, BridgedEnumDef};
use crate::env::Environment;
use crate::error::InterpError;
use crate::interp::InterpreterVisitor;
use crate::permission::{FilesystemPermission, NetworkPermission, Permission, ProcessPermission};
use crate::rt::FunctionRuntimeType;
use crate::value::{Value, value_display};

type PrintHandler = Box<dyn Fn(&str)>;

/// Signature of a native function registered through
/// [`Context::register_function`].
///
/// The closure receives the interpreter visitor (so it can call back into
/// Dart functions), the positional arguments and the named arguments.
pub type NativeFn = Box<
    dyn Fn(
        &mut InterpreterVisitor,
        Vec<Value>,
        HashMap<String, Value>,
    ) -> Result<Value, InterpError>,
>;

/// An independent Dart runtime, roughly analogous to a QuickJS `Context`
/// (or a `D4rt` instance in `~/d4rt`). Each `Context` owns its own global
/// environment with a freshly loaded `dart:core`, so state never leaks
/// between contexts.
///
/// ```
/// let mut ctx = d4rt_rs::Context::new();
/// ctx.eval("int add(int a, int b) => a + b;").unwrap();
/// assert_eq!(ctx.eval_expression("add(2, 3)").unwrap(), d4rt_rs::Value::Int(5));
/// ```
pub struct Context {
    env: Rc<RefCell<Environment>>,
    print_handler: Rc<RefCell<PrintHandler>>,
    state: Rc<crate::state::ContextState>,
    interpreted_instance: Option<Value>,
}

/// One-shot configuration for a sandboxed [`Context`], modeled after
/// Deno's `Deno.sandbox({ permissions })` API.
///
/// `Default` grants nothing (equivalent to a plain [`Context::new`]);
/// use `..Default::default()` to configure only some capabilities:
///
/// ```
/// use d4rt_rs::SandboxOptions;
/// use d4rt_rs::permission::FilesystemPermission;
/// let opts = SandboxOptions {
///     filesystem: Some(FilesystemPermission::ReadPath("/data".into())),
///     ..Default::default()
/// };
/// ```
#[derive(Default, Clone, Debug)]
pub struct SandboxOptions {
    /// Filesystem capability; `None` denies all filesystem access.
    pub filesystem: Option<FilesystemPermission>,
    /// Network capability; `None` denies all network access.
    pub network: Option<NetworkPermission>,
    /// Process capability; `None` denies running any command.
    pub process: Option<ProcessPermission>,
    /// Isolate creation and port communication.
    pub isolate: bool,
    /// `Platform.environment` reads.
    pub env: bool,
    /// Whether `import`/`export` may load modules from the filesystem
    /// (grants `Permission::ModuleFiles`). Off by default: imports resolve
    /// only from `dart:`/`package:` URIs and the virtual module registry
    /// ([`Context::register_dart_module`]).
    pub filesystem_modules: bool,
}

impl SandboxOptions {
    /// Grants every capability (the pre-permission behavior).
    pub fn allow_all() -> Self {
        SandboxOptions {
            filesystem: Some(FilesystemPermission::Any),
            network: Some(NetworkPermission::Any),
            process: Some(ProcessPermission::Any),
            isolate: true,
            env: true,
            filesystem_modules: true,
        }
    }
}

/// Converts a panic that escapes a `Context` entry point into an error:
/// in library mode a Dart error (or an interpreter bug it triggers) must
/// never crash the host process. The panic message is preserved. An
/// uncaught Dart exception still surfaces as `Err(InterpError::Throw(_))`
/// with the thrown value intact.
pub(crate) fn catch_dart_panic<T>(
    f: impl FnOnce() -> Result<T, InterpError>,
) -> Result<T, InterpError> {
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)) {
        Ok(result) => result,
        Err(payload) => {
            let msg = payload
                .downcast_ref::<String>()
                .map(String::as_str)
                .or_else(|| payload.downcast_ref::<&str>().copied())
                .unwrap_or("unknown internal error");
            Err(InterpError::runtime(format!(
                "Internal interpreter error (panic caught at the Context boundary): {msg}"
            )))
        }
    }
}

impl Context {
    /// Creates a context pre-configured with the given sandbox options,
    /// mirroring `Deno.sandbox({ permissions })`. Equivalent to
    /// [`Context::new`] followed by the matching `grant` calls.
    ///
    /// ```
    /// use d4rt_rs::{Context, SandboxOptions};
    /// use d4rt_rs::permission::{FilesystemPermission, Permission};
    /// let mut ctx = Context::new_with(SandboxOptions {
    ///     filesystem: Some(FilesystemPermission::ReadPath("/tmp".into())),
    ///     ..Default::default()
    /// });
    /// assert!(ctx.has_permission(&Permission::Filesystem(
    ///     FilesystemPermission::ReadPath("/tmp/x".into()),
    /// )));
    /// assert!(!ctx.has_permission(&Permission::Filesystem(
    ///     FilesystemPermission::WritePath("/tmp/x".into()),
    /// )));
    /// ```
    pub fn new_with(options: SandboxOptions) -> Self {
        let mut ctx = Context::new();
        if let Some(fs) = options.filesystem {
            ctx.grant(Permission::Filesystem(fs));
        }
        if let Some(net) = options.network {
            ctx.grant(Permission::Network(net));
        }
        if let Some(proc) = options.process {
            ctx.grant(Permission::Process(proc));
        }
        if options.isolate {
            ctx.grant(Permission::Isolate);
        }
        if options.env {
            ctx.grant(Permission::Env);
        }
        if options.filesystem_modules {
            ctx.grant(Permission::ModuleFiles);
        }
        ctx
    }

    /// Creates a fresh runtime with `dart:core` loaded and `print` defined.
    pub fn new() -> Self {
        let state = crate::state::ContextState::new();
        let (env, print_handler) = crate::state::with_current(&state, || {
            let env = Environment::new();
            crate::stdlib::core::load_core(&env);
            #[cfg(feature = "async")]
            if let Some(async_env) = crate::stdlib::load_library("dart:async") {
                Environment::import_environment(&env, &async_env, None, None).expect("async load");
            }
            let print_handler: Rc<RefCell<PrintHandler>> =
                Rc::new(RefCell::new(Box::new(|s: &str| println!("{s}"))));
            let handler = print_handler.clone();
            let print_fn = NativeFunction {
                name: Some("print".to_string()),
                min_arity: 1,
                max_arity: Some(1),
                is_async: false,
                is_generator: false,
                closure: Box::new(move |visitor, positional, _named| {
                    let s = positional
                        .first()
                        .map(|v| {
                            visitor
                                .display_value(v)
                                .unwrap_or_else(|_| value_display(v))
                        })
                        .unwrap_or_default();
                    (handler.borrow())(&s);
                    Ok(Value::Null)
                }),
                callable_runtime_type: FunctionRuntimeType::untyped(),
            };
            Environment::define(&env, "print", Value::NativeFn(Rc::new(print_fn)));
            (env, print_handler)
        });
        Context {
            env,
            print_handler,
            state,
            interpreted_instance: None,
        }
    }

    /// Parses and executes `source` as a Dart program: top-level declarations
    /// are registered, then `main()` is invoked if defined.
    pub fn eval(&mut self, source: &str) -> Result<Value, InterpError> {
        match self.execute(source, "main", vec![], HashMap::new()) {
            Ok(v) => self.pump_to_completion(v),
            Err(e) if e.message().is_some_and(|m| m.contains("not defined")) => Ok(Value::Null),
            Err(e) => Err(e),
        }
    }

    /// Runs the async task queue until `v` (if it is a future) completes.
    /// Non-future values are returned unchanged.
    pub fn pump_to_completion(&mut self, v: Value) -> Result<Value, InterpError> {
        let mut guard: u64 = 0;
        loop {
            match self.drive_bounded(&v, u64::MAX, &mut guard) {
                Ok(DriveVerdict::Done(res)) => return Ok(res),
                Ok(DriveVerdict::Wait) => {}
                Err(e) => return Err(e),
            }
        }
    }

    /// Cooperative counterpart of [`Context::execute`]: calls the function
    /// (synchronously) then drives the async task queue in small bounded
    /// batches, yielding control back to the async runtime between batches so
    /// the calling thread/event loop is not monopolized.
    ///
    /// Works with any executor ([`tokio`](https://tokio.rs), the `futures`
    /// crate's executors, …); just `.await` it from an existing async task.
    /// Yielding is built in, so no extra dependency is required. Non-async
    /// functions finish on the first `poll` without ever yielding.
    ///
    /// ```
    /// async fn run() {
    ///     let mut ctx = d4rt_rs::Context::new();
    ///     let src = "Future<int> compute(int n) async { await Future.delayed(Duration(milliseconds: 5)); return n * 2; }";
    ///     let v = ctx.execute_async_await(src, "compute", vec![d4rt_rs::Value::Int(21)], Default::default()).await.unwrap();
    ///     assert_eq!(v.as_int(), Some(42));
    /// }
    /// ```
    pub async fn execute_async_await(
        &mut self,
        source: &str,
        name: &str,
        positional: Vec<Value>,
        named: HashMap<String, Value>,
    ) -> Result<Value, InterpError> {
        let v = self.execute(source, name, positional, named)?;
        let mut guard: u64 = 0;
        loop {
            match self.drive_bounded(&v, MAX_STEPS_PER_BATCH, &mut guard) {
                Ok(DriveVerdict::Done(res)) => return Ok(res),
                Ok(DriveVerdict::Wait) => yield_now().await,
                Err(e) => return Err(e),
            }
        }
    }

    /// d4rt-style execution: parses `source`, registers top-level
    /// declarations, then calls the function named `name` with the given
    /// positional and named arguments.
    ///
    /// ```
    /// let mut ctx = d4rt_rs::Context::new();
    /// let v = ctx.execute(
    ///     "int add(int a, int b) => a + b;",
    ///     "add",
    ///     vec![d4rt_rs::Value::Int(2), d4rt_rs::Value::Int(3)],
    ///     std::collections::HashMap::new(),
    /// ).unwrap();
    /// assert_eq!(v, d4rt_rs::Value::Int(5));
    /// ```
    pub fn execute(
        &mut self,
        source: &str,
        name: &str,
        positional: Vec<Value>,
        named: HashMap<String, Value>,
    ) -> Result<Value, InterpError> {
        catch_dart_panic(|| {
            crate::state::with_current(&self.state, || {
                let unit = Rc::new(CompilationUnit::parse(source)?);
                let mut visitor = InterpreterVisitor::new(self.env.clone());
                visitor.execute_top_level(&unit)?;
                let result = match Environment::get(&self.env, name) {
                    Ok(Value::Function(f)) => {
                        visitor.call_interpreted_function(&f, positional, named)
                    }
                    Ok(Value::NativeFn(n)) => visitor.call_native_function(&n, positional, named),
                    Ok(other) => Err(InterpError::runtime(format!(
                        "{name} is not a function: {}",
                        value_display(&other)
                    ))),
                    Err(_) => Err(InterpError::runtime(format!(
                        "Function '{name}' is not defined"
                    ))),
                };
                if let Ok(v) = &result
                    && matches!(v, Value::Instance(_) | Value::Bridged(_))
                {
                    self.interpreted_instance = Some(v.clone());
                }
                result
            })
        })
    }

    /// d4rt-style instance invocation: dispatches `name` (an instance
    /// method, getter, static member, or field) on the object returned by
    /// the most recent [`Context::execute`] call, mirroring
    /// `D4rt.invoke` in `~/d4rt`. Used by extension hosts to call provider
    /// methods such as `getPopular` after `main(...)` produced the provider
    /// instance.
    ///
    /// ```
    /// let mut ctx = d4rt_rs::Context::new();
    /// let src = r#"
    /// class Provider {
    ///   String ping() => "pong";
    ///   int get magic => 42;
    /// }
    /// Provider main(String label) => Provider();
    /// "#;
    /// ctx.execute(src, "main", vec![d4rt_rs::Value::str("x")], Default::default()).unwrap();
    /// assert_eq!(ctx.invoke("ping", vec![], Default::default()).unwrap(), d4rt_rs::Value::str("pong"));
    /// assert_eq!(ctx.invoke("magic", vec![], Default::default()).unwrap(), d4rt_rs::Value::Int(42));
    /// ```
    pub fn invoke(
        &mut self,
        name: &str,
        positional: Vec<Value>,
        named: HashMap<String, Value>,
    ) -> Result<Value, InterpError> {
        let instance = self.interpreted_instance.clone().ok_or_else(|| {
            InterpError::runtime("No interpreted instance found. Call execute() before invoke().")
        })?;
        catch_dart_panic(|| {
            crate::state::with_current(&self.state, || {
                let mut visitor = InterpreterVisitor::new(self.env.clone());
                let member = match &instance {
                    Value::Instance(inst) => {
                        crate::value::instance_get(inst, name, Some(&mut visitor))?
                    }
                    Value::Bridged(b) => visitor.bridged_instance_get(b, name)?,
                    Value::BridgedClass(bc) => visitor.get_bridged_class_member(bc, name)?,
                    other => {
                        return Err(InterpError::runtime(format!(
                            "Cannot invoke '{name}' on {}",
                            value_display(other)
                        )));
                    }
                };
                if matches!(
                    &member,
                    Value::Function(_)
                        | Value::NativeFn(_)
                        | Value::BridgedSuperMethod(_)
                        | Value::BridgedMixinMethod(_)
                        | Value::BridgedEnumMixinMethod(_)
                        | Value::BoundBridgedSuper(_)
                ) {
                    visitor.call_value(&member, positional, named)
                } else {
                    Ok(member)
                }
            })
        })
    }

    /// Evaluates a single Dart expression in the context's global scope.
    pub fn eval_expression(&mut self, source: &str) -> Result<Value, InterpError> {
        catch_dart_panic(|| {
            crate::state::with_current(&self.state, || {
                crate::interp::evaluate_expression_in(&self.env, source)
            })
        })
    }

    /// Like [`Context::execute`], but also runs the async task queue until
    /// the returned future (if any) completes. Handy for calling async Dart
    /// functions from Rust without a manual [`Context::pump_to_completion`].
    /// This is the **blocking** variant; use
    /// [`Context::execute_async_await`] if you want a cooperative,
    /// non-blocking version for tokio-style runtimes.
    ///
    /// ```
    /// # #[cfg(feature = "async")]
    /// {
    /// let mut ctx = d4rt_rs::Context::new();
    /// let src = "Future<int> compute(int n) async { await Future.delayed(Duration(milliseconds: 5)); return n * 2; }";
    /// let v = ctx.execute_async(src, "compute", vec![d4rt_rs::Value::Int(21)], std::collections::HashMap::new()).unwrap();
    /// assert_eq!(v.as_int(), Some(42));
    /// # }
    /// ```
    pub fn execute_async(
        &mut self,
        source: &str,
        name: &str,
        positional: Vec<Value>,
        named: HashMap<String, Value>,
    ) -> Result<Value, InterpError> {
        let v = self.execute(source, name, positional, named)?;
        self.pump_to_completion(v)
    }

    /// Registers a native function callable from Dart code by `name`.
    ///
    /// The closure receives the interpreter visitor (so it can call back into
    /// Dart functions), the positional arguments and the named arguments.
    /// Use [`crate::FromValue`] / [`crate::ToValue`] for typed extraction and
    /// construction.
    ///
    /// ```
    /// use d4rt_rs::ToValue;
    /// let mut ctx = d4rt_rs::Context::new();
    /// ctx.register_function("double_it", |_v, args, _named| {
    ///     let n = d4rt_rs::FromValue::from_value(&args[0]).unwrap_or(0i64);
    ///     Ok((n * 2).to_value())
    /// });
    /// assert_eq!(ctx.eval_expression("double_it(21)").unwrap(), d4rt_rs::Value::Int(42));
    /// ```
    pub fn register_function<F>(&mut self, name: &str, f: F)
    where
        F: Fn(
                &mut InterpreterVisitor,
                Vec<Value>,
                HashMap<String, Value>,
            ) -> Result<Value, InterpError>
            + 'static,
    {
        let native = NativeFunction {
            name: Some(name.to_string()),
            min_arity: 0,
            max_arity: None,
            is_async: false,
            is_generator: false,
            closure: Box::new(f),
            callable_runtime_type: FunctionRuntimeType::untyped(),
        };
        Environment::define(&self.env, name, Value::NativeFn(Rc::new(native)));
    }

    /// Registers a bridged class, making it instantiable from Dart code.
    ///
    /// Corresponds to `D4rt.registerBridgedClass` in `~/d4rt`. Bridge
    /// definitions are built with the helpers in `crate::stdlib::helpers`.
    pub fn register_bridged_class(&mut self, bridged: Arc<BridgedClassDef>) {
        Environment::define_bridge(&self.env, bridged);
    }

    /// Registers a bridged enum (e.g. `MStatus`), making its values
    /// accessible from Dart code as `<name>.<value>`.
    pub fn register_bridged_enum(&mut self, def: Rc<BridgedEnumDef>) {
        Environment::define_bridged_enum(&self.env, def);
    }

    /// Reads a top-level variable, class, or function from the global scope.
    pub fn get(&self, name: &str) -> Result<Value, InterpError> {
        Environment::get(&self.env, name)
    }

    /// Defines a top-level variable in the global scope.
    pub fn set(&mut self, name: &str, value: Value) {
        Environment::define(&self.env, name, value);
    }

    /// Replaces the handler invoked by Dart's `print`. Useful for capturing
    /// program output in tests.
    pub fn set_print_handler<F>(&mut self, handler: F)
    where
        F: Fn(&str) + 'static,
    {
        *self.print_handler.borrow_mut() = Box::new(handler);
    }

    /// Access to the underlying global environment, for advanced use such as
    /// registering bridged enums and extensions.
    pub fn env(&self) -> Rc<RefCell<Environment>> {
        self.env.clone()
    }

    /// Grants a capability to this context. Matches d4rt's
    /// `interpreter.grant(...)` (and `hasPermission`/`revoke` for the other
    /// two). By default nothing is granted: filesystem, network, process,
    /// isolate and environment access all throw a permission error until
    /// explicitly allowed.
    ///
    /// ```
    /// use d4rt_rs::permission::{FilesystemPermission, Permission};
    /// let mut ctx = d4rt_rs::Context::new();
    /// assert!(!ctx.has_permission(&Permission::Filesystem(
    ///     FilesystemPermission::Read,
    /// )));
    /// ctx.grant(Permission::Filesystem(FilesystemPermission::Read));
    /// assert!(ctx.has_permission(&Permission::Filesystem(
    ///     FilesystemPermission::Read,
    /// )));
    /// ```
    pub fn grant(&mut self, permission: Permission) {
        self.state.permissions.grant(permission);
    }

    /// Withdraws exactly this permission. A broader grant (`Any`) keeps
    /// covering the capability afterwards.
    pub fn revoke(&mut self, permission: &Permission) {
        self.state.permissions.revoke(permission);
    }

    /// Registers a Dart source file under `name`, making it importable with
    /// `import 'name';` — e.g. `./mylib.dart` — without any filesystem
    /// access. Virtual modules take precedence over real files when
    /// filesystem module loading is enabled; `dart:` URIs and Rust module
    /// libraries ([`crate::stdlib::register_module_library`])
    /// are resolved before this registry. Registering the same `name` again
    /// replaces the previous source.
    ///
    /// ```
    /// use d4rt_rs::Context;
    /// let mut ctx = Context::new();
    /// ctx.register_dart_module("./mylib.dart", "int answer() => 42;");
    /// let v = ctx
    ///     .eval("import './mylib.dart'; String main() => '${answer()}';")
    ///     .unwrap();
    /// assert_eq!(v.as_str(), Some("42"));
    /// ```
    pub fn register_dart_module(&mut self, name: &str, source: &str) {
        self.state
            .virtual_modules
            .borrow_mut()
            .insert(name.to_string(), source.to_string());
    }

    /// Registers bridged classes under `uri` — a bare name like `cheerio` or
    /// a `package:...` URI — for **this** `Context` only, so
    /// `import 'uri';` resolves to them. Mirrors the process-global
    /// [`crate::stdlib::register_module_library`], but scoped to the sandbox:
    /// different `Context`s never share registrations. Per-`Context` entries
    /// take precedence over the global registry for the same URI. `dart:`
    /// URIs and virtual Dart-source modules
    /// ([`Context::register_dart_module`]) are resolved before this registry.
    ///
    /// ```
    /// use d4rt_rs::class::BridgedClassDef;
    /// use d4rt_rs::stdlib::helpers::static_getter;
    /// use d4rt_rs::{Context, ToValue};
    ///
    /// let mut bc = BridgedClassDef::new("LocalThing");
    /// bc.static_getters.insert("answer".into(), static_getter(|_v| Ok(42_i64.to_value())));
    ///
    /// let mut ctx = Context::new();
    /// ctx.register_module_library("local_thing", vec![std::sync::Arc::new(bc)]);
    /// let v = ctx.eval("import 'local_thing'; int main() => LocalThing.answer;").unwrap();
    /// assert_eq!(v.as_int(), Some(42));
    /// ```
    pub fn register_module_library(&mut self, uri: &str, classes: Vec<Arc<BridgedClassDef>>) {
        let mut reg = self.state.module_libraries.borrow_mut();
        let entry = reg.entry(uri.to_string()).or_default();
        for c in classes {
            if !entry.iter().any(|e| e.name == c.name) {
                entry.push(c);
            }
        }
    }

    /// Per-`Context` equivalent of
    /// [`crate::stdlib::register_package_library`]: registers bridged
    /// classes under a `package:...` URI for this `Context` only. See
    /// [`Context::register_module_library`].
    pub fn register_package_library(&mut self, uri: &str, classes: Vec<Arc<BridgedClassDef>>) {
        self.register_module_library(uri, classes);
    }

    /// Whether the context currently holds a permission covering
    /// `permission` (e.g. `Any` covers `ReadPath("/x")`).
    pub fn has_permission(&self, permission: &Permission) -> bool {
        self.state.permissions.allows(permission)
    }

    /// Internal: processes at most `step_limit` tasks of the async queue for
    /// `v`, stopping early if `v` completes. Never blocks: the caller decides
    /// how many steps to consume and what to do when the budget is exhausted.
    fn drive_bounded(
        &mut self,
        v: &Value,
        step_limit: u64,
        guard: &mut u64,
    ) -> Result<DriveVerdict, InterpError> {
        let state = self.state.clone();
        catch_dart_panic(|| {
            crate::state::with_current(&state, || self.drive_bounded_inner(v, step_limit, guard))
        })
    }

    fn drive_bounded_inner(
        &mut self,
        v: &Value,
        step_limit: u64,
        guard: &mut u64,
    ) -> Result<DriveVerdict, InterpError> {
        let mut visitor = crate::interp::InterpreterVisitor::new(self.env.clone());
        let mut steps: u64 = 0;
        loop {
            *guard += 1;
            if std::env::var("D4RT_DEBUG_AWAIT").is_ok() && (*guard).is_multiple_of(500) {
                eprintln!("PUMP iteration {guard}"); // diagnostic aid (enabled by D4RT_DEBUG_AWAIT)
            }
            if *guard > 1_000_000 {
                return Err(InterpError::runtime("pump runaway guard hit"));
            }
            match crate::async_state::unwrap_future(v) {
                None => return Ok(DriveVerdict::Done(v.clone())),
                Some(f) => {
                    let completed = {
                        let st = f.borrow();
                        st.completed
                    };
                    if completed {
                        let result = {
                            let st = f.borrow();
                            crate::async_state::future_result(&st)
                        };
                        return match result {
                            Ok(val) => Ok(DriveVerdict::Done(val)),
                            Err(e) => Err(InterpError::Throw(e)),
                        };
                    }
                    match crate::async_state::dequeue_task() {
                        Some(task) => {
                            crate::async_state::process_task(&mut visitor, task);
                        }
                        None => {
                            return Err(InterpError::runtime(
                                "Async deadlock: no pending tasks for an incomplete future",
                            ));
                        }
                    }
                }
            }
            steps += 1;
            if steps >= step_limit {
                return Ok(DriveVerdict::Wait);
            }
        }
    }
}

/// Number of async tasks processed per cooperative batch before yielding.
const MAX_STEPS_PER_BATCH: u64 = 64;

/// Outcome of a single bounded drive pass.
enum DriveVerdict {
    /// `v` completed (or was never a future); carry its final value.
    Done(Value),
    /// Still pending; the queue had work but the step budget was exhausted.
    Wait,
}

/// Yields control to the current async executor once, so other tasks/events
/// can run between drive batches. Works with any executor that re-polls a
/// woken future (the `futures` crate, tokio's `block_on`, …).
struct YieldNow;

impl std::future::Future for YieldNow {
    type Output = ();
    fn poll(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        cx.waker().wake_by_ref();
        std::task::Poll::Pending
    }
}

fn yield_now() -> YieldNow {
    YieldNow
}

/// Runs a future to completion using a tiny self-contained executor, so the
/// cooperative API can be tested without pulling in tokio or the `futures`
/// crate.
#[cfg(test)]
fn block_on<F: std::future::Future>(mut fut: F) -> F::Output {
    use std::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};

    fn noop_clone(_: *const ()) -> RawWaker {
        noop_raw_waker()
    }
    fn noop(_: *const ()) {}
    fn noop_raw_waker() -> RawWaker {
        RawWaker::new(
            std::ptr::null(),
            &RawWakerVTable::new(noop_clone, noop, noop, noop),
        )
    }
    let waker = unsafe { Waker::from_raw(noop_raw_waker()) };
    let mut cx = Context::from_waker(&waker);
    // The interpreter is fully deterministic and self-waking, so stopping on
    // `Pending` means the future schedules itself again; loop until ready.
    loop {
        match unsafe { std::pin::Pin::new_unchecked(&mut fut) }.poll(&mut cx) {
            Poll::Ready(r) => return r,
            Poll::Pending => continue,
        }
    }
}

impl Default for Context {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for Context {
    /// Reclaims every OS resource (sockets, servers, watchers, pipes,
    /// processes, file locks) that this `Context` created, so a dropped
    /// sandbox leaks nothing into the process or other `Context`s.
    fn drop(&mut self) {
        #[cfg(feature = "io")]
        crate::stdlib::http::cleanup_context(self.state.id);
        #[cfg(feature = "io")]
        crate::stdlib::io::cleanup_context(self.state.id);
        #[cfg(feature = "io")]
        crate::stdlib::process::cleanup_context(self.state.id);
    }
}

/// A [`Context`] that may be moved to exactly one other thread.
///
/// `Context` is not `Send` because its interpreter heap is built on `Rc`
/// (non-atomic reference counts). Sending it to a single other thread is
/// sound as long as **all** of the following invariants hold:
///
/// - The move happens *between* `Context` method calls (the `eval` /
///   `execute` / `eval_expression` family), never from inside a native
///   closure or `with_current` scope: the source thread's call-scoped
///   `ContextState` handle stack must be empty, so it no longer holds any
///   `Rc` of this context.
/// - The entire `Rc` ownership graph moves with the context. The host must
///   not keep `Value`s, environments or other handles taken from this
///   context and touch them from the original thread afterwards.
/// - The context is never shared back: at most one thread may hold
///   references to its heap at a time.
///
/// # Example
///
/// ```
/// use d4rt_rs::{Context, SendableContext};
///
/// let (tx, rx) = std::sync::mpsc::channel();
/// std::thread::spawn(move || {
///     let mut ctx = SendableContext::new(Context::new()).into_inner();
///     ctx.eval("int f() => 6 * 7;").unwrap();
///     let _ = tx.send(format!("{:?}", ctx.eval_expression("f()").unwrap()));
/// });
/// assert_eq!(rx.recv().unwrap(), "42");
/// ```
pub struct SendableContext(Context);

// SAFETY: sound under the invariants documented on `SendableContext`; the
// non-atomic `Rc` refcounts are only ever touched by the single thread
// that currently owns the context.
unsafe impl Send for SendableContext {}

impl SendableContext {
    /// Wraps a `Context` so it can be moved to one other thread.
    pub fn new(context: Context) -> Self {
        SendableContext(context)
    }

    /// Unwraps the context back (typically on the destination thread).
    pub fn into_inner(self) -> Context {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contexts_are_independent() {
        let mut a = Context::new();
        let mut b = Context::new();
        a.eval("var shared = 10;").unwrap();
        assert!(b.eval_expression("shared").is_err());
        assert_eq!(a.eval_expression("shared").unwrap(), Value::Int(10));
    }

    #[test]
    fn register_and_call() {
        let mut ctx = Context::new();
        ctx.register_function("double_it", |_v, args, _named| {
            let n = crate::helpers::as_int(&args[0]).unwrap_or(0);
            Ok(Value::Int(n * 2))
        });
        assert_eq!(
            ctx.eval_expression("double_it(21)").unwrap(),
            Value::Int(42)
        );
    }

    #[test]
    fn execute_with_args() {
        let mut ctx = Context::new();
        let v = ctx
            .execute(
                "int add(int a, int b) => a + b;",
                "add",
                vec![Value::Int(2), Value::Int(3)],
                HashMap::new(),
            )
            .unwrap();
        assert_eq!(v, Value::Int(5));
    }

    #[test]
    fn print_handler_captures_output() {
        let mut ctx = Context::new();
        let captured = Rc::new(RefCell::new(Vec::new()));
        let c = captured.clone();
        ctx.set_print_handler(move |s| c.borrow_mut().push(s.to_string()));
        ctx.eval("void main() { print('hello'); print('world'); }")
            .unwrap();
        assert_eq!(
            *captured.borrow(),
            vec!["hello".to_string(), "world".to_string()]
        );
    }

    #[cfg(feature = "async")]
    #[test]
    fn execute_async_waits_for_future() {
        let mut ctx = Context::new();
        let src = "Future<int> compute(int n) async { await Future.delayed(Duration(milliseconds: 5)); return n * 2; }";
        let v = ctx
            .execute_async(src, "compute", vec![Value::Int(21)], HashMap::new())
            .unwrap();
        assert_eq!(v, Value::Int(42));
    }

    #[test]
    fn execute_async_returns_sync_values_unchanged() {
        let mut ctx = Context::new();
        let v = ctx
            .execute_async(
                "int add(int a, int b) => a + b;",
                "add",
                vec![Value::Int(2), Value::Int(3)],
                HashMap::new(),
            )
            .unwrap();
        assert_eq!(v, Value::Int(5));
    }

    #[cfg(feature = "async")]
    #[test]
    fn execute_async_await_cooperative_resolves_future() {
        let mut ctx = Context::new();
        let src = "Future<int> compute(int n) async { for (var i = 0; i < 5; i++) { await Future.delayed(Duration(milliseconds: 1)); } return n * 3; }";
        let v = block_on(async {
            ctx.execute_async_await(src, "compute", vec![Value::Int(14)], HashMap::new())
                .await
        })
        .unwrap();
        assert_eq!(v, Value::Int(42));
    }

    #[test]
    fn execute_async_await_cooperative_returns_sync_immediately() {
        let mut ctx = Context::new();
        let v = block_on(async {
            ctx.execute_async_await(
                "int add(int a, int b) => a + b;",
                "add",
                vec![Value::Int(2), Value::Int(3)],
                HashMap::new(),
            )
            .await
        })
        .unwrap();
        assert_eq!(v, Value::Int(5));
    }

    #[test]
    fn execute_async_await_propagates_async_error() {
        let mut ctx = Context::new();
        let src = "Future<int> boom() async { await Future.delayed(Duration(milliseconds: 1)); throw StateError('kaboom'); }";
        let res = block_on(async {
            ctx.execute_async_await(src, "boom", vec![], HashMap::new())
                .await
        });
        assert!(res.is_err());
    }
}
