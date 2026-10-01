//! Per-`Context` runtime state.
//!
//! Everything a sandboxed runtime must not share with other `Context`s
//! (library environments, bridged class registry, receive ports, pending
//! async tasks, the virtual clock) lives in a [`ContextState`] owned by
//! `Context`. The stdlib reaches it through a *call-scoped* thread-local
//! handle ([`with_current`]): it is pushed around every entry point
//! (`Context::execute`, pumping, expression evaluation) and popped
//! afterwards, so no data persists beyond the call and a `Context` can
//! never observe another `Context`'s state.

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, VecDeque};
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};

use crate::async_state::{ResumeContinuation, Task};
use crate::class::BridgedClassDef;
use crate::env::Environment;
use crate::value::Value;

static NEXT_CONTEXT_ID: AtomicI64 = AtomicI64::new(1);

/// Everything the stdlib stores per runtime instance.
/// A raw (unbridged) isolate receive port: the port value plus its
/// keep-alive flag.
pub type RawPort = (Value, Rc<RefCell<bool>>);

/// Everything the stdlib stores per runtime instance.
pub struct ContextState {
    /// Stable unique id; also used to scope OS resources (sockets,
    /// watchers, processes) so a dropped `Context` can reclaim them.
    pub id: i64,
    /// Per-`Context` cache of loaded `dart:*` library environments.
    pub libraries: RefCell<HashMap<String, Rc<RefCell<Environment>>>>,
    /// Per-`Context` bridged class registry (replaces the old shared
    /// thread-local one).
    pub bridged: RefCell<HashMap<String, Arc<BridgedClassDef>>>,
    /// Receive ports (`dart:isolate`) keyed by port id.
    pub receive_ports: RefCell<HashMap<i64, Value>>,
    /// Raw receive ports with their keep-alive flags.
    pub raw_ports: RefCell<HashMap<i64, RawPort>>,
    /// Resume continuations captured by `yield*`, keyed by marker pointer.
    pub yield_star_resumes: RefCell<HashMap<usize, ResumeContinuation>>,
    /// Pending async tasks for this context.
    pub tasks: RefCell<VecDeque<Task>>,
    /// Simulated virtual clock (microseconds) for this context.
    pub clock: Cell<i64>,
    /// Capabilities granted by the host; every dangerous stdlib operation
    /// is checked against this before running.
    pub permissions: crate::permission::PermissionSet,
    /// Virtual module registry: module name (as written in `import`, e.g.
    /// `./mylib.dart`) → Dart source. Consulted before filesystem loading,
    /// so embedders can serve modules without granting
    /// [`Permission::ModuleFiles`](crate::permission::Permission::ModuleFiles).
    pub virtual_modules: RefCell<HashMap<String, String>>,
    /// Per-`Context` module library registry: URI (bare name or
    /// `package:...`) → bridged classes, registered via
    /// `Context::register_module_library`. Consulted before the
    /// process-global registry
    /// ([`crate::stdlib::register_module_library`]).
    pub module_libraries: RefCell<HashMap<String, Vec<Arc<BridgedClassDef>>>>,
}

impl ContextState {
    pub fn new() -> Rc<Self> {
        Rc::new(Self {
            id: NEXT_CONTEXT_ID.fetch_add(1, Ordering::Relaxed),
            libraries: RefCell::new(HashMap::new()),
            bridged: RefCell::new(HashMap::new()),
            receive_ports: RefCell::new(HashMap::new()),
            raw_ports: RefCell::new(HashMap::new()),
            yield_star_resumes: RefCell::new(HashMap::new()),
            tasks: RefCell::new(VecDeque::new()),
            clock: Cell::new(0),
            permissions: crate::permission::PermissionSet::new(),
            virtual_modules: RefCell::new(HashMap::new()),
            module_libraries: RefCell::new(HashMap::new()),
        })
    }
}

thread_local! {
    static CURRENT: RefCell<Vec<Rc<ContextState>>> = const { RefCell::new(Vec::new()) };
}

/// The state of the `Context` currently executing on this thread, if any.
pub fn try_current() -> Option<Rc<ContextState>> {
    CURRENT.with(|c| c.borrow().last().cloned())
}

/// The state of the `Context` currently executing on this thread.
///
/// Panics when called outside of any `Context` (i.e. not within
/// [`with_current`]).
pub fn current() -> Rc<ContextState> {
    try_current().unwrap_or_else(|| {
        panic!("d4rt: stdlib state accessed outside of a Context (missing with_current scope)")
    })
}

/// Runs `f` with `state` as the current context state, restoring the
/// previous one afterwards (nesting is supported).
pub fn with_current<T>(state: &Rc<ContextState>, f: impl FnOnce() -> T) -> T {
    CURRENT.with(|c| c.borrow_mut().push(state.clone()));
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(f));
    CURRENT.with(|c| {
        c.borrow_mut().pop();
    });
    match result {
        Ok(v) => v,
        Err(payload) => std::panic::resume_unwind(payload),
    }
}
