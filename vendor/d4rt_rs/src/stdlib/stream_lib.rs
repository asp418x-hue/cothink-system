//! dart:async — Stream, StreamController, StreamSubscription,
//! StreamTransformer, StreamSink, EventSink, StreamIterator.
//!
//! Native representations (all `Value::List` cell vectors):
//! - Stream: `[state, ops]`; ops holds the pipeline of transformation
//!   cells applied when an event reaches a subscription.
//! - stream state: `[events, listeners, closed, controller, broadcast,
//!   periodic_cb, periodic_counter, periodic_ticking]`; `events` is a list
//!   of data values (or `[Bool(true), error]` for buffered errors) emitted
//!   before a listener attaches; `listeners` holds subscription values.
//! - StreamController native: the shared stream state; the controller cells
//!   live at state\[3\]: `[sync, broadcast, close_called, paused, on_listen,
//!   on_pause, on_resume, on_cancel, done_future]`.
//! - StreamSubscription: `[state, ops, alive, on_data, on_error, on_done,
//!   paused, pause_buffer]`.
//! - StreamTransformer: `[inner, bind_cb, handlers]`; `handlers` =
//!   `[handle_data, handle_error, handle_done]` (Null when absent).
//! - StreamSink: `[controller-native]`.
//! - EventSink: `[sub, ops, index]` — events added flow down the op chain
//!   starting at `index`.
//! - StreamIterator: `[state, ops, it]`; `it` = `[started, done, current,
//!   has_current, pending, error]`.
//!
//! Op cells are `[kind, ...]` lists. `on_data`/`on_error`/`on_done` cells of
//! a subscription hold either a Dart closure, `Null`, or an internal marker
//! `[Int(kind), ...]` driving the future-returning helpers (`toList`, `first`,
//! etc.); their extras are either raw values (I_FWD, I_ITER) or "slots"
//! (single-cell lists) that the handler mutates in place.

use super::helpers as h;
use crate::async_state;
use crate::callable::NativeFunction;
use crate::class::BridgedClassDef;
use crate::env::Environment;
use crate::error::InterpError;
use crate::rt::FunctionRuntimeType;
use crate::value::{DartSet, Value};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

/// A `Stream` value's backing `(state, ops)` list pair.
pub type StreamStateParts = (Rc<RefCell<Vec<Value>>>, Rc<RefCell<Vec<Value>>>);

const K_MAP: i64 = 2;
const K_WHERE: i64 = 3;
const K_EXPAND: i64 = 4;
const K_TAKE: i64 = 5;
const K_SKIP: i64 = 6;
const K_TAKE_WHILE: i64 = 7;
const K_SKIP_WHILE: i64 = 8;
const K_DISTINCT: i64 = 9;
const K_ASYNC_MAP: i64 = 10;
const K_ASYNC_EXPAND: i64 = 11;
const K_HANDLER: i64 = 13;
const K_HANDLE_ERROR: i64 = 14;
/// Stream.timeout op: `[K_TIMEOUT, duration_us, onTimeout, gen]`. `gen`
/// increments on every data/error event (and on done) so pending
/// `Task::Timeout` checks with an older gen become stale.
const K_TIMEOUT: i64 = 15;

// Internal-listener marker kinds.
const I_COLLECT: i64 = 1;
const I_FIRST: i64 = 2;
const I_ISEMPTY: i64 = 3;
const I_LENGTH: i64 = 4;
const I_JOIN: i64 = 5;
const I_TOSET: i64 = 6;
const I_FOLD: i64 = 7;
const I_REDUCE: i64 = 8;
const I_ANY: i64 = 9;
const I_EVERY: i64 = 10;
const I_CONTAINS: i64 = 11;
const I_FOREACH: i64 = 12;
const I_ELEMENTAT: i64 = 13;
const I_FIRSTWHERE: i64 = 14;
const I_LASTWHERE: i64 = 15;
const I_SINGLEWHERE: i64 = 16;
const I_DRAIN: i64 = 17;
const I_LAST: i64 = 18;
const I_SINGLE: i64 = 19;
const I_ITER: i64 = 20;
const I_FWD: i64 = 21;
/// Forwards events of a stream being `yield*`'d into the generator's
/// own stream state. Cells: `[I_GEN_FWD, gen_state, alive]`.
const I_GEN_FWD: i64 = 22;
/// Stream.asFuture marker: `[I_ASFUTURE, future, [seed]]`; the last data
/// event (or the seed) completes the future on done.
const I_ASFUTURE: i64 = 23;

/// Marker kind for stream subscriptions created by `await for`.
pub const I_ITER_KIND: i64 = I_ITER;
/// Marker kind used by `yield*` subscriptions (see `I_GEN_FWD`).
pub const I_GEN_FWD_KIND: i64 = I_GEN_FWD;

pub fn stash_yield_star(key: usize, cont: crate::async_state::ResumeContinuation) {
    crate::state::current()
        .yield_star_resumes
        .borrow_mut()
        .insert(key, cont);
}

fn take_yield_star(key: usize) -> Option<crate::async_state::ResumeContinuation> {
    crate::state::current()
        .yield_star_resumes
        .borrow_mut()
        .remove(&key)
}

fn define_bridge(env: &Rc<RefCell<Environment>>, bc: BridgedClassDef) {
    let bc = Arc::new(bc);
    h::register_class(bc.clone());
    Environment::define_bridge(env, bc);
}

pub fn load(env: &Rc<RefCell<Environment>>) {
    define_bridge(env, stream_definition());
    define_bridge(env, stream_controller_definition());
    define_bridge(env, stream_subscription_definition());
    define_bridge(env, stream_transformer_definition());
    define_bridge(env, stream_sink_definition());
    define_bridge(env, event_sink_definition());
    define_bridge(env, stream_iterator_definition());
}

// ---------- low-level helpers ----------

fn wrap(bc_name: &str, native: Value) -> Value {
    h::wrap(&h::class(bc_name), native)
}

fn wrap_future(state: Rc<RefCell<async_state::FutureState>>) -> Value {
    wrap("Future", Value::Future(state))
}

fn new_future() -> Value {
    wrap_future(Rc::new(RefCell::new(async_state::FutureState::new())))
}

fn future_value(v: Value) -> Value {
    let f = Rc::new(RefCell::new(async_state::FutureState::new()));
    async_state::complete(&f, Ok(v));
    wrap_future(f)
}

fn as_bool(v: &Value) -> bool {
    h::as_bool(v).unwrap_or(false)
}

fn truthy(v: &Value) -> bool {
    !matches!(v, Value::Null | Value::Bool(false))
}

fn call_arity(v: &Value) -> usize {
    match v.to_callable() {
        Some(crate::callable::Callable::Interpreted(f)) => f.positional_param_count(),
        Some(crate::callable::Callable::Native(f)) => f.max_arity.unwrap_or(f.min_arity),
        _ => 1,
    }
}

fn call_cb(
    visitor: &mut crate::interp::InterpreterVisitor,
    cb: &Value,
    args: Vec<Value>,
) -> Result<Value, InterpError> {
    let n = call_arity(cb).max(1);
    let args: Vec<Value> = args.into_iter().take(n).collect();
    visitor.call_value(cb, args, HashMap::new())
}

fn as_list(v: &Value) -> Option<Rc<RefCell<Vec<Value>>>> {
    h::as_list(v)
}

fn stream_state(v: &Value) -> Rc<RefCell<Vec<Value>>> {
    as_list(v)
        .and_then(|l| l.borrow().first().cloned())
        .and_then(|s| as_list(&s))
        .unwrap_or_else(|| Rc::new(RefCell::new(Vec::new())))
}

fn stream_ops(v: &Value) -> Rc<RefCell<Vec<Value>>> {
    as_list(v)
        .and_then(|l| l.borrow().get(1).cloned())
        .and_then(|s| as_list(&s))
        .unwrap_or_else(|| Rc::new(RefCell::new(Vec::new())))
}

fn sub_state(sub: &Value) -> Rc<RefCell<Vec<Value>>> {
    as_list(sub)
        .and_then(|l| l.borrow().first().cloned())
        .and_then(|s| as_list(&s))
        .unwrap_or_else(|| Rc::new(RefCell::new(Vec::new())))
}

fn sub_ops(sub: &Value) -> Rc<RefCell<Vec<Value>>> {
    as_list(sub)
        .and_then(|l| l.borrow().get(1).cloned())
        .and_then(|s| as_list(&s))
        .unwrap_or_else(|| Rc::new(RefCell::new(Vec::new())))
}

fn sub_is_alive(sub: &Value) -> bool {
    as_list(sub)
        .and_then(|l| l.borrow().get(2).cloned())
        .and_then(|c| h::as_bool(&c))
        .unwrap_or(false)
}

fn sub_is_paused(sub: &Value) -> bool {
    as_list(sub)
        .and_then(|l| l.borrow().get(6).cloned())
        .and_then(|c| h::as_bool(&c))
        .unwrap_or(false)
}

fn sub_cell(sub: &Value, i: usize) -> Value {
    as_list(sub)
        .and_then(|l| l.borrow().get(i).cloned())
        .unwrap_or(Value::Null)
}

fn sub_eq(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Bridged(x), Value::Bridged(y)) => Rc::ptr_eq(x, y),
        _ => a == b,
    }
}

fn state_listeners(state: &Rc<RefCell<Vec<Value>>>) -> Rc<RefCell<Vec<Value>>> {
    state
        .borrow()
        .get(1)
        .cloned()
        .and_then(|l| as_list(&l))
        .unwrap_or_else(|| Rc::new(RefCell::new(Vec::new())))
}

fn state_events(state: &Rc<RefCell<Vec<Value>>>) -> Rc<RefCell<Vec<Value>>> {
    state
        .borrow()
        .first()
        .cloned()
        .and_then(|l| as_list(&l))
        .unwrap_or_else(|| Rc::new(RefCell::new(Vec::new())))
}

fn state_controller(state: &Rc<RefCell<Vec<Value>>>) -> Option<Rc<RefCell<Vec<Value>>>> {
    state.borrow().get(3).cloned().and_then(|c| as_list(&c))
}

fn state_is_closed(state: &Rc<RefCell<Vec<Value>>>) -> bool {
    state
        .borrow()
        .get(2)
        .cloned()
        .and_then(|c| h::as_bool(&c))
        .unwrap_or(false)
}

fn state_is_broadcast(state: &Rc<RefCell<Vec<Value>>>) -> bool {
    let own = state
        .borrow()
        .get(4)
        .cloned()
        .and_then(|c| h::as_bool(&c))
        .unwrap_or(false);
    if own {
        return true;
    }
    state_controller(state)
        .and_then(|c| c.borrow().get(1).cloned())
        .and_then(|x| h::as_bool(&x))
        .unwrap_or(false)
}

fn controller_cells(state: &Rc<RefCell<Vec<Value>>>) -> Option<Rc<RefCell<Vec<Value>>>> {
    state_controller(state)
}

fn new_stream(state: Rc<RefCell<Vec<Value>>>, ops: Vec<Value>) -> Value {
    wrap(
        "Stream",
        Value::List(Rc::new(RefCell::new(vec![
            Value::List(state),
            Value::List(Rc::new(RefCell::new(ops))),
        ]))),
    )
}

/// Creates the stream an `async*` generator produces, returning
/// `(stream, state)` where `state` is the event-queue state value.
pub fn make_gen_stream() -> (Value, Value) {
    let state = new_state();
    let stream = new_stream(state.clone(), Vec::new());
    (stream, Value::List(state))
}

/// The pointer of a value's backing list, used as a registry key.
pub fn list_ptr(v: &Value) -> Option<usize> {
    as_list(v).map(|l| Rc::as_ptr(&l) as usize)
}

/// Public alias of the internal `as_list` helper.
pub fn as_list_rc(v: &Value) -> Option<Rc<RefCell<Vec<Value>>>> {
    as_list(v)
}

/// Splits a Stream value into its `(state, ops)` backing lists.
pub fn gen_state_parts(stream: &Value) -> Option<StreamStateParts> {
    let v = match stream {
        Value::Bridged(b) => b.borrow().native.clone(),
        other => other.clone(),
    };
    let l = as_list(&v)?;
    let s = l.borrow().first().cloned()?;
    let o = l.borrow().get(1).cloned()?;
    drop(l);
    match (as_list(&s), as_list(&o)) {
        (Some(s), Some(o)) => Some((s, o)),
        _ => None,
    }
}

/// Emits a data event from an `async*` body into its own stream: buffered
/// when no listener is attached yet, otherwise delivered as a task.
pub fn gen_emit(state: &Value, event: Value) {
    if std::env::var("D4RT_DBG").is_ok() {
        let listeners = as_list(state)
            .map(|s| state_listeners(&s).borrow().len())
            .unwrap_or(0);
        eprintln!("[dbg] gen_emit event={:?} listeners={}", event, listeners);
    }
    let state = match as_list(state) {
        Some(s) => s,
        None => return,
    };
    if state_listeners(&state).borrow().is_empty() {
        state_events(&state).borrow_mut().push(event);
    } else {
        async_state::enqueue_task(async_state::Task::StreamEvent {
            state: Value::List(state),
            event,
        });
    }
}

/// Subscribes the interpreter to an inner stream, using `marker` for all
/// three callbacks (triggers `onListen` and drains buffered events).
pub fn gen_subscribe(
    visitor: &mut crate::interp::InterpreterVisitor,
    state: &Rc<RefCell<Vec<Value>>>,
    ops: &Rc<RefCell<Vec<Value>>>,
    marker: Value,
) -> Value {
    make_sub(visitor, state, ops, marker.clone(), marker.clone(), marker)
}

/// Cancels the subscription stashed at `it[6]` of an `await for`
/// iterator state.
pub fn gen_cancel_sub(it: &Value) {
    let Some(sub) = as_list(it).and_then(|l| l.borrow().get(6).cloned()) else {
        return;
    };
    let Some(sub) = as_list(&sub) else {
        return;
    };
    let state = sub.borrow().first().cloned();
    if sub.borrow().len() > 2 {
        sub.borrow_mut()[2] = Value::Bool(false);
    }
    if let Some(state) = state.and_then(|s| as_list(&s)) {
        let listeners = state_listeners(&state);
        listeners
            .borrow_mut()
            .retain(|x| !as_list(x).is_some_and(|lx| Rc::ptr_eq(&lx, &sub)));
    }
}

fn new_state() -> Rc<RefCell<Vec<Value>>> {
    Rc::new(RefCell::new(vec![
        Value::list(),      // 0 events
        Value::list(),      // 1 listeners
        Value::Bool(false), // 2 closed
        Value::Null,        // 3 controller
        Value::Bool(false), // 4 broadcast
        Value::Null,        // 5 periodic callback
        Value::Int(0),      // 6 periodic counter
        Value::Bool(false), // 7 periodic ticking
    ]))
}

fn op_cell_at(ops: &Rc<RefCell<Vec<Value>>>, index: usize) -> Option<Value> {
    let b = ops.borrow();
    b.get(index).cloned()
}

/// Reads a cell of an op cell (`Value::List`).
fn cell_get(cell: &Value, i: usize) -> Value {
    as_list(cell)
        .and_then(|l| l.borrow().get(i).cloned())
        .unwrap_or(Value::Null)
}

/// Writes a cell of an op cell (`Value::List`).
fn cell_set(cell: &Value, i: usize, v: Value) {
    if let Some(l) = as_list(cell) {
        let mut c = l.borrow_mut();
        if c.len() > i {
            c[i] = v;
        }
    }
}

fn op_kind(cell: &Value) -> i64 {
    as_list(cell)
        .and_then(|l| l.borrow().first().cloned())
        .and_then(|k| h::as_int(&k))
        .unwrap_or(-1)
}

fn is_error_entry(e: &Value) -> Option<Value> {
    as_list(e).and_then(|l| {
        let b = l.borrow();
        if b.len() == 2 && b.first().and_then(h::as_bool) == Some(true) {
            b.get(1).cloned()
        } else {
            None
        }
    })
}

/// Enqueues tasks for events buffered before a listener attached and clears
/// the buffer.
fn drain_events(state: &Rc<RefCell<Vec<Value>>>) {
    if std::env::var("D4RT_DBG").is_ok() {
        eprintln!(
            "[dbg] drain_events ptr={:?} events={} listeners={}",
            Rc::as_ptr(state),
            state_events(state).borrow().len(),
            state_listeners(state).borrow().len()
        );
    }
    let events: Vec<Value> = state_events(state).borrow().clone();
    if events.is_empty() && !state_is_closed(state) {
        return;
    }
    state_events(state).borrow_mut().clear();
    for e in events {
        if let Some(err) = is_error_entry(&e) {
            async_state::enqueue_task(async_state::Task::StreamError {
                state: Value::List(state.clone()),
                error: err,
            });
        } else {
            async_state::enqueue_task(async_state::Task::StreamEvent {
                state: Value::List(state.clone()),
                event: e,
            });
        }
    }
    if state_is_closed(state) {
        async_state::enqueue_task(async_state::Task::StreamDone {
            state: Value::List(state.clone()),
        });
    }
}

// ---------- subscription ----------

fn make_sub(
    visitor: &mut crate::interp::InterpreterVisitor,
    state: &Rc<RefCell<Vec<Value>>>,
    ops: &Rc<RefCell<Vec<Value>>>,
    on_data: Value,
    on_error: Value,
    on_done: Value,
) -> Value {
    let sub = wrap(
        "StreamSubscription",
        Value::List(Rc::new(RefCell::new(vec![
            Value::List(state.clone()),
            Value::List(ops.clone()),
            Value::Bool(true), // alive
            on_data,
            on_error,
            on_done,
            Value::Bool(false), // paused
            Value::list(),      // pause buffer
            Value::Bool(false), // cancel on error
        ]))),
    );
    let first_listener = state_listeners(state).borrow().is_empty();
    if std::env::var("D4RT_DBG").is_ok() {
        eprintln!(
            "[dbg] make_sub: pushed sub, ptr={:?} listeners={}",
            Rc::as_ptr(state),
            state_listeners(state).borrow().len()
        );
    }
    state_listeners(state).borrow_mut().push(sub.clone());
    if first_listener && let Some(ctrl) = controller_cells(state) {
        let on_listen = ctrl.borrow().get(4).cloned().unwrap_or(Value::Null);
        if !on_listen.is_null() {
            let _ = call_cb(visitor, &on_listen, vec![]);
        }
    }
    enqueue_timeout_ops(&sub, ops);
    drain_events(state);
    // Restart periodic ticks if they stopped.
    let is_periodic = !state
        .borrow()
        .get(5)
        .cloned()
        .unwrap_or(Value::Null)
        .is_null();
    if is_periodic && !as_bool(&state.borrow().get(7).cloned().unwrap_or(Value::Bool(false))) {
        state.borrow_mut()[7] = Value::Bool(true);
        async_state::enqueue_task(async_state::Task::StreamTick {
            state: Value::List(state.clone()),
        });
    }
    sub
}

/// Subscribes without calling user code (used by helpers where no visitor
/// callback is available).
fn make_sub_borrow(
    state: &Rc<RefCell<Vec<Value>>>,
    ops: &Rc<RefCell<Vec<Value>>>,
    on_data: Value,
    on_error: Value,
    on_done: Value,
) -> Value {
    let sub = wrap(
        "StreamSubscription",
        Value::List(Rc::new(RefCell::new(vec![
            Value::List(state.clone()),
            Value::List(ops.clone()),
            Value::Bool(true),
            on_data,
            on_error,
            on_done,
            Value::Bool(false),
            Value::list(),
            Value::Bool(false),
        ]))),
    );
    state_listeners(state).borrow_mut().push(sub.clone());
    enqueue_timeout_ops(&sub, ops);
    drain_events(state);
    sub
}

/// Internal marker: `[kind, future, extras...]`. Extras are "slots"
/// (single-cell lists) mutated in place, except for I_FWD/I_ITER which use
/// raw values.
fn marker(kind: i64, fut: Rc<RefCell<async_state::FutureState>>, extras: Vec<Value>) -> Value {
    let mut cells = vec![Value::Int(kind), Value::Future(fut)];
    cells.extend(extras);
    Value::List(Rc::new(RefCell::new(cells)))
}

fn marker_kind(m: &Value) -> i64 {
    op_kind(m)
}

fn marker_future(m: &Value) -> Option<Rc<RefCell<async_state::FutureState>>> {
    as_list(m)
        .and_then(|l| l.borrow().get(1).cloned())
        .and_then(|f| async_state::unwrap_future(&f))
}

fn marker_extra(m: &Value, i: usize) -> Value {
    as_list(m)
        .and_then(|l| l.borrow().get(i + 2).cloned())
        .unwrap_or(Value::Null)
}

fn marker_cell1(m: &Value) -> Value {
    as_list(m)
        .and_then(|l| l.borrow().get(1).cloned())
        .unwrap_or(Value::Null)
}

fn marker_slot(m: &Value, i: usize) -> Rc<RefCell<Vec<Value>>> {
    marker_extra(m, i)
        .let_list()
        .unwrap_or_else(|| Rc::new(RefCell::new(vec![Value::Null])))
}

fn slot_get(slot: &Rc<RefCell<Vec<Value>>>) -> Value {
    slot.borrow().first().cloned().unwrap_or(Value::Null)
}

fn slot_set(slot: &Rc<RefCell<Vec<Value>>>, v: Value) {
    let mut s = slot.borrow_mut();
    if s.is_empty() {
        s.push(v);
    } else {
        s[0] = v;
    }
}

trait LetList {
    fn let_list(self) -> Option<Rc<RefCell<Vec<Value>>>>;
}
impl LetList for Value {
    fn let_list(self) -> Option<Rc<RefCell<Vec<Value>>>> {
        h::as_list(&self)
    }
}

trait LetSet {
    fn let_set(self) -> Option<Rc<RefCell<DartSet>>>;
}
impl LetSet for Value {
    fn let_set(self) -> Option<Rc<RefCell<DartSet>>> {
        h::as_set(&self)
    }
}

// ---------- delivery ----------

fn cancel_sub(visitor: &mut crate::interp::InterpreterVisitor, sub: &Value) {
    if std::env::var("D4RT_DBG").is_ok() {
        eprintln!("[dbg] cancel_sub called");
    }
    let cells = match as_list(sub) {
        Some(l) => l,
        None => return,
    };
    {
        let mut c = cells.borrow_mut();
        if c.len() > 2 {
            c[2] = Value::Bool(false);
        }
    }
    let state = sub_state(sub);
    let was_listener = {
        let listeners = state_listeners(&state);
        let mut l = listeners.borrow_mut();
        let before = l.len();
        l.retain(|s| !sub_eq(s, sub));
        l.len() != before
    };
    if state_listeners(&state).borrow().is_empty() {
        let is_periodic = !state
            .borrow()
            .get(5)
            .cloned()
            .unwrap_or(Value::Null)
            .is_null();
        if is_periodic {
            state.borrow_mut()[7] = Value::Bool(false);
        }
    }
    if was_listener && let Some(ctrl) = controller_cells(&state) {
        let on_cancel = ctrl.borrow().get(7).cloned().unwrap_or(Value::Null);
        if !on_cancel.is_null() {
            let _ = call_cb(visitor, &on_cancel, vec![]);
        }
    }
}

fn deliver_event_to_listener(
    visitor: &mut crate::interp::InterpreterVisitor,
    sub: &Value,
    event: Value,
) {
    if std::env::var("D4RT_DBG").is_ok() {
        eprintln!(
            "[dbg] deliver_event_to_listener alive={}",
            sub_is_alive(sub)
        );
    }
    if !sub_is_alive(sub) {
        return;
    }
    if sub_is_paused(sub) {
        if let Some(buf) = sub_cell(sub, 7).let_list() {
            buf.borrow_mut().push(event);
        }
        return;
    }
    let on_data = sub_cell(sub, 3);
    if std::env::var("D4RT_DBG").is_ok() {
        eprintln!(
            "[dbg] on_data null? {} is_list? {}",
            on_data.is_null(),
            h::as_list(&on_data).is_some()
        );
    }
    if on_data.is_null() {
        return;
    }
    if h::as_list(&on_data).is_some() {
        handle_internal_data(visitor, sub, &on_data, event);
    } else {
        let _ = call_cb(visitor, &on_data, vec![event]);
    }
}

fn deliver_error_to_listener(
    visitor: &mut crate::interp::InterpreterVisitor,
    sub: &Value,
    error: Value,
) {
    if !sub_is_alive(sub) {
        return;
    }
    let on_error = sub_cell(sub, 4);
    if !on_error.is_null() {
        if h::as_list(&on_error).is_some() {
            handle_internal_error(visitor, sub, &on_error, error);
        } else {
            let _ = call_cb(visitor, &on_error, vec![error, Value::Null]);
        }
    }
    let cancel_on_error = as_bool(&sub_cell(sub, 8));
    if cancel_on_error {
        cancel_sub(visitor, sub);
    }
}

fn deliver_done_to_listener(visitor: &mut crate::interp::InterpreterVisitor, sub: &Value) {
    if !sub_is_alive(sub) {
        return;
    }
    let on_done = sub_cell(sub, 5);
    if on_done.is_null() {
        return;
    }
    if h::as_list(&on_done).is_some() {
        handle_internal_done(visitor, sub, &on_done);
    } else {
        let _ = call_cb(visitor, &on_done, vec![]);
    }
}

fn event_sink(sub: &Value, ops: &Rc<RefCell<Vec<Value>>>, index: usize) -> Value {
    wrap(
        "EventSink",
        Value::List(Rc::new(RefCell::new(vec![
            sub.clone(),
            Value::List(ops.clone()),
            Value::Int(index as i64),
        ]))),
    )
}

/// Walks `event` down the op chain starting at `(ops, index)`. When a chain
/// cell is exhausted, the walk continues at `(rest_ops, rest_index)` (the
/// sibling position after the enclosing chain cell).
fn process_event(
    visitor: &mut crate::interp::InterpreterVisitor,
    sub: &Value,
    ops: &Rc<RefCell<Vec<Value>>>,
    index: usize,
    event: Value,
    rest: Option<(Rc<RefCell<Vec<Value>>>, usize)>,
) {
    let cell = op_cell_at(ops, index);
    let cell = match cell {
        Some(c) => c,
        None => {
            if let Some((ro, ri)) = rest
                && ri < ro.borrow().len()
            {
                process_event(visitor, sub, &ro, ri, event, None);
                return;
            }
            deliver_event_to_listener(visitor, sub, event);
            return;
        }
    };
    match op_kind(&cell) {
        K_MAP => match call_cb(visitor, &cell_get(&cell, 1), vec![event]) {
            Ok(r) => process_event(visitor, sub, ops, index + 1, r, rest),
            Err(e) => process_error(
                visitor,
                sub,
                ops,
                index + 1,
                async_state::exception_value(e),
                rest,
            ),
        },
        K_WHERE => match call_cb(visitor, &cell_get(&cell, 1), vec![event.clone()]) {
            Ok(b) => {
                if truthy(&b) {
                    process_event(visitor, sub, ops, index + 1, event, rest);
                }
            }
            Err(e) => process_error(
                visitor,
                sub,
                ops,
                index + 1,
                async_state::exception_value(e),
                rest,
            ),
        },
        K_EXPAND => match call_cb(visitor, &cell_get(&cell, 1), vec![event]) {
            Ok(r) => match as_list(&r) {
                Some(l) => {
                    let items = l.borrow().clone();
                    for it in items {
                        process_event(visitor, sub, ops, index + 1, it, rest.clone());
                    }
                }
                None => process_event(visitor, sub, ops, index + 1, r, rest),
            },
            Err(e) => process_error(
                visitor,
                sub,
                ops,
                index + 1,
                async_state::exception_value(e),
                rest,
            ),
        },
        K_TAKE => {
            let n = h::as_int(&cell_get(&cell, 1)).unwrap_or(0);
            if n > 0 {
                cell_set(&cell, 1, Value::Int(n - 1));
                process_event(visitor, sub, ops, index + 1, event, rest.clone());
                if n - 1 == 0 {
                    // Real Stream.take emits done once the count is reached,
                    // even if the source never closes.
                    process_done(visitor, sub, ops, index + 1, rest);
                    cancel_sub(visitor, sub);
                }
            } else {
                process_done(visitor, sub, ops, index + 1, rest);
                cancel_sub(visitor, sub);
            }
        }
        K_SKIP => {
            let n = h::as_int(&cell_get(&cell, 1)).unwrap_or(0);
            if n > 0 {
                cell_set(&cell, 1, Value::Int(n - 1));
            } else {
                process_event(visitor, sub, ops, index + 1, event, rest);
            }
        }
        K_TAKE_WHILE => match call_cb(visitor, &cell_get(&cell, 1), vec![event.clone()]) {
            Ok(b) => {
                if truthy(&b) {
                    process_event(visitor, sub, ops, index + 1, event, rest);
                } else {
                    process_done(visitor, sub, ops, index + 1, rest);
                    cancel_sub(visitor, sub);
                }
            }
            Err(e) => process_error(
                visitor,
                sub,
                ops,
                index + 1,
                async_state::exception_value(e),
                rest,
            ),
        },
        K_SKIP_WHILE => {
            let skipping = as_bool(&cell_get(&cell, 1));
            if skipping {
                match call_cb(visitor, &cell_get(&cell, 2), vec![event.clone()]) {
                    Ok(b) => {
                        if truthy(&b) {
                            return;
                        }
                        cell_set(&cell, 1, Value::Bool(false));
                    }
                    Err(e) => {
                        cell_set(&cell, 1, Value::Bool(false));
                        process_error(
                            visitor,
                            sub,
                            ops,
                            index + 1,
                            async_state::exception_value(e),
                            rest,
                        );
                        return;
                    }
                }
            }
            process_event(visitor, sub, ops, index + 1, event, rest);
        }
        K_DISTINCT => {
            let prev = &cell_get(&cell, 1);
            let eq = match prev {
                Value::Null => false,
                p => match &cell_get(&cell, 2) {
                    Value::Null => p == &event,
                    f => call_cb(visitor, f, vec![p.clone(), event.clone()])
                        .map(|r| truthy(&r))
                        .unwrap_or(false),
                },
            };
            if !eq {
                cell_set(&cell, 1, event.clone());
                process_event(visitor, sub, ops, index + 1, event, rest);
            }
        }
        K_ASYNC_MAP => {
            let busy = as_bool(&cell_get(&cell, 2));
            if busy {
                if let Some(q) = cell_get(&cell, 3).let_list() {
                    q.borrow_mut().push(event);
                }
                return;
            }
            cell_set(&cell, 2, Value::Bool(true));
            match call_cb(visitor, &cell_get(&cell, 1), vec![event]) {
                Ok(r) => match async_state::unwrap_future(&r) {
                    Some(f) => {
                        let st = f.borrow();
                        if st.completed {
                            let result = async_state::future_result(&st);
                            drop(st);
                            async_state::enqueue_task(async_state::Task::StreamOpResume {
                                sub: sub.clone(),
                                ops: ops.clone(),
                                index,
                                rest_ops: rest
                                    .as_ref()
                                    .map(|(o, _)| o.clone())
                                    .unwrap_or_else(|| ops.clone()),
                                rest_index: rest.as_ref().map(|(_, i)| *i).unwrap_or(usize::MAX),
                                result,
                            });
                        } else {
                            drop(st);
                            f.borrow_mut().continuations.push(
                                async_state::Continuation::StreamOp {
                                    sub: sub.clone(),
                                    ops: ops.clone(),
                                    index,
                                    rest_ops: rest
                                        .as_ref()
                                        .map(|(o, _)| o.clone())
                                        .unwrap_or_else(|| ops.clone()),
                                    rest_index: rest
                                        .as_ref()
                                        .map(|(_, i)| *i)
                                        .unwrap_or(usize::MAX),
                                },
                            );
                        }
                    }
                    None => {
                        cell_set(&cell, 2, Value::Bool(false));
                        process_event(visitor, sub, ops, index + 1, r, rest);
                    }
                },
                Err(e) => {
                    cell_set(&cell, 2, Value::Bool(false));
                    process_error(
                        visitor,
                        sub,
                        ops,
                        index + 1,
                        async_state::exception_value(e),
                        rest,
                    );
                }
            }
        }
        K_ASYNC_EXPAND => {
            let busy = as_bool(&cell_get(&cell, 2));
            if busy {
                if let Some(q) = cell_get(&cell, 3).let_list() {
                    q.borrow_mut().push(event);
                }
                return;
            }
            cell_set(&cell, 2, Value::Bool(true));
            let r = call_cb(visitor, &cell_get(&cell, 1), vec![event]);
            match r {
                Ok(r) => match async_state::unwrap_future(&r) {
                    Some(f) => {
                        let st = f.borrow();
                        if st.completed {
                            let result = async_state::future_result(&st);
                            drop(st);
                            async_state::enqueue_task(async_state::Task::StreamOpResume {
                                sub: sub.clone(),
                                ops: ops.clone(),
                                index,
                                rest_ops: rest
                                    .as_ref()
                                    .map(|(o, _)| o.clone())
                                    .unwrap_or_else(|| ops.clone()),
                                rest_index: rest.as_ref().map(|(_, i)| *i).unwrap_or(usize::MAX),
                                result,
                            });
                        } else {
                            drop(st);
                            f.borrow_mut().continuations.push(
                                async_state::Continuation::StreamOp {
                                    sub: sub.clone(),
                                    ops: ops.clone(),
                                    index,
                                    rest_ops: rest
                                        .as_ref()
                                        .map(|(o, _)| o.clone())
                                        .unwrap_or_else(|| ops.clone()),
                                    rest_index: rest
                                        .as_ref()
                                        .map(|(_, i)| *i)
                                        .unwrap_or(usize::MAX),
                                },
                            );
                        }
                    }
                    None => match as_list(&r) {
                        Some(s) if s.borrow().len() == 2 => {
                            let s_state = s.borrow()[0].clone();
                            let s_ops = s.borrow()[1].clone();
                            if let (Some(s_state), Some(s_ops)) =
                                (as_list(&s_state), as_list(&s_ops))
                            {
                                let fwd = Value::List(Rc::new(RefCell::new(vec![
                                    Value::Int(I_FWD),
                                    sub.clone(),
                                    Value::List(ops.clone()),
                                    Value::Int(index as i64 + 1),
                                    Value::Int(index as i64),
                                ])));
                                make_sub(visitor, &s_state, &s_ops, fwd.clone(), fwd.clone(), fwd);
                            }
                        }
                        _ => {
                            cell_set(&cell, 2, Value::Bool(false));
                            process_event(visitor, sub, ops, index + 1, r, rest);
                        }
                    },
                },
                Err(e) => {
                    cell_set(&cell, 2, Value::Bool(false));
                    process_error(
                        visitor,
                        sub,
                        ops,
                        index + 1,
                        async_state::exception_value(e),
                        rest,
                    );
                }
            }
        }
        K_HANDLER => {
            let hd = cell_get(&cell, 1).clone();
            if hd.is_null() {
                process_event(visitor, sub, ops, index + 1, event, rest);
            } else {
                let sink = event_sink(sub, ops, index + 1);
                let _ = call_cb(visitor, &hd, vec![event, sink]);
            }
        }
        K_HANDLE_ERROR => {
            // handleError only filters errors; data passes through.
            process_event(visitor, sub, ops, index + 1, event, rest);
        }
        K_TIMEOUT => {
            // A data event resets the timeout.
            reset_timeout(sub, ops, index, &cell);
            process_event(visitor, sub, ops, index + 1, event, rest);
        }
        _ => process_event(visitor, sub, ops, index + 1, event, rest),
    }
}

fn process_error(
    visitor: &mut crate::interp::InterpreterVisitor,
    sub: &Value,
    ops: &Rc<RefCell<Vec<Value>>>,
    index: usize,
    error: Value,
    rest: Option<(Rc<RefCell<Vec<Value>>>, usize)>,
) {
    let cell = op_cell_at(ops, index);
    let cell = match cell {
        Some(c) => c,
        None => {
            if let Some((ro, ri)) = rest
                && ri < ro.borrow().len()
            {
                process_error(visitor, sub, &ro, ri, error, None);
                return;
            }
            deliver_error_to_listener(visitor, sub, error);
            return;
        }
    };
    match op_kind(&cell) {
        K_HANDLER => {
            let he = cell_get(&cell, 2);
            if he.is_null() {
                process_error(visitor, sub, ops, index + 1, error, rest);
            } else {
                let sink = event_sink(sub, ops, index + 1);
                let _ = call_cb(visitor, &he, vec![error, Value::Null, sink]);
            }
        }
        K_HANDLE_ERROR => {
            let test = cell_get(&cell, 2).clone();
            let passes = match &test {
                Value::Null => true,
                t => call_cb(visitor, t, vec![error.clone()])
                    .map(|r| truthy(&r))
                    .unwrap_or(true),
            };
            if !passes || cell_get(&cell, 1).is_null() {
                process_error(visitor, sub, ops, index + 1, error, rest);
            } else {
                let busy = as_bool(&cell_get(&cell, 3));
                if busy {
                    if let Some(q) = cell_get(&cell, 4).let_list() {
                        q.borrow_mut().push(error);
                    }
                } else {
                    cell_set(&cell, 3, Value::Bool(true));
                    let r = call_cb(
                        visitor,
                        &cell_get(&cell, 1),
                        vec![error.clone(), Value::Null],
                    );
                    match r {
                        Ok(r) => match async_state::unwrap_future(&r) {
                            Some(f) => {
                                let st = f.borrow();
                                if st.completed {
                                    let result = async_state::future_result(&st);
                                    drop(st);
                                    async_state::enqueue_task(async_state::Task::StreamOpResume {
                                        sub: sub.clone(),
                                        ops: ops.clone(),
                                        index,
                                        rest_ops: rest
                                            .as_ref()
                                            .map(|(o, _)| o.clone())
                                            .unwrap_or_else(|| ops.clone()),
                                        rest_index: rest
                                            .as_ref()
                                            .map(|(_, i)| *i)
                                            .unwrap_or(usize::MAX),
                                        result,
                                    });
                                } else {
                                    drop(st);
                                    f.borrow_mut().continuations.push(
                                        async_state::Continuation::StreamOp {
                                            sub: sub.clone(),
                                            ops: ops.clone(),
                                            index,
                                            rest_ops: rest
                                                .as_ref()
                                                .map(|(o, _)| o.clone())
                                                .unwrap_or_else(|| ops.clone()),
                                            rest_index: rest
                                                .as_ref()
                                                .map(|(_, i)| *i)
                                                .unwrap_or(usize::MAX),
                                        },
                                    );
                                }
                            }
                            None => {
                                cell_set(&cell, 3, Value::Bool(false));
                                drain_handle_error_queue(visitor, sub, ops, index, rest);
                            }
                        },
                        Err(_) => {
                            cell_set(&cell, 3, Value::Bool(false));
                            drain_handle_error_queue(visitor, sub, ops, index, rest);
                        }
                    }
                }
            }
        }
        K_TIMEOUT => {
            // An error event also resets the timeout.
            reset_timeout(sub, ops, index, &cell);
            process_error(visitor, sub, ops, index + 1, error, rest);
        }
        _ => process_error(visitor, sub, ops, index + 1, error, rest),
    }
}

/// Bumps the gen of a K_TIMEOUT cell (invalidating pending timeout tasks),
/// records the virtual time of the reset and enqueues a fresh
/// `Task::Timeout` with the new gen.
fn reset_timeout(sub: &Value, ops: &Rc<RefCell<Vec<Value>>>, index: usize, cell: &Value) {
    let gen_id = h::as_int(&cell_get(cell, 3)).unwrap_or(0) + 1;
    cell_set(cell, 3, Value::Int(gen_id));
    cell_set(cell, 4, Value::Int(async_state::virtual_now()));
    async_state::enqueue_task(async_state::Task::Timeout {
        sub: sub.clone(),
        ops: ops.clone(),
        index,
        gen_id,
    });
}

fn drain_handle_error_queue(
    visitor: &mut crate::interp::InterpreterVisitor,
    sub: &Value,
    ops: &Rc<RefCell<Vec<Value>>>,
    index: usize,
    rest: Option<(Rc<RefCell<Vec<Value>>>, usize)>,
) {
    let cell = op_cell_at(ops, index).unwrap_or(Value::Null);
    let queued: Vec<Value> = h::as_list(&cell)
        .and_then(|l| l.borrow().get(4).cloned())
        .and_then(|q| as_list(&q))
        .map(|q| q.borrow().clone())
        .unwrap_or_default();
    if let Some(q) = cell
        .let_list()
        .and_then(|l| l.borrow().get(4).cloned())
        .and_then(|q| as_list(&q))
    {
        q.borrow_mut().clear();
    }
    for e in queued {
        process_error(visitor, sub, ops, index, e, rest.clone());
    }
}

fn process_done(
    visitor: &mut crate::interp::InterpreterVisitor,
    sub: &Value,
    ops: &Rc<RefCell<Vec<Value>>>,
    index: usize,
    rest: Option<(Rc<RefCell<Vec<Value>>>, usize)>,
) {
    let cell = op_cell_at(ops, index);
    let cell = match cell {
        Some(c) => c,
        None => {
            if let Some((ro, ri)) = rest
                && ri < ro.borrow().len()
            {
                process_done(visitor, sub, &ro, ri, None);
                return;
            }
            deliver_done_to_listener(visitor, sub);
            return;
        }
    };
    match op_kind(&cell) {
        K_TIMEOUT => {
            // Stream done: invalidate any pending timeout check.
            let gen_id = h::as_int(&cell_get(&cell, 3)).unwrap_or(0) + 1;
            cell_set(&cell, 3, Value::Int(gen_id));
            process_done(visitor, sub, ops, index + 1, rest);
        }
        K_HANDLER => {
            let hd_done = cell_get(&cell, 3).clone();
            if hd_done.is_null() {
                process_done(visitor, sub, ops, index + 1, rest);
            } else {
                let sink = event_sink(sub, ops, index + 1);
                let _ = call_cb(visitor, &hd_done, vec![sink]);
            }
        }
        K_ASYNC_MAP | K_ASYNC_EXPAND => {
            // Delay done while an async op still has queued events.
            if as_bool(&cell_get(&cell, 2)) {
                cell_set(&cell, 4, Value::Bool(true));
            } else {
                process_done(visitor, sub, ops, index + 1, rest);
            }
        }
        _ => process_done(visitor, sub, ops, index + 1, rest),
    }
}

// ---------- internal (future-returning) listeners ----------

fn complete_future(m: &Value, result: Result<Value, Value>) {
    if let Some(f) = marker_future(m) {
        async_state::complete(&f, result);
    }
}

fn handle_internal_data(
    visitor: &mut crate::interp::InterpreterVisitor,
    sub: &Value,
    m: &Value,
    event: Value,
) {
    let kind = marker_kind(m);
    if std::env::var("D4RT_DBG").is_ok() {
        eprintln!("[dbg] handle_internal_data kind={kind}");
    }
    match kind {
        I_COLLECT => {
            if let Some(acc) = slot_get(&marker_slot(m, 0)).let_list() {
                acc.borrow_mut().push(event);
            }
        }
        I_FIRST => {
            complete_future(m, Ok(event));
            cancel_sub(visitor, sub);
        }
        I_ISEMPTY => {
            complete_future(m, Ok(Value::Bool(false)));
            cancel_sub(visitor, sub);
        }
        I_LENGTH => {
            let n = slot_get(&marker_slot(m, 0)).as_int().unwrap_or(0);
            slot_set(&marker_slot(m, 0), Value::Int(n + 1));
        }
        I_JOIN => {
            if let Some(buf) = slot_get(&marker_slot(m, 0)).let_list() {
                buf.borrow_mut().push(event);
            }
        }
        I_TOSET => {
            if let Some(set) = slot_get(&marker_slot(m, 0)).let_set() {
                set.borrow_mut().insert(event);
            }
        }
        I_FOLD => {
            let acc = slot_get(&marker_slot(m, 0));
            let closure = slot_get(&marker_slot(m, 1));
            if let Ok(r) = call_cb(visitor, &closure, vec![acc, event]) {
                slot_set(&marker_slot(m, 0), r);
            }
        }
        I_REDUCE => {
            let closure = slot_get(&marker_slot(m, 0));
            let prev = slot_get(&marker_slot(m, 1));
            let r = if prev.is_null() {
                Ok(event)
            } else {
                call_cb(visitor, &closure, vec![prev, event])
            };
            match r {
                Ok(v) => slot_set(&marker_slot(m, 1), v),
                Err(e) => {
                    complete_future(m, Err(async_state::exception_value(e)));
                    cancel_sub(visitor, sub);
                }
            }
        }
        I_ANY => {
            let test = slot_get(&marker_slot(m, 0));
            if let Ok(b) = call_cb(visitor, &test, vec![event.clone()])
                && truthy(&b)
            {
                complete_future(m, Ok(Value::Bool(true)));
                cancel_sub(visitor, sub);
            }
        }
        I_EVERY => {
            let test = slot_get(&marker_slot(m, 0));
            if let Ok(b) = call_cb(visitor, &test, vec![event.clone()])
                && !truthy(&b)
            {
                complete_future(m, Ok(Value::Bool(false)));
                cancel_sub(visitor, sub);
            }
        }
        I_CONTAINS => {
            let needle = slot_get(&marker_slot(m, 0));
            if needle == event {
                complete_future(m, Ok(Value::Bool(true)));
                cancel_sub(visitor, sub);
            }
        }
        I_FOREACH => {
            let closure = slot_get(&marker_slot(m, 0));
            let _ = call_cb(visitor, &closure, vec![event]);
        }
        I_ELEMENTAT => {
            let count = slot_get(&marker_slot(m, 0)).as_int().unwrap_or(0);
            let target = slot_get(&marker_slot(m, 1)).as_int().unwrap_or(-1);
            if count == target {
                complete_future(m, Ok(event));
                cancel_sub(visitor, sub);
            } else {
                slot_set(&marker_slot(m, 0), Value::Int(count + 1));
            }
        }
        I_FIRSTWHERE => {
            let test = slot_get(&marker_slot(m, 0));
            if let Ok(b) = call_cb(visitor, &test, vec![event.clone()])
                && truthy(&b)
            {
                complete_future(m, Ok(event));
                cancel_sub(visitor, sub);
            }
        }
        I_LASTWHERE => {
            let test = slot_get(&marker_slot(m, 0));
            if let Ok(b) = call_cb(visitor, &test, vec![event.clone()])
                && truthy(&b)
            {
                slot_set(&marker_slot(m, 2), event);
            }
        }
        I_SINGLEWHERE => {
            let test = slot_get(&marker_slot(m, 0));
            if let Ok(b) = call_cb(visitor, &test, vec![event.clone()])
                && truthy(&b)
            {
                let found = slot_get(&marker_slot(m, 2));
                if !found.is_null() {
                    complete_future(m, Err(Value::str("Bad state: Too many elements")));
                    cancel_sub(visitor, sub);
                } else {
                    slot_set(&marker_slot(m, 2), event);
                }
            }
        }
        I_DRAIN => {}
        I_LAST | I_SINGLE => {
            let prev = slot_get(&marker_slot(m, 0));
            if kind == I_SINGLE && !prev.is_null() {
                complete_future(m, Err(Value::str("Bad state: Too many elements")));
                cancel_sub(visitor, sub);
            } else {
                slot_set(&marker_slot(m, 0), event);
            }
        }
        I_ITER => {
            let it = marker_extra(m, 0).let_list();
            if let Some(it) = it {
                let mut c = it.borrow_mut();
                if std::env::var("D4RT_DBG").is_ok() {
                    eprintln!(
                        "[dbg] iter data: has_current={} cur={:?} queue={:?}",
                        truthy(&c[3]),
                        c.get(2).cloned(),
                        c.get(7).and_then(as_list).map(|q| q.borrow().len())
                    );
                }
                if c.len() >= 5 {
                    let has_current = truthy(&c[3]);
                    if !has_current {
                        c[2] = event.clone();
                        c[3] = Value::Bool(true);
                    } else if c.len() > 7
                        && let Some(q) = as_list(&c[7])
                    {
                        q.borrow_mut().push(event);
                    }
                    let pending = c.get(4).cloned().unwrap_or(Value::Null);
                    if let Some(f) = async_state::unwrap_future(&pending) {
                        c[4] = Value::Null;
                        drop(c);
                        async_state::complete(&f, Ok(Value::Bool(true)));
                    }
                }
            }
        }
        I_FWD => {
            let target_sub = marker_cell1(m);
            let target_ops = marker_extra(m, 0);
            let target_index = marker_extra(m, 1).as_int().unwrap_or(0) as usize;
            if let Some(ops) = as_list(&target_ops) {
                process_event(visitor, &target_sub, &ops, target_index, event, None);
            }
        }
        I_GEN_FWD if truthy(&marker_extra(m, 1)) => {
            gen_emit(&marker_extra(m, 0), event);
        }
        I_ASFUTURE => {
            slot_set(&marker_slot(m, 0), event);
        }
        _ => {}
    }
}

fn handle_internal_error(
    visitor: &mut crate::interp::InterpreterVisitor,
    sub: &Value,
    m: &Value,
    error: Value,
) {
    let kind = marker_kind(m);
    if kind == I_ITER {
        if let Some(it) = marker_extra(m, 0).let_list() {
            let mut c = it.borrow_mut();
            if c.len() >= 5 {
                c[1] = Value::Bool(true);
                c[5] = error.clone();
                let pending = c.get(4).cloned().unwrap_or(Value::Null);
                if let Some(f) = async_state::unwrap_future(&pending) {
                    c[4] = Value::Null;
                    drop(c);
                    async_state::complete(&f, Err(error));
                }
            }
        }
        return;
    }
    if kind == I_FWD {
        let target_sub = marker_cell1(m);
        let target_ops = marker_extra(m, 0);
        let target_index = marker_extra(m, 1).as_int().unwrap_or(0) as usize;
        if let Some(ops) = as_list(&target_ops) {
            process_error(visitor, &target_sub, &ops, target_index, error, None);
        }
        return;
    }
    if kind == I_GEN_FWD {
        // An error in a `yield*`'d stream re-throws inside the generator
        // body: resume it with the error cached at the yield* point so its
        // replay pass throws (and any try/catch can handle it).
        if let Some(mut cont) = take_yield_star(list_ptr(m).unwrap_or(0)) {
            cont.cache
                .insert((cont.resume_node, cont.resume_iters.clone()), Err(error));
            async_state::enqueue_task(async_state::Task::Resume(cont));
        }
        if let Some(l) = as_list(m) {
            let mut c = l.borrow_mut();
            if c.len() > 3 {
                c[3] = Value::Bool(false);
            }
        }
        return;
    }
    complete_future(m, Err(error));
    cancel_sub(visitor, sub);
}

fn handle_internal_done(visitor: &mut crate::interp::InterpreterVisitor, _sub: &Value, m: &Value) {
    let kind = marker_kind(m);
    match kind {
        I_COLLECT => complete_future(m, Ok(slot_get(&marker_slot(m, 0)))),
        I_FIRST => complete_future(m, Err(Value::str("Bad state: No element"))),
        I_ISEMPTY => complete_future(m, Ok(Value::Bool(true))),
        I_LENGTH => complete_future(m, Ok(slot_get(&marker_slot(m, 0)))),
        I_JOIN => {
            let sep = slot_get(&marker_slot(m, 1));
            let sep = h::as_str(&sep).unwrap_or("");
            let items: Vec<String> = slot_get(&marker_slot(m, 0))
                .let_list()
                .map(|l| {
                    l.borrow()
                        .iter()
                        .filter_map(|v| h::as_str(v).map(|s| s.to_string()))
                        .collect()
                })
                .unwrap_or_default();
            complete_future(m, Ok(Value::str(items.join(sep))));
        }
        I_TOSET => complete_future(m, Ok(slot_get(&marker_slot(m, 0)))),
        I_FOLD => complete_future(m, Ok(slot_get(&marker_slot(m, 0)))),
        I_REDUCE => {
            let acc = slot_get(&marker_slot(m, 1));
            if acc.is_null() {
                complete_future(m, Err(Value::str("Bad state: No element")));
            } else {
                complete_future(m, Ok(acc));
            }
        }
        I_ANY | I_EVERY | I_CONTAINS => {
            complete_future(m, Ok(Value::Bool(kind == I_ANY || kind == I_EVERY)));
        }
        I_FOREACH | I_DRAIN => complete_future(m, Ok(Value::Null)),
        I_ELEMENTAT => {
            complete_future(m, Err(Value::str("Bad state: No element at index")));
        }
        I_FIRSTWHERE => {
            let or_else = slot_get(&marker_slot(m, 1));
            if or_else.is_null() {
                complete_future(m, Err(Value::str("Bad state: No element")));
            } else {
                match call_cb(visitor, &or_else, vec![]) {
                    Ok(v) => complete_future(m, Ok(v)),
                    Err(e) => complete_future(m, Err(async_state::exception_value(e))),
                }
            }
        }
        I_LASTWHERE | I_SINGLEWHERE => {
            let found = slot_get(&marker_slot(m, 2));
            if found.is_null() {
                let or_else = slot_get(&marker_slot(m, 1));
                if or_else.is_null() {
                    complete_future(m, Err(Value::str("Bad state: No element")));
                } else {
                    match call_cb(visitor, &or_else, vec![]) {
                        Ok(v) => complete_future(m, Ok(v)),
                        Err(e) => complete_future(m, Err(async_state::exception_value(e))),
                    }
                }
            } else {
                complete_future(m, Ok(found));
            }
        }
        I_LAST | I_SINGLE => {
            let found = slot_get(&marker_slot(m, 0));
            if found.is_null() {
                complete_future(m, Err(Value::str("Bad state: No element")));
            } else {
                complete_future(m, Ok(found));
            }
        }
        I_ITER => {
            if let Some(it) = marker_extra(m, 0).let_list() {
                let mut c = it.borrow_mut();
                if c.len() >= 5 {
                    c[1] = Value::Bool(true);
                    let pending = c.get(4).cloned().unwrap_or(Value::Null);
                    if let Some(f) = async_state::unwrap_future(&pending) {
                        c[4] = Value::Null;
                        drop(c);
                        async_state::complete(&f, Ok(Value::Bool(false)));
                    }
                }
            }
        }
        I_FWD => {
            // The expanded inner stream completed: resume the source
            // asyncExpand op and drain its queue.
            let sub = marker_cell1(m);
            let src_ops = marker_extra(m, 0);
            let src_index = marker_extra(m, 2).as_int().unwrap_or(0) as usize;
            if let Some(ops) = as_list(&src_ops) {
                let op_cell = ops.borrow().get(src_index).cloned();
                if let Some(cell) = op_cell {
                    if let Some(l) = h::as_list(&cell)
                        && l.borrow().len() > 2
                    {
                        l.borrow_mut()[2] = Value::Bool(false);
                    }
                    let queued: Vec<Value> = h::as_list(&cell)
                        .and_then(|l| l.borrow().get(3).cloned())
                        .and_then(|q| as_list(&q))
                        .map(|q| q.borrow().clone())
                        .unwrap_or_default();
                    if let Some(q) = cell
                        .let_list()
                        .and_then(|l| l.borrow().get(3).cloned())
                        .and_then(|q| as_list(&q))
                    {
                        q.borrow_mut().clear();
                    }
                    for e in queued {
                        process_event(visitor, &sub, &ops, src_index, e, None);
                    }
                }
            }
        }
        I_GEN_FWD => {
            // The `yield*`'d stream completed: resume the generator body
            // at the yield* statement.
            if let Some(cont) = take_yield_star(list_ptr(m).unwrap_or(0)) {
                async_state::enqueue_task(async_state::Task::Resume(cont));
            }
            if let Some(l) = as_list(m) {
                let mut c = l.borrow_mut();
                if c.len() > 3 {
                    c[3] = Value::Bool(false);
                }
            }
        }
        I_ASFUTURE => complete_future(m, Ok(slot_get(&marker_slot(m, 0)))),
        _ => {}
    }
}

// ---------- task handlers (called from async_state) ----------

pub fn handle_stream_event(
    visitor: &mut crate::interp::InterpreterVisitor,
    state: &Value,
    event: &Value,
) {
    if std::env::var("D4RT_DBG").is_ok() {
        let subs = as_list(state)
            .map(|s| state_listeners(&s).borrow().len())
            .unwrap_or(0);
        let cell1 = as_list(state).and_then(|s| s.borrow().get(1).cloned());
        eprintln!(
            "[dbg] handle_stream_event listeners={subs} ptr={:?} cell1_is_list={} cell1_len={}",
            as_list(state).map(|s| Rc::as_ptr(&s)),
            cell1
                .as_ref()
                .and_then(crate::stdlib::stream_lib::as_list_rc)
                .is_some(),
            cell1
                .as_ref()
                .and_then(crate::stdlib::stream_lib::as_list_rc)
                .map(|l| l.borrow().len())
                .unwrap_or(0)
        );
    }
    let state = match as_list(state) {
        Some(s) => s,
        None => return,
    };
    let broadcast = state_is_broadcast(&state);
    let subs: Vec<Value> = state_listeners(&state).borrow().clone();
    for sub in subs {
        if !sub_is_alive(&sub) {
            continue;
        }
        let ops = sub_ops(&sub);
        process_event(visitor, &sub, &ops, 0, event.clone(), None);
        if !broadcast {
            break;
        }
    }
}

pub fn handle_stream_error(
    visitor: &mut crate::interp::InterpreterVisitor,
    state: &Value,
    error: &Value,
) {
    let state = match as_list(state) {
        Some(s) => s,
        None => return,
    };
    let broadcast = state_is_broadcast(&state);
    let subs: Vec<Value> = state_listeners(&state).borrow().clone();
    for sub in subs {
        if !sub_is_alive(&sub) {
            continue;
        }
        let ops = sub_ops(&sub);
        process_error(visitor, &sub, &ops, 0, error.clone(), None);
        if !broadcast {
            break;
        }
    }
}

pub fn handle_stream_done(visitor: &mut crate::interp::InterpreterVisitor, state: &Value) {
    let state = match as_list(state) {
        Some(s) => s,
        None => return,
    };
    {
        let mut s = state.borrow_mut();
        if s.len() > 2 {
            s[2] = Value::Bool(true);
        }
    }
    let broadcast = state_is_broadcast(&state);
    let subs: Vec<Value> = state_listeners(&state).borrow().clone();
    for sub in subs {
        if !sub_is_alive(&sub) {
            continue;
        }
        let ops = sub_ops(&sub);
        process_done(visitor, &sub, &ops, 0, None);
        if !broadcast {
            break;
        }
    }
    // Complete the controller's close/done future.
    if let Some(ctrl) = controller_cells(&state)
        && let Some(done) = ctrl.borrow().get(8).cloned()
        && let Some(f) = async_state::unwrap_future(&done)
    {
        async_state::complete(&f, Ok(Value::Null));
    }
}

/// Schedules the initial timeout checks of a fresh subscription: one
/// `Task::Timeout` per K_TIMEOUT op in the subscription's pipeline.
fn enqueue_timeout_ops(sub: &Value, ops: &Rc<RefCell<Vec<Value>>>) {
    for (i, cell) in ops.borrow().iter().enumerate() {
        if op_kind(cell) == K_TIMEOUT {
            let gen_id = h::as_int(&cell_get(cell, 3)).unwrap_or(0);
            cell_set(cell, 4, Value::Int(async_state::virtual_now()));
            async_state::enqueue_task(async_state::Task::Timeout {
                sub: sub.clone(),
                ops: ops.clone(),
                index: i,
                gen_id,
            });
        }
    }
}

/// Fires a Stream.timeout check (from `Task::Timeout`). Stale checks (the
/// gen no longer matches, or the subscription died) are no-ops. On a real
/// timeout: without `onTimeout` the stream emits a `TimeoutException` and
/// closes; with it, the callback gets an EventSink and the timer restarts.
pub fn handle_stream_timeout(
    visitor: &mut crate::interp::InterpreterVisitor,
    sub: &Value,
    ops: &Rc<RefCell<Vec<Value>>>,
    index: usize,
    gen_id: i64,
) {
    if !sub_is_alive(sub) {
        return;
    }
    let Some(cell) = op_cell_at(ops, index) else {
        return;
    };
    if op_kind(&cell) != K_TIMEOUT {
        return;
    }
    if h::as_int(&cell_get(&cell, 3)) != Some(gen_id) {
        return;
    }
    let duration = h::as_int(&cell_get(&cell, 1)).unwrap_or(0);
    let last = h::as_int(&cell_get(&cell, 4)).unwrap_or(0);
    let now = async_state::virtual_now();
    if duration > 0 && now - last < duration {
        // Let any other pending work (events, delayed completions) run first
        // so it can reset the timer at its proper virtual time. Only when the
        // queue is otherwise idle do we advance the clock toward the deadline.
        if crate::async_state::task_queue_len() > 0 {
            async_state::enqueue_task(async_state::Task::Timeout {
                sub: sub.clone(),
                ops: ops.clone(),
                index,
                gen_id,
            });
        } else {
            async_state::advance_virtual(last + duration - now);
            async_state::enqueue_task(async_state::Task::Timeout {
                sub: sub.clone(),
                ops: ops.clone(),
                index,
                gen_id,
            });
        }
        return;
    }
    let on_timeout = cell_get(&cell, 2);
    if on_timeout.is_null() {
        let err = wrap(
            "TimeoutException",
            Value::List(Rc::new(RefCell::new(vec![
                Value::str("No stream event within time limit"),
                Value::Int(duration),
            ]))),
        );
        process_error(visitor, sub, ops, index + 1, err, None);
        process_done(visitor, sub, ops, index + 1, None);
        cancel_sub(visitor, sub);
    } else {
        let sink = event_sink(sub, ops, index + 1);
        let _ = call_cb(visitor, &on_timeout, vec![sink]);
        let gen_id = h::as_int(&cell_get(&cell, 3)).unwrap_or(0);
        cell_set(&cell, 4, Value::Int(async_state::virtual_now()));
        async_state::enqueue_task(async_state::Task::Timeout {
            sub: sub.clone(),
            ops: ops.clone(),
            index,
            gen_id,
        });
    }
}

pub fn handle_stream_tick(visitor: &mut crate::interp::InterpreterVisitor, state: &Value) {
    let state = match as_list(state) {
        Some(s) => s,
        None => return,
    };
    let cb = state.borrow().get(5).cloned().unwrap_or(Value::Null);
    if cb.is_null() {
        return;
    }
    let counter = state
        .borrow()
        .get(6)
        .cloned()
        .and_then(|c| h::as_int(&c))
        .unwrap_or(0);
    state.borrow_mut()[6] = Value::Int(counter + 1);
    let r = call_cb(visitor, &cb, vec![Value::Int(counter)]).unwrap_or(Value::Null);
    let has_listener = state_listeners(&state).borrow().iter().any(sub_is_alive);
    if has_listener {
        async_state::enqueue_task(async_state::Task::StreamEvent {
            state: Value::List(state.clone()),
            event: r,
        });
    } else {
        state_events(&state).borrow_mut().push(r);
        state.borrow_mut()[7] = Value::Bool(false);
        return;
    }
    if state_listeners(&state).borrow().iter().any(sub_is_alive) {
        async_state::enqueue_task(async_state::Task::StreamTick {
            state: Value::List(state.clone()),
        });
    } else {
        state.borrow_mut()[7] = Value::Bool(false);
    }
}

pub fn handle_stream_event_sub(
    visitor: &mut crate::interp::InterpreterVisitor,
    sub: &Value,
    event: &Value,
) {
    deliver_event_to_listener(visitor, sub, event.clone());
}

pub fn handle_stream_op_resume(
    visitor: &mut crate::interp::InterpreterVisitor,
    sub: &Value,
    ops: &Rc<RefCell<Vec<Value>>>,
    index: usize,
    rest_ops: &Rc<RefCell<Vec<Value>>>,
    rest_index: usize,
    result: Result<Value, Value>,
) {
    let cell = ops.borrow().get(index).cloned();
    let cell = match cell {
        Some(c) => c,
        None => return,
    };
    let kind = op_kind(&cell);
    if let Some(l) = cell.let_list()
        && l.borrow().len() > 2
    {
        l.borrow_mut()[2] = Value::Bool(false);
    }
    let rest = if rest_index == usize::MAX {
        None
    } else {
        Some((rest_ops.clone(), rest_index))
    };
    if kind == K_HANDLE_ERROR {
        drain_handle_error_queue(visitor, sub, ops, index, rest);
        return;
    }
    match result {
        Ok(v) => match kind {
            K_ASYNC_EXPAND => {
                let mut subscribed = false;
                if let Some(s) = as_list(&v)
                    && s.borrow().len() == 2
                {
                    let s_state = s.borrow()[0].clone();
                    let s_ops = s.borrow()[1].clone();
                    if let (Some(s_state), Some(s_ops)) = (as_list(&s_state), as_list(&s_ops)) {
                        let fwd = Value::List(Rc::new(RefCell::new(vec![
                            Value::Int(I_FWD),
                            sub.clone(),
                            Value::List(ops.clone()),
                            Value::Int(index as i64 + 1),
                            Value::Int(index as i64),
                        ])));
                        make_sub(visitor, &s_state, &s_ops, fwd.clone(), fwd.clone(), fwd);
                        subscribed = true;
                    }
                }
                if !subscribed {
                    process_event(visitor, sub, ops, index + 1, v, rest.clone());
                }
            }
            _ => process_event(visitor, sub, ops, index + 1, v, rest.clone()),
        },
        Err(e) => process_error(visitor, sub, ops, index + 1, e, rest.clone()),
    }
    // Drain queued events for async ops.
    if let Some(c) = ops.borrow().get(index).cloned() {
        let cl = c.clone();
        if let Some(l) = cl.let_list() {
            let queued_opt = l.borrow().get(3).cloned();
            if let Some(q) = queued_opt.and_then(|v| as_list(&v)) {
                let queued = q.borrow().clone();
                q.borrow_mut().clear();
                for e in queued {
                    process_event(visitor, sub, ops, index, e, rest.clone());
                }
            }
            // Flush a held-done once the async op has drained its queue.
            if matches!(kind, K_ASYNC_MAP | K_ASYNC_EXPAND)
                && !as_bool(&cell_get(&c, 2))
                && as_bool(&cell_get(&c, 4))
            {
                cell_set(&c, 4, Value::Bool(false));
                process_done(visitor, sub, ops, index + 1, rest);
            }
        }
    }
}

pub fn handle_stream_emit(
    _visitor: &mut crate::interp::InterpreterVisitor,
    state: &Value,
    result: Result<Value, Value>,
) {
    let state = match as_list(state) {
        Some(s) => s,
        None => return,
    };
    let has_listener = state_listeners(&state).borrow().iter().any(sub_is_alive);
    match result {
        Ok(v) => {
            if has_listener {
                async_state::enqueue_task(async_state::Task::StreamEvent {
                    state: Value::List(state.clone()),
                    event: v,
                });
            } else {
                state_events(&state).borrow_mut().push(v);
            }
        }
        Err(e) => {
            if has_listener {
                async_state::enqueue_task(async_state::Task::StreamError {
                    state: Value::List(state.clone()),
                    error: e,
                });
            } else {
                state_events(&state)
                    .borrow_mut()
                    .push(Value::List(Rc::new(RefCell::new(vec![
                        Value::Bool(true),
                        e,
                    ]))));
            }
        }
    }
    if has_listener {
        async_state::enqueue_task(async_state::Task::StreamDone {
            state: Value::List(state.clone()),
        });
    } else {
        state.borrow_mut()[2] = Value::Bool(true);
    }
}

// ---------- Stream ----------

/// Resolves a `StreamTransformer` against a base stream eagerly:
/// - a fromHandlers transformer appends a K_HANDLER op to the pipeline;
/// - a fromBind transformer binds the base stream and returns the bound
///   stream;
/// - a wrapped (cast) transformer recurses into its inner transformer.
fn resolve_transform(
    visitor: &mut crate::interp::InterpreterVisitor,
    transformer: &Value,
    state: &Rc<RefCell<Vec<Value>>>,
    ops: &Rc<RefCell<Vec<Value>>>,
) -> Value {
    let native = match as_list(transformer) {
        Some(l) => l,
        None => return new_stream(state.clone(), ops.borrow().clone()),
    };
    let (inner, bind_cb, handlers) = {
        let b = native.borrow();
        (
            b.first().cloned().unwrap_or(Value::Null),
            b.get(1).cloned().unwrap_or(Value::Null),
            b.get(2).cloned().unwrap_or(Value::Null),
        )
    };
    if let Some(inner_list) = as_list(&inner)
        && !inner_list.borrow().is_empty()
    {
        return resolve_transform(visitor, &inner, state, ops);
    }
    if !bind_cb.is_null() {
        let base = new_stream(state.clone(), ops.borrow().clone());
        return match call_cb(visitor, &bind_cb, vec![base]) {
            Ok(r) => r,
            Err(_) => new_stream(state.clone(), ops.borrow().clone()),
        };
    }
    if let Some(handlers) = as_list(&handlers) {
        let (hd, he, hd_done) = {
            let b = handlers.borrow();
            (
                b.first().cloned().unwrap_or(Value::Null),
                b.get(1).cloned().unwrap_or(Value::Null),
                b.get(2).cloned().unwrap_or(Value::Null),
            )
        };
        let mut cells = ops.borrow().clone();
        cells.push(Value::List(Rc::new(RefCell::new(vec![
            Value::Int(K_HANDLER),
            hd,
            he,
            hd_done,
        ]))));
        return new_stream(state.clone(), cells);
    }
    new_stream(state.clone(), ops.borrow().clone())
}

fn stream_definition() -> BridgedClassDef {
    let mut bc = BridgedClassDef::new("Stream");

    bc.static_methods.insert(
        "value".to_string(),
        h::static_method(|_v, args, _n| {
            let state = new_state();
            state_events(&state)
                .borrow_mut()
                .push(args.first().cloned().unwrap_or(Value::Null));
            state.borrow_mut()[2] = Value::Bool(true);
            Ok(new_stream(state, Vec::new()))
        }),
    );
    bc.static_methods.insert(
        "error".to_string(),
        h::static_method(|_v, args, _n| {
            let state = new_state();
            state_events(&state)
                .borrow_mut()
                .push(Value::List(Rc::new(RefCell::new(vec![
                    Value::Bool(true),
                    args.first().cloned().unwrap_or(Value::Null),
                ]))));
            state.borrow_mut()[2] = Value::Bool(true);
            Ok(new_stream(state, Vec::new()))
        }),
    );
    bc.static_methods.insert(
        "empty".to_string(),
        h::static_method(|_v, _args, _n| {
            let state = new_state();
            state.borrow_mut()[2] = Value::Bool(true);
            Ok(new_stream(state, Vec::new()))
        }),
    );
    bc.static_methods.insert(
        "fromIterable".to_string(),
        h::static_method(|_v, args, _n| {
            let items = args.first().cloned().unwrap_or(Value::Null);
            if let Some(l) = as_list(&items) {
                Ok(stream_from_iterable(l.borrow().clone()))
            } else {
                Ok(stream_from_iterable(Vec::new()))
            }
        }),
    );
    bc.static_methods.insert(
        "fromFuture".to_string(),
        h::static_method(|_v, args, _n| {
            let fut = args.first().cloned().unwrap_or(Value::Null);
            Ok(stream_from_future(&fut))
        }),
    );
    bc.static_methods.insert(
        "fromFutures".to_string(),
        h::static_method(|_v, args, _n| {
            let state = new_state();
            let futures = args.first().cloned().unwrap_or(Value::Null);
            if let Some(l) = as_list(&futures) {
                let items: Vec<Value> = l.borrow().clone();
                for f in items {
                    attach_stream_emit(&f, &state);
                }
            }
            Ok(new_stream(state, Vec::new()))
        }),
    );
    bc.static_methods.insert(
        "periodic".to_string(),
        h::static_method(|_v, args, _n| {
            let state = new_state();
            state.borrow_mut()[5] = args.get(1).cloned().unwrap_or(Value::Null);
            state.borrow_mut()[7] = Value::Bool(true);
            async_state::enqueue_task(async_state::Task::StreamTick {
                state: Value::List(state.clone()),
            });
            Ok(new_stream(state, Vec::new()))
        }),
    );
    bc.static_methods.insert(
        "castFrom".to_string(),
        h::static_method(|_v, args, _n| Ok(args.first().cloned().unwrap_or(Value::Null))),
    );

    bc.methods.insert(
        "listen".to_string(),
        h::method(|visitor, recv, args, named| stream_listen_value(visitor, &recv, &args, &named)),
    );
    bc.methods.insert(
        "timeout".to_string(),
        h::method(|_v, recv, args, named| {
            let state = stream_state(&recv);
            let mut ops = stream_ops(&recv).borrow().clone();
            let duration = args
                .first()
                .cloned()
                .and_then(|a| h::as_int(&a))
                .unwrap_or(0);
            let on_timeout = named.get("onTimeout").cloned().unwrap_or(Value::Null);
            ops.push(Value::List(Rc::new(RefCell::new(vec![
                Value::Int(K_TIMEOUT),
                Value::Int(duration),
                on_timeout,
                Value::Int(0), // gen
                Value::Int(0), // last reset (virtual time)
            ]))));
            Ok(new_stream(state, ops))
        }),
    );
    bc.methods.insert(
        "asFuture".to_string(),
        h::method(|_v, recv, args, _n| {
            let state = stream_state(&recv);
            let ops = stream_ops(&recv);
            let fut = Rc::new(RefCell::new(async_state::FutureState::new()));
            let seed = args.first().cloned().unwrap_or(Value::Null);
            let m = marker(
                I_ASFUTURE,
                fut.clone(),
                vec![Value::List(Rc::new(RefCell::new(vec![seed])))],
            );
            make_sub_borrow(&state, &ops, m.clone(), m.clone(), m);
            Ok(wrap_future(fut))
        }),
    );
    bc.methods.insert(
        "pipe".to_string(),
        h::method(|visitor, recv, args, named| {
            let state = stream_state(&recv);
            let ops = stream_ops(&recv);
            let consumer = args.first().cloned().unwrap_or(Value::Null);
            let out = Rc::new(RefCell::new(async_state::FutureState::new()));
            // A StreamController or its sink exposes the shared controller
            // state directly as its native; forward events into it.
            if let Some(cs) = as_list(&consumer) {
                let add_cs = cs.clone();
                let on_data = Value::NativeFn(Rc::new(NativeFunction {
                    name: Some("<Stream.pipe add>".to_string()),
                    min_arity: 1,
                    max_arity: Some(1),
                    is_async: false,
                    is_generator: false,
                    closure: Box::new(move |visitor, args, _n| {
                        controller_add(
                            visitor,
                            &add_cs,
                            args.first().cloned().unwrap_or(Value::Null),
                        );
                        Ok(Value::Null)
                    }),
                    callable_runtime_type: FunctionRuntimeType::untyped(),
                }));
                let err_cs = cs.clone();
                let on_error = Value::NativeFn(Rc::new(NativeFunction {
                    name: Some("<Stream.pipe addError>".to_string()),
                    min_arity: 1,
                    max_arity: Some(1),
                    is_async: false,
                    is_generator: false,
                    closure: Box::new(move |visitor, args, _n| {
                        controller_add_error(
                            visitor,
                            &err_cs,
                            args.first().cloned().unwrap_or(Value::Null),
                        );
                        Ok(Value::Null)
                    }),
                    callable_runtime_type: FunctionRuntimeType::untyped(),
                }));
                let done_cs = cs.clone();
                let done_out = out.clone();
                let on_done = Value::NativeFn(Rc::new(NativeFunction {
                    name: Some("<Stream.pipe close>".to_string()),
                    min_arity: 0,
                    max_arity: Some(0),
                    is_async: false,
                    is_generator: false,
                    closure: Box::new(move |visitor, _args, _n| {
                        let done = controller_close(visitor, &done_cs)?;
                        async_state::chain_into(&done, &done_out);
                        Ok(Value::Null)
                    }),
                    callable_runtime_type: FunctionRuntimeType::untyped(),
                }));
                make_sub(visitor, &state, &ops, on_data, on_error, on_done);
            } else {
                async_state::complete(&out, Ok(Value::Null));
            }
            let _ = named;
            Ok(wrap_future(out))
        }),
    );
    bc.methods.insert(
        "transform".to_string(),
        h::method(|visitor, recv, args, _n| {
            let state = stream_state(&recv);
            let ops = stream_ops(&recv);
            let transformer = args.first().cloned().unwrap_or(Value::Null);
            Ok(resolve_transform(visitor, &transformer, &state, &ops))
        }),
    );
    bc.methods.insert(
        "handleError".to_string(),
        h::method(|_v, recv, args, named| {
            let state = stream_state(&recv);
            let mut ops = stream_ops(&recv).borrow().clone();
            let on_error = args.first().cloned().unwrap_or(Value::Null);
            let test = named.get("test").cloned().unwrap_or(Value::Null);
            ops.push(Value::List(Rc::new(RefCell::new(vec![
                Value::Int(K_HANDLE_ERROR),
                on_error,
                test,
                Value::Bool(false), // busy
                Value::list(),      // error queue
            ]))));
            Ok(new_stream(state, ops))
        }),
    );
    bc.methods.insert(
        "map".to_string(),
        h::method(|_v, recv, args, _n| push_unary_op(recv, args, K_MAP)),
    );
    bc.methods.insert(
        "where".to_string(),
        h::method(|_v, recv, args, _n| push_unary_op(recv, args, K_WHERE)),
    );
    bc.methods.insert(
        "expand".to_string(),
        h::method(|_v, recv, args, _n| push_unary_op(recv, args, K_EXPAND)),
    );
    bc.methods.insert(
        "asyncMap".to_string(),
        h::method(|_v, recv, args, _n| {
            let state = stream_state(&recv);
            let mut ops = stream_ops(&recv).borrow().clone();
            ops.push(Value::List(Rc::new(RefCell::new(vec![
                Value::Int(K_ASYNC_MAP),
                args.first().cloned().unwrap_or(Value::Null),
                Value::Bool(false), // busy
                Value::list(),      // queue
                Value::Bool(false), // held done
            ]))));
            Ok(new_stream(state, ops))
        }),
    );
    bc.methods.insert(
        "asyncExpand".to_string(),
        h::method(|_v, recv, args, _n| {
            let state = stream_state(&recv);
            let mut ops = stream_ops(&recv).borrow().clone();
            ops.push(Value::List(Rc::new(RefCell::new(vec![
                Value::Int(K_ASYNC_EXPAND),
                args.first().cloned().unwrap_or(Value::Null),
                Value::Bool(false), // busy
                Value::list(),      // queue
                Value::Bool(false), // held done
            ]))));
            Ok(new_stream(state, ops))
        }),
    );
    bc.methods.insert(
        "take".to_string(),
        h::method(|_v, recv, args, _n| {
            let state = stream_state(&recv);
            let mut ops = stream_ops(&recv).borrow().clone();
            let n = args
                .first()
                .cloned()
                .and_then(|a| h::as_int(&a))
                .unwrap_or(0);
            ops.push(Value::List(Rc::new(RefCell::new(vec![
                Value::Int(K_TAKE),
                Value::Int(n),
            ]))));
            Ok(new_stream(state, ops))
        }),
    );
    bc.methods.insert(
        "skip".to_string(),
        h::method(|_v, recv, args, _n| {
            let state = stream_state(&recv);
            let mut ops = stream_ops(&recv).borrow().clone();
            let n = args
                .first()
                .cloned()
                .and_then(|a| h::as_int(&a))
                .unwrap_or(0);
            ops.push(Value::List(Rc::new(RefCell::new(vec![
                Value::Int(K_SKIP),
                Value::Int(n),
            ]))));
            Ok(new_stream(state, ops))
        }),
    );
    bc.methods.insert(
        "takeWhile".to_string(),
        h::method(|_v, recv, args, _n| push_unary_op(recv, args, K_TAKE_WHILE)),
    );
    bc.methods.insert(
        "skipWhile".to_string(),
        h::method(|_v, recv, args, _n| {
            let state = stream_state(&recv);
            let mut ops = stream_ops(&recv).borrow().clone();
            ops.push(Value::List(Rc::new(RefCell::new(vec![
                Value::Int(K_SKIP_WHILE),
                Value::Bool(true),
                args.first().cloned().unwrap_or(Value::Null),
            ]))));
            Ok(new_stream(state, ops))
        }),
    );
    bc.methods.insert(
        "distinct".to_string(),
        h::method(|_v, recv, args, _n| {
            let state = stream_state(&recv);
            let mut ops = stream_ops(&recv).borrow().clone();
            ops.push(Value::List(Rc::new(RefCell::new(vec![
                Value::Int(K_DISTINCT),
                Value::Null, // previous emitted
                args.first().cloned().unwrap_or(Value::Null),
            ]))));
            Ok(new_stream(state, ops))
        }),
    );
    bc.methods.insert(
        "cast".to_string(),
        h::method(|_v, recv, _args, _n| Ok(recv.clone())),
    );
    bc.methods.insert(
        "asBroadcastStream".to_string(),
        h::method(|_v, recv, _args, _n| {
            let state = stream_state(&recv);
            state.borrow_mut()[4] = Value::Bool(true);
            Ok(recv.clone())
        }),
    );

    bc.getters.insert(
        "isBroadcast".to_string(),
        h::getter(|_v, native| {
            let state = stream_state(&native);
            Ok(Value::Bool(state_is_broadcast(&state)))
        }),
    );
    bc.getters.insert(
        "iterator".to_string(),
        h::getter(|_v, native| {
            let state = stream_state(&native);
            let ops = stream_ops(&native);
            Ok(wrap(
                "StreamIterator",
                Value::List(Rc::new(RefCell::new(vec![
                    Value::List(state),
                    Value::List(ops),
                    Value::List(Rc::new(RefCell::new(vec![
                        Value::Bool(false), // started
                        Value::Bool(false), // done
                        Value::Null,        // current
                        Value::Bool(false), // has current
                        Value::Null,        // pending
                        Value::Null,        // error
                    ]))),
                ]))),
            ))
        }),
    );
    bc.getters.insert(
        "first".to_string(),
        h::getter(|_v, recv| future_op(recv, I_FIRST, vec![], Value::Null)),
    );
    bc.getters.insert(
        "last".to_string(),
        h::getter(|_v, recv| future_op(recv, I_LAST, vec![], Value::Null)),
    );
    bc.getters.insert(
        "single".to_string(),
        h::getter(|_v, recv| future_op(recv, I_SINGLE, vec![], Value::Null)),
    );
    bc.getters.insert(
        "isEmpty".to_string(),
        h::getter(|_v, recv| future_op(recv, I_ISEMPTY, vec![], Value::Null)),
    );
    bc.getters.insert(
        "length".to_string(),
        h::getter(|_v, recv| future_op(recv, I_LENGTH, vec![], Value::Null)),
    );

    for (name, kind) in [
        ("toList", I_COLLECT),
        ("toSet", I_TOSET),
        ("join", I_JOIN),
        ("drain", I_DRAIN),
        ("firstWhere", I_FIRSTWHERE),
        ("lastWhere", I_LASTWHERE),
        ("singleWhere", I_SINGLEWHERE),
        ("elementAt", I_ELEMENTAT),
        ("any", I_ANY),
        ("every", I_EVERY),
        ("contains", I_CONTAINS),
        ("fold", I_FOLD),
        ("reduce", I_REDUCE),
        ("forEach", I_FOREACH),
    ] {
        bc.methods.insert(
            name.to_string(),
            h::method(move |_v, recv, args, named| {
                let or_else =
                    if kind == I_FIRSTWHERE || kind == I_LASTWHERE || kind == I_SINGLEWHERE {
                        named.get("orElse").cloned().unwrap_or(Value::Null)
                    } else {
                        Value::Null
                    };
                future_op(recv, kind, args, or_else)
            }),
        );
    }
    bc
}

fn attach_stream_emit(fut: &Value, state: &Rc<RefCell<Vec<Value>>>) {
    if let Some(f) = async_state::unwrap_future(fut) {
        let mut st = f.borrow_mut();
        if st.completed {
            let result = async_state::future_result(&st);
            drop(st);
            async_state::enqueue_task(async_state::Task::StreamEmit {
                state: Value::List(state.clone()),
                result,
            });
        } else {
            st.continuations
                .push(async_state::Continuation::StreamEmit {
                    state: Value::List(state.clone()),
                });
        }
    }
}

/// Builds a single-subscription stream that emits `fut`'s result (value or
/// error) and then closes. Backs both `Stream.fromFuture` and
/// `Future.asStream`.
pub fn stream_from_future(fut: &Value) -> Value {
    let state = new_state();
    attach_stream_emit(fut, &state);
    new_stream(state, Vec::new())
}

/// Builds a single-subscription stream preloaded with `items` (closed).
/// Backs `Stream.fromIterable` and native producers like `Directory.list`.
pub fn stream_from_iterable(items: Vec<Value>) -> Value {
    let state = new_state();
    state_events(&state).borrow_mut().extend(items);
    state.borrow_mut()[2] = Value::Bool(true);
    new_stream(state, Vec::new())
}

/// Creates a StreamController (single-subscription unless `broadcast`) and
/// returns its bridged controller value. Shared by `dart:io`/`dart:isolate`
/// native code that needs an event source.
pub fn make_controller_value(sync: bool, broadcast: bool) -> Value {
    make_controller(sync, broadcast)
}

/// The shared stream state behind a StreamController value (its native).
pub fn controller_state_of(v: &Value) -> Rc<RefCell<Vec<Value>>> {
    controller_state(v)
}

/// The Stream exposed by a StreamController value.
pub fn controller_stream_value(v: &Value) -> Value {
    new_stream(controller_state(v), Vec::new())
}

/// StreamController onCancel hook setter (cell index 7).
pub fn controller_set_on_cancel(v: &Value, callback: Value) {
    if let Some(cells) = controller_cells(&controller_state(v))
        && cells.borrow().len() > 7
    {
        cells.borrow_mut()[7] = callback;
    }
}

/// StreamController.add on a controller value.
pub fn controller_add_value(
    visitor: &mut crate::interp::InterpreterVisitor,
    v: &Value,
    value: Value,
) {
    controller_add(visitor, &controller_state(v), value);
}

/// StreamController.addError on a controller value.
pub fn controller_add_error_value(
    visitor: &mut crate::interp::InterpreterVisitor,
    v: &Value,
    error: Value,
) {
    controller_add_error(visitor, &controller_state(v), error);
}

/// StreamController.close on a controller value.
pub fn controller_close_value(
    visitor: &mut crate::interp::InterpreterVisitor,
    v: &Value,
) -> Result<Value, InterpError> {
    controller_close(visitor, &controller_state(v))
}

/// Stream.listen on a stream value.
pub fn stream_listen_value(
    visitor: &mut crate::interp::InterpreterVisitor,
    recv: &Value,
    args: &[Value],
    named: &HashMap<String, Value>,
) -> Result<Value, InterpError> {
    let state = stream_state(recv);
    let ops = stream_ops(recv);
    let on_data = args.first().cloned().unwrap_or(Value::Null);
    let on_error = named
        .get("onError")
        .cloned()
        .or_else(|| args.get(1).cloned())
        .unwrap_or(Value::Null);
    let on_done = named
        .get("onDone")
        .cloned()
        .or_else(|| args.get(2).cloned())
        .unwrap_or(Value::Null);
    let cancel_on_error = named.get("cancelOnError").map(as_bool).unwrap_or(false);
    let sub = make_sub(visitor, &state, &ops, on_data, on_error, on_done);
    if cancel_on_error && let Some(l) = as_list(&sub) {
        let mut c = l.borrow_mut();
        if c.len() > 8 {
            c[8] = Value::Bool(true);
        }
    }
    Ok(sub)
}

/// Stream.transform on a stream value.
pub fn stream_transform_value(
    visitor: &mut crate::interp::InterpreterVisitor,
    recv: &Value,
    args: &[Value],
) -> Result<Value, InterpError> {
    let state = stream_state(recv);
    let ops = stream_ops(recv);
    let transformer = args.first().cloned().unwrap_or(Value::Null);
    Ok(resolve_transform(visitor, &transformer, &state, &ops))
}

/// Subscribes Rust-side data/error/done handlers to a stream value.
pub fn stream_subscribe_value(
    visitor: &mut crate::interp::InterpreterVisitor,
    recv: &Value,
    on_data: Value,
    on_error: Value,
    on_done: Value,
) {
    let state = stream_state(recv);
    let ops = stream_ops(recv);
    make_sub(visitor, &state, &ops, on_data, on_error, on_done);
}

// Public stream op markers for delegating stream classes (e.g. dart:io
// sockets/responses) to the shared stream machinery.
pub const OP_MAP: i64 = K_MAP;
pub const OP_WHERE: i64 = K_WHERE;
pub const OP_EXPAND: i64 = K_EXPAND;
pub const OP_TAKE: i64 = K_TAKE;
pub const OP_SKIP: i64 = K_SKIP;
pub const OP_TAKE_WHILE: i64 = K_TAKE_WHILE;
pub const OP_SKIP_WHILE: i64 = K_SKIP_WHILE;
pub const OP_DISTINCT: i64 = K_DISTINCT;
pub const OP_ASYNC_MAP: i64 = K_ASYNC_MAP;
pub const OP_ASYNC_EXPAND: i64 = K_ASYNC_EXPAND;
pub const FUTURE_OP_COLLECT: i64 = I_COLLECT;
pub const FUTURE_OP_FIRST: i64 = I_FIRST;
pub const FUTURE_OP_ISEMPTY: i64 = I_ISEMPTY;
pub const FUTURE_OP_LENGTH: i64 = I_LENGTH;
pub const FUTURE_OP_JOIN: i64 = I_JOIN;
pub const FUTURE_OP_TOSET: i64 = I_TOSET;
pub const FUTURE_OP_FOLD: i64 = I_FOLD;
pub const FUTURE_OP_REDUCE: i64 = I_REDUCE;
pub const FUTURE_OP_ANY: i64 = I_ANY;
pub const FUTURE_OP_EVERY: i64 = I_EVERY;
pub const FUTURE_OP_CONTAINS: i64 = I_CONTAINS;
pub const FUTURE_OP_FOREACH: i64 = I_FOREACH;
pub const FUTURE_OP_ELEMENTAT: i64 = I_ELEMENTAT;
pub const FUTURE_OP_FIRSTWHERE: i64 = I_FIRSTWHERE;
pub const FUTURE_OP_LASTWHERE: i64 = I_LASTWHERE;
pub const FUTURE_OP_SINGLEWHERE: i64 = I_SINGLEWHERE;
pub const FUTURE_OP_LAST: i64 = I_LAST;
pub const FUTURE_OP_SINGLE: i64 = I_SINGLE;
pub const FUTURE_OP_DRAIN: i64 = I_DRAIN;

/// Applies a unary stream op (map/where/expand/take/skip/distinct/...) and
/// returns the derived stream.
pub fn stream_unary_op_value(
    recv: &Value,
    args: &[Value],
    kind: i64,
) -> Result<Value, InterpError> {
    push_unary_op(recv.clone(), args.to_vec(), kind)
}

/// Stream.toList/join/fold/... on a stream value (kind constants are
/// internal to this module; callers pass the marker via `future_op`).
pub fn stream_future_op(
    recv: &Value,
    kind: i64,
    args: &[Value],
    or_else: Value,
) -> Result<Value, InterpError> {
    future_op(recv.clone(), kind, args.to_vec(), or_else)
}

/// Stream.first/last/single/isEmpty/length getters on a stream value.
pub fn stream_future_op_getter(recv: &Value, kind: i64) -> Result<Value, InterpError> {
    future_op(recv.clone(), kind, Vec::new(), Value::Null)
}

/// Registers Stream-mixin methods on a bridged class whose native value
/// holds the stream at `stream_index` (used by ReceivePort, ServerSocket,
/// Socket, ...).
pub fn delegate_stream(bc: &mut BridgedClassDef, stream_index: usize) {
    let list_get = |v: &Value, i: usize| h::as_list(v).and_then(|l| l.borrow().get(i).cloned());
    bc.methods.insert(
        "listen".to_string(),
        h::method(move |visitor, recv, args, named| {
            let stream = list_get(&recv, stream_index).unwrap_or(Value::Null);
            stream_listen_value(visitor, &stream, &args, &named)
        }),
    );
    bc.methods.insert(
        "transform".to_string(),
        h::method(move |visitor, recv, args, _n| {
            let stream = list_get(&recv, stream_index).unwrap_or(Value::Null);
            stream_transform_value(visitor, &stream, &args)
        }),
    );
    for (name, kind) in [
        ("toList", FUTURE_OP_COLLECT),
        ("join", FUTURE_OP_JOIN),
        ("drain", FUTURE_OP_DRAIN),
        ("toSet", FUTURE_OP_TOSET),
        ("fold", FUTURE_OP_FOLD),
        ("reduce", FUTURE_OP_REDUCE),
        ("forEach", FUTURE_OP_FOREACH),
        ("any", FUTURE_OP_ANY),
        ("every", FUTURE_OP_EVERY),
        ("contains", FUTURE_OP_CONTAINS),
        ("elementAt", FUTURE_OP_ELEMENTAT),
    ] {
        bc.methods.insert(
            name.to_string(),
            h::method(move |_v, recv, args, _n| {
                let stream = list_get(&recv, stream_index).unwrap_or(Value::Null);
                stream_future_op(&stream, kind, &args, Value::Null)
            }),
        );
    }
    for (name, kind) in [
        ("firstWhere", FUTURE_OP_FIRSTWHERE),
        ("lastWhere", FUTURE_OP_LASTWHERE),
        ("singleWhere", FUTURE_OP_SINGLEWHERE),
    ] {
        bc.methods.insert(
            name.to_string(),
            h::method(move |_v, recv, args, _n| {
                let stream = list_get(&recv, stream_index).unwrap_or(Value::Null);
                let or_else = args.get(1).cloned().unwrap_or(Value::Null);
                stream_future_op(&stream, kind, &args, or_else)
            }),
        );
    }
    for (name, kind) in [
        ("map", OP_MAP),
        ("where", OP_WHERE),
        ("expand", OP_EXPAND),
        ("take", OP_TAKE),
        ("skip", OP_SKIP),
        ("takeWhile", OP_TAKE_WHILE),
        ("skipWhile", OP_SKIP_WHILE),
        ("distinct", OP_DISTINCT),
    ] {
        bc.methods.insert(
            name.to_string(),
            h::method(move |_v, recv, args, _n| {
                let stream = list_get(&recv, stream_index).unwrap_or(Value::Null);
                stream_unary_op_value(&stream, &args, kind)
            }),
        );
    }
    for (name, kind) in [
        ("first", FUTURE_OP_FIRST),
        ("last", FUTURE_OP_LAST),
        ("single", FUTURE_OP_SINGLE),
        ("isEmpty", FUTURE_OP_ISEMPTY),
        ("length", FUTURE_OP_LENGTH),
    ] {
        bc.getters.insert(
            name.to_string(),
            h::getter(move |_v, recv| {
                let stream = list_get(&recv, stream_index).unwrap_or(Value::Null);
                stream_future_op_getter(&stream, kind)
            }),
        );
    }
}

/// Pushes a data event into an EventSink (native `[sub, ops, index]`).
/// Backs converter `bindStream` style handlers (e.g. `utf8.decoder`).
pub fn sink_add_value(visitor: &mut crate::interp::InterpreterVisitor, sink: &Value, value: Value) {
    if let Some(l) = as_list(sink) {
        let (sub, ops, index) = {
            let c = l.borrow();
            (
                c.first().cloned(),
                c.get(1).cloned().and_then(|o| as_list_rc(&o)),
                c.get(2).cloned().and_then(|i| h::as_int(&i)),
            )
        };
        if let (Some(sub), Some(ops), Some(index)) = (sub, ops, index) {
            process_event(visitor, &sub, &ops, index as usize, value, None);
        }
    }
}

/// Pushes an error into an EventSink.
pub fn sink_add_error_value(
    visitor: &mut crate::interp::InterpreterVisitor,
    sink: &Value,
    error: Value,
) {
    if let Some(l) = as_list(sink) {
        let (sub, ops, index) = {
            let c = l.borrow();
            (
                c.first().cloned(),
                c.get(1).cloned().and_then(|o| as_list_rc(&o)),
                c.get(2).cloned().and_then(|i| h::as_int(&i)),
            )
        };
        if let (Some(sub), Some(ops), Some(index)) = (sub, ops, index) {
            process_error(visitor, &sub, &ops, index as usize, error, None);
        }
    }
}

/// Closes an EventSink.
pub fn sink_close_value(visitor: &mut crate::interp::InterpreterVisitor, sink: &Value) {
    if let Some(l) = as_list(sink) {
        let (sub, ops, index) = {
            let c = l.borrow();
            (
                c.first().cloned(),
                c.get(1).cloned().and_then(|o| as_list_rc(&o)),
                c.get(2).cloned().and_then(|i| h::as_int(&i)),
            )
        };
        if let (Some(sub), Some(ops), Some(index)) = (sub, ops, index) {
            process_done(visitor, &sub, &ops, index as usize, None);
        }
    }
}

fn push_unary_op(recv: Value, args: Vec<Value>, kind: i64) -> Result<Value, InterpError> {
    let state = stream_state(&recv);
    let mut ops = stream_ops(&recv).borrow().clone();
    ops.push(Value::List(Rc::new(RefCell::new(vec![
        Value::Int(kind),
        args.first().cloned().unwrap_or(Value::Null),
    ]))));
    Ok(new_stream(state, ops))
}

fn future_op(
    recv: Value,
    kind: i64,
    args: Vec<Value>,
    or_else: Value,
) -> Result<Value, InterpError> {
    let state = stream_state(&recv);
    let ops = stream_ops(&recv);
    let fut = Rc::new(RefCell::new(async_state::FutureState::new()));
    let mut extras: Vec<Value> = Vec::new();
    match kind {
        I_COLLECT | I_TOSET => {
            extras.push(Value::list());
        }
        I_JOIN => {
            extras.push(Value::list());
            extras.push(args.first().cloned().unwrap_or(Value::str("")));
        }
        I_FOLD => {
            extras.push(args.first().cloned().unwrap_or(Value::Null)); // acc
            extras.push(args.get(1).cloned().unwrap_or(Value::Null)); // closure
        }
        I_REDUCE => {
            extras.push(args.first().cloned().unwrap_or(Value::Null)); // closure
            extras.push(Value::Null); // acc
        }
        I_LENGTH => {
            extras.push(Value::Int(0));
        }
        I_ANY | I_EVERY | I_FOREACH => {
            extras.push(args.first().cloned().unwrap_or(Value::Null));
        }
        I_FIRSTWHERE => {
            extras.push(args.first().cloned().unwrap_or(Value::Null));
            extras.push(or_else);
        }
        I_LASTWHERE | I_SINGLEWHERE => {
            extras.push(args.first().cloned().unwrap_or(Value::Null));
            extras.push(or_else);
            extras.push(Value::Null); // found
        }
        I_CONTAINS => {
            extras.push(args.first().cloned().unwrap_or(Value::Null));
        }
        I_ELEMENTAT => {
            extras.push(Value::Int(0));
            extras.push(args.first().cloned().unwrap_or(Value::Int(-1)));
        }
        I_LAST | I_SINGLE => {
            extras.push(Value::Null);
        }
        _ => {}
    }
    // Wrap extras in mutable slots.
    let slots: Vec<Value> = extras
        .into_iter()
        .map(|v| Value::List(Rc::new(RefCell::new(vec![v]))))
        .collect();
    let m = marker(kind, fut.clone(), slots);
    make_sub_borrow(&state, &ops, m.clone(), m.clone(), m);
    Ok(wrap_future(fut))
}

// ---------- StreamController ----------

fn stream_controller_definition() -> BridgedClassDef {
    let mut bc = BridgedClassDef::new("StreamController");
    bc.constructors.insert(
        "".to_string(),
        h::ctor(|_v, _args, named| {
            let sync = named.get("sync").map(as_bool).unwrap_or(false);
            Ok(make_controller(sync, false))
        }),
    );
    bc.constructors.insert(
        "broadcast".to_string(),
        h::ctor(|_v, _args, _named| Ok(make_controller(false, true))),
    );
    bc.getters.insert(
        "stream".to_string(),
        h::getter(|_v, native| {
            let state = controller_state(&native);
            Ok(new_stream(state, Vec::new()))
        }),
    );
    bc.getters.insert(
        "sink".to_string(),
        h::getter(|_v, native| Ok(wrap("StreamSink", native.clone()))),
    );
    bc.getters.insert(
        "isClosed".to_string(),
        h::getter(|_v, native| {
            let state = controller_state(&native);
            let closed = controller_cells(&state)
                .and_then(|c| c.borrow().get(2).cloned())
                .and_then(|x| h::as_bool(&x))
                .unwrap_or(false);
            Ok(Value::Bool(closed))
        }),
    );
    bc.getters.insert(
        "isPaused".to_string(),
        h::getter(|_v, native| {
            let state = controller_state(&native);
            let paused = controller_cells(&state)
                .and_then(|c| c.borrow().get(3).cloned())
                .and_then(|x| h::as_bool(&x))
                .unwrap_or(false);
            Ok(Value::Bool(paused))
        }),
    );
    bc.getters.insert(
        "hasListener".to_string(),
        h::getter(|_v, native| {
            let state = controller_state(&native);
            let has = state_listeners(&state).borrow().iter().any(sub_is_alive);
            Ok(Value::Bool(has))
        }),
    );
    bc.getters.insert(
        "done".to_string(),
        h::getter(|_v, native| {
            let state = controller_state(&native);
            Ok(controller_cells(&state)
                .and_then(|c| c.borrow().get(8).cloned())
                .unwrap_or(Value::Null))
        }),
    );
    for (name, idx) in [
        ("onListen", 4),
        ("onPause", 5),
        ("onResume", 6),
        ("onCancel", 7),
    ] {
        bc.getters.insert(
            name.to_string(),
            h::getter(move |_v, native| {
                let state = controller_state(&native);
                Ok(controller_cells(&state)
                    .and_then(|c| c.borrow().get(idx).cloned())
                    .unwrap_or(Value::Null))
            }),
        );
        bc.setters.insert(
            name.to_string(),
            h::setter(move |_v, native, value| {
                let state = controller_state(&native);
                if let Some(c) = controller_cells(&state) {
                    let mut cb = c.borrow_mut();
                    if cb.len() > idx {
                        cb[idx] = value;
                    }
                }
                Ok(())
            }),
        );
    }
    bc.methods.insert(
        "add".to_string(),
        h::method(|visitor, recv, args, _n| {
            let state = controller_state(&recv);
            controller_add(
                visitor,
                &state,
                args.first().cloned().unwrap_or(Value::Null),
            );
            Ok(Value::Null)
        }),
    );
    bc.methods.insert(
        "addError".to_string(),
        h::method(|visitor, recv, args, _n| {
            let state = controller_state(&recv);
            controller_add_error(
                visitor,
                &state,
                args.first().cloned().unwrap_or(Value::Null),
            );
            Ok(Value::Null)
        }),
    );
    bc.methods.insert(
        "close".to_string(),
        h::method(|visitor, recv, _args, _n| {
            let state = controller_state(&recv);
            controller_close(visitor, &state)
        }),
    );
    bc.methods.insert(
        "addStream".to_string(),
        h::method(|_visitor, recv, args, named| {
            let state = controller_state(&recv);
            let cancel_on_error = named.get("cancelOnError").map(as_bool).unwrap_or(false);
            let stream = args.first().cloned().unwrap_or(Value::Null);
            let out = Rc::new(RefCell::new(async_state::FutureState::new()));
            if let Some(pair) = as_list(&stream) {
                let src_state = pair
                    .borrow()
                    .first()
                    .cloned()
                    .and_then(|v| as_list(&v))
                    .unwrap_or_else(|| Rc::new(RefCell::new(Vec::new())));
                let src_ops = pair
                    .borrow()
                    .get(1)
                    .cloned()
                    .and_then(|v| as_list(&v))
                    .unwrap_or_else(|| Rc::new(RefCell::new(Vec::new())));
                let add_state = state.clone();
                let on_data = Value::NativeFn(Rc::new(NativeFunction {
                    name: Some("<StreamController.addStream add>".to_string()),
                    min_arity: 1,
                    max_arity: Some(1),
                    is_async: false,
                    is_generator: false,
                    closure: Box::new(move |visitor, args, _n| {
                        controller_add(
                            visitor,
                            &add_state,
                            args.first().cloned().unwrap_or(Value::Null),
                        );
                        Ok(Value::Null)
                    }),
                    callable_runtime_type: FunctionRuntimeType::untyped(),
                }));
                let err_state = state.clone();
                let on_error = Value::NativeFn(Rc::new(NativeFunction {
                    name: Some("<StreamController.addStream addError>".to_string()),
                    min_arity: 1,
                    max_arity: Some(1),
                    is_async: false,
                    is_generator: false,
                    closure: Box::new(move |visitor, args, _n| {
                        controller_add_error(
                            visitor,
                            &err_state,
                            args.first().cloned().unwrap_or(Value::Null),
                        );
                        Ok(Value::Null)
                    }),
                    callable_runtime_type: FunctionRuntimeType::untyped(),
                }));
                let done_out = out.clone();
                let on_done = Value::NativeFn(Rc::new(NativeFunction {
                    name: Some("<StreamController.addStream done>".to_string()),
                    min_arity: 0,
                    max_arity: Some(0),
                    is_async: false,
                    is_generator: false,
                    closure: Box::new(move |_v, _args, _n| {
                        async_state::complete(&done_out, Ok(Value::Null));
                        Ok(Value::Null)
                    }),
                    callable_runtime_type: FunctionRuntimeType::untyped(),
                }));
                let sub = make_sub_borrow(&src_state, &src_ops, on_data, on_error, on_done);
                if cancel_on_error && let Some(l) = as_list(&sub) {
                    let mut c = l.borrow_mut();
                    if c.len() > 8 {
                        c[8] = Value::Bool(true);
                    }
                }
            } else {
                async_state::complete(&out, Ok(Value::Null));
            }
            Ok(wrap_future(out))
        }),
    );
    bc
}

fn make_controller(sync: bool, broadcast: bool) -> Value {
    let done = new_future();
    let ctrl = Rc::new(RefCell::new(vec![
        Value::Bool(sync),
        Value::Bool(broadcast),
        Value::Bool(false), // close called
        Value::Bool(false), // paused
        Value::Null,        // onListen
        Value::Null,        // onPause
        Value::Null,        // onResume
        Value::Null,        // onCancel
        done,
    ]));
    let state = new_state();
    state.borrow_mut()[3] = Value::List(ctrl);
    wrap("StreamController", Value::List(state))
}

/// The controller native is the shared stream state (not a [state, ops] pair).
fn controller_state(v: &Value) -> Rc<RefCell<Vec<Value>>> {
    as_list(v).unwrap_or_else(|| Rc::new(RefCell::new(Vec::new())))
}

fn controller_is_closed(state: &Rc<RefCell<Vec<Value>>>) -> bool {
    controller_cells(state)
        .and_then(|c| c.borrow().get(2).cloned())
        .and_then(|x| h::as_bool(&x))
        .unwrap_or(false)
}

fn controller_add(
    visitor: &mut crate::interp::InterpreterVisitor,
    state: &Rc<RefCell<Vec<Value>>>,
    value: Value,
) {
    if controller_is_closed(state) {
        return;
    }
    let sync = controller_cells(state)
        .and_then(|c| c.borrow().first().cloned())
        .and_then(|x| h::as_bool(&x))
        .unwrap_or(false);
    if state_listeners(state).borrow().iter().any(sub_is_alive) {
        if sync {
            handle_stream_event(visitor, &Value::List(state.clone()), &value);
        } else {
            async_state::enqueue_task(async_state::Task::StreamEvent {
                state: Value::List(state.clone()),
                event: value,
            });
        }
    } else {
        state_events(state).borrow_mut().push(value);
    }
}

fn controller_add_error(
    visitor: &mut crate::interp::InterpreterVisitor,
    state: &Rc<RefCell<Vec<Value>>>,
    error: Value,
) {
    if controller_is_closed(state) {
        return;
    }
    let sync = controller_cells(state)
        .and_then(|c| c.borrow().first().cloned())
        .and_then(|x| h::as_bool(&x))
        .unwrap_or(false);
    if state_listeners(state).borrow().iter().any(sub_is_alive) {
        if sync {
            handle_stream_error(visitor, &Value::List(state.clone()), &error);
        } else {
            async_state::enqueue_task(async_state::Task::StreamError {
                state: Value::List(state.clone()),
                error,
            });
        }
    } else {
        state_events(state)
            .borrow_mut()
            .push(Value::List(Rc::new(RefCell::new(vec![
                Value::Bool(true),
                error,
            ]))));
    }
}

fn controller_close(
    visitor: &mut crate::interp::InterpreterVisitor,
    state: &Rc<RefCell<Vec<Value>>>,
) -> Result<Value, InterpError> {
    let done = if let Some(ctrl) = controller_cells(state) {
        if !as_bool(&ctrl.borrow().get(2).cloned().unwrap_or(Value::Bool(false))) {
            ctrl.borrow_mut()[2] = Value::Bool(true);
            let has_listener = state_listeners(state).borrow().iter().any(sub_is_alive);
            if has_listener {
                async_state::enqueue_task(async_state::Task::StreamDone {
                    state: Value::List(state.clone()),
                });
            } else {
                state.borrow_mut()[2] = Value::Bool(true);
                let f = ctrl.borrow().get(8).cloned();
                if let Some(f) = f.and_then(|v| async_state::unwrap_future(&v)) {
                    async_state::complete(&f, Ok(Value::Null));
                }
            }
        }
        ctrl.borrow().get(8).cloned().unwrap_or(Value::Null)
    } else {
        Value::Null
    };
    let _ = visitor;
    Ok(done)
}

// ---------- StreamSubscription ----------

fn stream_subscription_definition() -> BridgedClassDef {
    let mut bc = BridgedClassDef::new("StreamSubscription");
    bc.methods.insert(
        "cancel".to_string(),
        h::method(|visitor, recv, _args, _n| {
            cancel_sub(visitor, &recv);
            Ok(new_future())
        }),
    );
    bc.methods.insert(
        "pause".to_string(),
        h::method(|visitor, recv, _args, _n| {
            if let Some(cells) = as_list(&recv) {
                cells.borrow_mut()[6] = Value::Bool(true);
                if let Some(ctrl) = controller_cells(&sub_state(&recv)) {
                    ctrl.borrow_mut()[3] = Value::Bool(true);
                    let on_pause = ctrl.borrow().get(5).cloned().unwrap_or(Value::Null);
                    if !on_pause.is_null() {
                        let _ = call_cb(visitor, &on_pause, vec![]);
                    }
                }
            }
            Ok(Value::Null)
        }),
    );
    bc.methods.insert(
        "resume".to_string(),
        h::method(|visitor, recv, _args, _n| {
            let was_paused = sub_is_paused(&recv);
            if was_paused {
                if let Some(cells) = as_list(&recv) {
                    cells.borrow_mut()[6] = Value::Bool(false);
                    let buffered: Vec<Value> = cells
                        .borrow()
                        .get(7)
                        .cloned()
                        .and_then(|b| as_list(&b))
                        .map(|b| b.borrow().clone())
                        .unwrap_or_default();
                    if let Some(b) = cells.borrow().get(7).cloned().and_then(|b| as_list(&b)) {
                        b.borrow_mut().clear();
                    }
                    for e in buffered {
                        async_state::enqueue_task(async_state::Task::StreamEventSub {
                            sub: recv.clone(),
                            event: e,
                        });
                    }
                }
                if let Some(ctrl) = controller_cells(&sub_state(&recv)) {
                    ctrl.borrow_mut()[3] = Value::Bool(false);
                    let on_resume = ctrl.borrow().get(6).cloned().unwrap_or(Value::Null);
                    if !on_resume.is_null() {
                        let _ = call_cb(visitor, &on_resume, vec![]);
                    }
                }
            }
            Ok(Value::Null)
        }),
    );
    bc.getters.insert(
        "isPaused".to_string(),
        h::getter(|_v, recv| Ok(Value::Bool(sub_is_paused(&recv)))),
    );
    for (name, idx) in [("onData", 3), ("onError", 4), ("onDone", 5)] {
        bc.getters.insert(
            name.to_string(),
            h::getter(move |_v, recv| Ok(sub_cell(&recv, idx))),
        );
        bc.setters.insert(
            name.to_string(),
            h::setter(move |_v, recv, value| {
                if let Some(l) = as_list(&recv) {
                    let mut c = l.borrow_mut();
                    if c.len() > idx {
                        c[idx] = value;
                    }
                }
                Ok(())
            }),
        );
    }
    bc
}

// ---------- StreamTransformer ----------

fn stream_transformer_definition() -> BridgedClassDef {
    let mut bc = BridgedClassDef::new("StreamTransformer");
    bc.static_methods.insert(
        "castFrom".to_string(),
        h::static_method(|_v, args, _n| Ok(args.first().cloned().unwrap_or(Value::Null))),
    );
    bc.constructors.insert(
        "".to_string(),
        h::ctor(|_v, _args, _named| {
            Ok(wrap(
                "StreamTransformer",
                Value::List(Rc::new(RefCell::new(vec![
                    Value::Null,
                    Value::Null,
                    Value::Null,
                ]))),
            ))
        }),
    );
    bc.constructors.insert(
        "fromHandlers".to_string(),
        h::ctor(|_v, _args, named| {
            Ok(wrap(
                "StreamTransformer",
                Value::List(Rc::new(RefCell::new(vec![
                    Value::Null,
                    Value::Null,
                    Value::List(Rc::new(RefCell::new(vec![
                        named.get("handleData").cloned().unwrap_or(Value::Null),
                        named.get("handleError").cloned().unwrap_or(Value::Null),
                        named.get("handleDone").cloned().unwrap_or(Value::Null),
                    ]))),
                ]))),
            ))
        }),
    );
    bc.constructors.insert(
        "fromBind".to_string(),
        h::ctor(|_v, args, _named| {
            Ok(wrap(
                "StreamTransformer",
                Value::List(Rc::new(RefCell::new(vec![
                    Value::Null,
                    args.first().cloned().unwrap_or(Value::Null),
                    Value::Null,
                ]))),
            ))
        }),
    );
    bc.methods.insert(
        "cast".to_string(),
        h::method(|_v, recv, _args, _n| {
            let native = as_list(&recv).unwrap_or_else(|| Rc::new(RefCell::new(Vec::new())));
            Ok(wrap(
                "StreamTransformer",
                Value::List(Rc::new(RefCell::new(vec![
                    Value::List(native),
                    Value::Null,
                    Value::Null,
                ]))),
            ))
        }),
    );
    bc.methods.insert(
        "bind".to_string(),
        h::method(|visitor, recv, args, _n| {
            let s = args.first().cloned().unwrap_or(Value::Null);
            let state = stream_state(&s);
            let ops = stream_ops(&s);
            Ok(resolve_transform(visitor, &recv, &state, &ops))
        }),
    );
    bc.methods.insert(
        "transform".to_string(),
        h::method(|_v, recv, _args, _n| Ok(recv.clone())),
    );
    bc
}

// ---------- StreamSink / EventSink ----------

fn stream_sink_definition() -> BridgedClassDef {
    let mut bc = BridgedClassDef::new("StreamSink");
    bc.methods.insert(
        "add".to_string(),
        h::method(|visitor, recv, args, _n| {
            let state = controller_state(&recv);
            controller_add(
                visitor,
                &state,
                args.first().cloned().unwrap_or(Value::Null),
            );
            Ok(new_future())
        }),
    );
    bc.methods.insert(
        "addError".to_string(),
        h::method(|visitor, recv, args, _n| {
            let state = controller_state(&recv);
            controller_add_error(
                visitor,
                &state,
                args.first().cloned().unwrap_or(Value::Null),
            );
            Ok(new_future())
        }),
    );
    bc.methods.insert(
        "close".to_string(),
        h::method(|visitor, recv, _args, _n| {
            let state = controller_state(&recv);
            controller_close(visitor, &state)
        }),
    );
    bc.getters.insert(
        "done".to_string(),
        h::getter(|_v, recv| {
            let state = controller_state(&recv);
            Ok(controller_cells(&state)
                .and_then(|c| c.borrow().get(8).cloned())
                .unwrap_or(Value::Null))
        }),
    );
    bc
}

fn event_sink_definition() -> BridgedClassDef {
    let mut bc = BridgedClassDef::new("EventSink");
    bc.methods.insert(
        "add".to_string(),
        h::method(|visitor, recv, args, _n| {
            let cells = as_list(&recv).unwrap_or_else(|| Rc::new(RefCell::new(Vec::new())));
            let sub = cells.borrow().first().cloned().unwrap_or(Value::Null);
            let ops = cells
                .borrow()
                .get(1)
                .cloned()
                .and_then(|o| as_list(&o))
                .unwrap_or_else(|| Rc::new(RefCell::new(Vec::new())));
            let index = cells
                .borrow()
                .get(2)
                .cloned()
                .and_then(|i| h::as_int(&i))
                .unwrap_or(0) as usize;
            process_event(
                visitor,
                &sub,
                &ops,
                index,
                args.first().cloned().unwrap_or(Value::Null),
                None,
            );
            Ok(Value::Null)
        }),
    );
    bc.methods.insert(
        "addError".to_string(),
        h::method(|visitor, recv, args, _n| {
            let cells = as_list(&recv).unwrap_or_else(|| Rc::new(RefCell::new(Vec::new())));
            let sub = cells.borrow().first().cloned().unwrap_or(Value::Null);
            let ops = cells
                .borrow()
                .get(1)
                .cloned()
                .and_then(|o| as_list(&o))
                .unwrap_or_else(|| Rc::new(RefCell::new(Vec::new())));
            let index = cells
                .borrow()
                .get(2)
                .cloned()
                .and_then(|i| h::as_int(&i))
                .unwrap_or(0) as usize;
            process_error(
                visitor,
                &sub,
                &ops,
                index,
                args.first().cloned().unwrap_or(Value::Null),
                None,
            );
            Ok(Value::Null)
        }),
    );
    bc.methods.insert(
        "close".to_string(),
        h::method(|visitor, recv, _args, _n| {
            let cells = as_list(&recv).unwrap_or_else(|| Rc::new(RefCell::new(Vec::new())));
            let sub = cells.borrow().first().cloned().unwrap_or(Value::Null);
            let ops = cells
                .borrow()
                .get(1)
                .cloned()
                .and_then(|o| as_list(&o))
                .unwrap_or_else(|| Rc::new(RefCell::new(Vec::new())));
            let index = cells
                .borrow()
                .get(2)
                .cloned()
                .and_then(|i| h::as_int(&i))
                .unwrap_or(0) as usize;
            process_done(visitor, &sub, &ops, index, None);
            Ok(Value::Null)
        }),
    );
    bc
}

// ---------- StreamIterator ----------

fn stream_iterator_definition() -> BridgedClassDef {
    let mut bc = BridgedClassDef::new("StreamIterator");
    bc.constructors.insert(
        "".to_string(),
        h::ctor(|_v, args, _n| {
            let stream = args.first().cloned().unwrap_or(Value::Null);
            let state = stream_state(&stream);
            let ops = stream_ops(&stream);
            let it = Rc::new(RefCell::new(vec![
                Value::Bool(false), // 0 started
                Value::Bool(false), // 1 done
                Value::Null,        // 2 current
                Value::Bool(false), // 3 has_current
                Value::Null,        // 4 pending future
                Value::Null,        // 5 error
                Value::Null,        // 6 subscription
                Value::list(),      // 7 overflow queue
            ]));
            Ok(wrap(
                "StreamIterator",
                Value::List(Rc::new(RefCell::new(vec![
                    Value::List(state),
                    Value::List(ops),
                    Value::List(it),
                ]))),
            ))
        }),
    );
    bc.methods.insert(
        "moveNext".to_string(),
        h::method(|_visitor, recv, _args, _n| {
            let cells = as_list(&recv).unwrap_or_else(|| Rc::new(RefCell::new(Vec::new())));
            let state = cells
                .borrow()
                .first()
                .cloned()
                .and_then(|s| as_list(&s))
                .unwrap_or_else(|| Rc::new(RefCell::new(Vec::new())));
            let ops = cells
                .borrow()
                .get(1)
                .cloned()
                .and_then(|o| as_list(&o))
                .unwrap_or_else(|| Rc::new(RefCell::new(Vec::new())));
            let it = cells
                .borrow()
                .get(2)
                .cloned()
                .and_then(|i| as_list(&i))
                .unwrap_or_else(|| Rc::new(RefCell::new(Vec::new())));
            let (has_current, done, started) = {
                let c = it.borrow();
                (
                    c.get(3)
                        .cloned()
                        .and_then(|x| h::as_bool(&x))
                        .unwrap_or(false),
                    c.get(1)
                        .cloned()
                        .and_then(|x| h::as_bool(&x))
                        .unwrap_or(false),
                    c.first()
                        .cloned()
                        .and_then(|x| h::as_bool(&x))
                        .unwrap_or(false),
                )
            };
            if has_current {
                let q = it.borrow().get(7).cloned().and_then(|v| as_list(&v));
                it.borrow_mut()[3] = Value::Bool(false);
                if let Some(q) = q {
                    let next = {
                        let mut q = q.borrow_mut();
                        if q.is_empty() {
                            None
                        } else {
                            Some(q.remove(0))
                        }
                    };
                    if let Some(next) = next {
                        let mut c = it.borrow_mut();
                        c[2] = next;
                        c[3] = Value::Bool(true);
                        return Ok(future_value(Value::Bool(true)));
                    }
                }
                let pending = it.borrow().get(4).cloned().unwrap_or(Value::Null);
                if let Some(f) = async_state::unwrap_future(&pending) {
                    return Ok(wrap_future(f));
                }
                return Ok(future_value(Value::Bool(false)));
            }
            if done {
                let error = it.borrow().get(5).cloned().unwrap_or(Value::Null);
                if error.is_null() {
                    return Ok(future_value(Value::Bool(false)));
                }
                return Ok(future_value(error));
            }
            if !started {
                it.borrow_mut()[0] = Value::Bool(true);
                let m = marker(
                    I_ITER,
                    Rc::new(RefCell::new(async_state::FutureState::new())),
                    vec![Value::List(it.clone())],
                );
                make_sub_borrow(&state, &ops, m.clone(), m.clone(), m);
            }
            let f = Rc::new(RefCell::new(async_state::FutureState::new()));
            {
                let mut c = it.borrow_mut();
                if c.len() > 4 {
                    c[4] = Value::Future(f.clone());
                }
            }
            Ok(wrap_future(f))
        }),
    );
    bc.getters.insert(
        "current".to_string(),
        h::getter(|_v, recv| {
            let cells = as_list(&recv).unwrap_or_else(|| Rc::new(RefCell::new(Vec::new())));
            let it = cells
                .borrow()
                .get(2)
                .cloned()
                .and_then(|i| as_list(&i))
                .unwrap_or_else(|| Rc::new(RefCell::new(Vec::new())));
            Ok(it.borrow().get(2).cloned().unwrap_or(Value::Null))
        }),
    );
    bc
}
