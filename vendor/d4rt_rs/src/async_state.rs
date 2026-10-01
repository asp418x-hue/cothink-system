//! Async execution state for `dart:async` support.
//!
//! Architecture: an async function runs synchronously until an `await`
//! hits a *pending* future. `visit_await` then registers a
//! [`ResumeContinuation`] on the future's [`FutureState`] and returns
//! [`InterpError::Suspend`], which unwinds to the async call boundary
//! (the completer of that async function's own future). The event loop
//! (pumped by `Context::eval`) processes a queue of [`Task`]s; when a
//! future completes, its continuations become `Resume` tasks that re-run
//! the async body from the point of suspension.
//!
//! Resume re-execution replays: the resumed body runs top-down again and
//! every `await` that has already executed (and is still cached) returns
//! its cached future's result instead of re-evaluating its expression —
//! re-evaluating would re-create a pending future and loop forever.
//! - cache keys are `(await node id, enclosing loop-iteration stack)`:
//!   loops restart from iteration 0 on every resume pass, so the key is
//!   stable across passes and distinguishes each loop iteration's own
//!   `await` execution. Entries are never consumed, so previously
//!   suspended iterations replay their values while later iterations
//!   evaluate fresh futures (and suspend again until the event loop
//!   completes them).

use crate::ast::CompilationUnit;
use crate::ast::NodeId;
use crate::env::Environment;
use crate::error::InterpError;
use crate::interp::InterpreterVisitor;
use crate::stdlib::helpers as h;
use crate::value::Value;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
#[cfg(feature = "io")]
use std::sync::mpsc::Receiver;

/// State of a single pending/completed future.
pub struct FutureState {
    pub completed: bool,
    pub value: Option<Value>,
    pub error: Option<Value>,
    pub continuations: Vec<Continuation>,
}

impl Default for FutureState {
    fn default() -> Self {
        Self::new()
    }
}

impl FutureState {
    pub fn new() -> Self {
        FutureState {
            completed: false,
            value: None,
            error: None,
            continuations: Vec::new(),
        }
    }
}

/// Stack entry for a running async function: the function's compilation
/// unit, body node and the completer that must be completed when the
/// function finishes. For `async*` bodies `stream_state` is the event
/// queue state of the stream the generator emits into.
#[derive(Clone)]
pub struct AsyncFrame {
    pub unit: Rc<CompilationUnit>,
    pub body: NodeId,
    pub completer: Rc<RefCell<FutureState>>,
    pub stream_state: Option<Value>,
}

/// Captured state needed to resume an async body after an `await`.
pub struct ResumeContinuation {
    pub unit: Rc<CompilationUnit>,
    pub body: NodeId,
    pub function_env: Rc<RefCell<Environment>>,
    pub completer: Rc<RefCell<FutureState>>,
    pub cache: HashMap<(NodeId, Vec<usize>), Result<Value, Value>>,
    /// The await node (with its loop-iteration stack) where the pass
    /// suspended: the resume pass skips statements before it.
    pub resume_node: NodeId,
    pub resume_iters: Vec<usize>,
    /// The event-queue state of the enclosing `async*` stream, if any.
    pub stream_state: Option<Value>,
}

/// A listener attached to a future.
pub enum Continuation {
    /// Resume an async body when the future completes.
    Resume(ResumeContinuation),
    /// `.then(onValue, onError)` chain: run the callback with the result.
    Then {
        callback: Option<Value>,
        on_error: Option<Value>,
        then_future: Rc<RefCell<FutureState>>,
    },
    /// `.whenComplete(cb)`: run `cb` then forward the original result.
    WhenComplete {
        callback: Value,
        then_future: Rc<RefCell<FutureState>>,
    },
    /// Forward the completion (used for future flattening).
    Forward(Rc<RefCell<FutureState>>),
    /// Resume an async stream op (asyncMap/asyncExpand/handleError) whose
    /// callback returned a future that has now completed.
    StreamOp {
        sub: Value,
        ops: Rc<RefCell<Vec<Value>>>,
        index: usize,
        rest_ops: Rc<RefCell<Vec<Value>>>,
        rest_index: usize,
    },
    /// Emit the completed value into a stream state (Stream.fromFuture,
    /// Stream.fromFutures).
    StreamEmit { state: Value },
    /// The body of an `async*` generator finished (or threw): close the
    /// generator's stream, delivering any error first.
    AsyncStarDone { state: Value },
}

/// Unit of work processed by the event loop pump.
pub enum Task {
    Complete {
        future: Rc<RefCell<FutureState>>,
        value: Value,
        /// Simulated delay in microseconds; advances the virtual clock.
        delay_us: i64,
    },
    CompleteError {
        future: Rc<RefCell<FutureState>>,
        error: Value,
    },
    Forward {
        future: Rc<RefCell<FutureState>>,
        result: Result<Value, Value>,
    },
    Resume(ResumeContinuation),
    Then {
        callback: Option<Value>,
        on_error: Option<Value>,
        then_future: Rc<RefCell<FutureState>>,
        result: Result<Value, Value>,
    },
    WhenComplete {
        callback: Value,
        then_future: Rc<RefCell<FutureState>>,
        result: Result<Value, Value>,
    },
    /// A one-shot timer callback. `timer` is the bridged `[active, tick]`
    /// instance, if any; it is marked inactive once the callback fires.
    Timer {
        callback: Value,
        timer: Option<Value>,
    },
    /// A repeating timer callback (Timer.periodic); stops firing once the
    /// timer was canceled.
    TimerPeriodic {
        timer: Value,
        callback: Value,
    },
    /// A `Future.timeout` deadline: once the virtual clock has advanced by
    /// `delay_us`, the target future completes with `error` (a bridged
    /// `TimeoutException`) or with `on_timeout`'s result, unless the source
    /// future has already completed — in which case the deadline is moot.
    FutureTimeout {
        source: Rc<RefCell<FutureState>>,
        target: Rc<RefCell<FutureState>>,
        delay_us: i64,
        on_timeout: Value,
        error: Value,
    },
    /// Fire a Stream.timeout check: if no event passed the K_TIMEOUT op
    /// since `gen`, the stream times out.
    Timeout {
        sub: Value,
        ops: Rc<RefCell<Vec<Value>>>,
        index: usize,
        gen_id: i64,
    },
    /// Deliver a data event to the listeners of a stream state.
    StreamEvent {
        state: Value,
        event: Value,
    },
    /// Deliver a data event to a single subscription (pause-buffer drain).
    StreamEventSub {
        sub: Value,
        event: Value,
    },
    /// Deliver an error event to the listeners of a stream state.
    StreamError {
        state: Value,
        error: Value,
    },
    /// Deliver the done event to the listeners of a stream state.
    StreamDone {
        state: Value,
    },
    /// Periodic stream tick.
    StreamTick {
        state: Value,
    },
    /// Resume an async stream op whose callback's future completed.
    StreamOpResume {
        sub: Value,
        ops: Rc<RefCell<Vec<Value>>>,
        index: usize,
        rest_ops: Rc<RefCell<Vec<Value>>>,
        rest_index: usize,
        result: Result<Value, Value>,
    },
    /// A completed future feeding a `Stream.fromFuture(s)` state.
    StreamEmit {
        state: Value,
        result: Result<Value, Value>,
    },
    /// Poll a started OS process: drains stdout/stderr chunks into its
    /// stream controllers and completes the exit future when it exits.
    #[cfg(feature = "io")]
    ProcessPoll {
        pid: i64,
        exit_future: Rc<RefCell<FutureState>>,
        stdout_ctrl: Value,
        stderr_ctrl: Value,
    },
    /// A Socket.connect in flight: waits for the worker thread to report
    /// the connected socket (or the connection error).
    #[cfg(feature = "io")]
    SocketConnectPoll {
        state: Rc<RefCell<FutureState>>,
        host: String,
        port: i64,
        id_rx: Receiver<Result<(i64, String, i64), String>>,
        data_rx: Receiver<crate::stdlib::http::SocketData>,
        class: String,
    },
    /// Drain a connected socket's data channel into its stream controller.
    #[cfg(feature = "io")]
    SocketPoll {
        id: i64,
        rx: Receiver<crate::stdlib::http::SocketData>,
        ctrl: Value,
        done: Rc<RefCell<FutureState>>,
    },
    /// Drain a UDP socket's datagram channel into its receive queue and
    /// event stream controller.
    #[cfg(feature = "io")]
    UdpPoll {
        id: i64,
        rx: Receiver<crate::stdlib::http::UdpData>,
        ctrl: Value,
        done: Rc<RefCell<FutureState>>,
    },
    /// Drain a server's accept channel, delivering HttpRequest values.
    #[cfg(feature = "io")]
    ServerPoll {
        id: i64,
        rx: Receiver<crate::stdlib::http::ServerData>,
        ctrl: Value,
    },
    /// Drain an HttpClient exchange channel, completing the response future.
    #[cfg(feature = "io")]
    HttpClientPoll {
        state: Rc<RefCell<FutureState>>,
        rx: Receiver<crate::stdlib::http::HttpData>,
        ctrl: Value,
    },
    /// Deliver one isolate message into a ReceivePort's stream controller.
    PortEvent {
        ctrl: Value,
        message: Value,
    },
    /// Drain a filesystem-watcher channel into a stream controller.
    #[cfg(feature = "io")]
    WatchPoll {
        id: i64,
        rx: Receiver<crate::stdlib::io::WatchEvent>,
        ctrl: Value,
    },
}

pub fn enqueue_task(task: Task) {
    crate::state::current().tasks.borrow_mut().push_back(task);
}

pub fn dequeue_task() -> Option<Task> {
    crate::state::current().tasks.borrow_mut().pop_front()
}

/// Number of tasks currently pending in the queue.
pub fn task_queue_len() -> usize {
    crate::state::current().tasks.borrow().len()
}

/// Current simulated time in microseconds.
pub fn virtual_now() -> i64 {
    crate::state::current().clock.get()
}

/// Advances the simulated clock by `us` microseconds.
pub fn advance_virtual(us: i64) {
    let s = crate::state::current();
    s.clock.set(s.clock.get() + us.max(0));
}

/// Completes `state` and schedules its listeners.
pub fn complete(state: &Rc<RefCell<FutureState>>, result: Result<Value, Value>) {
    let mut st = state.borrow_mut();
    if st.completed {
        return;
    }
    st.completed = true;
    match result {
        Ok(v) => st.value = Some(v),
        Err(e) => st.error = Some(e),
    }
    let conts = std::mem::take(&mut st.continuations);
    drop(st);
    let result = future_result(&state.borrow());
    for cont in conts {
        match cont {
            Continuation::Resume(c) => enqueue_task(Task::Resume(c)),
            Continuation::Then {
                callback,
                on_error,
                then_future,
            } => enqueue_task(Task::Then {
                callback,
                on_error,
                then_future,
                result: result.clone(),
            }),
            Continuation::WhenComplete {
                callback,
                then_future,
            } => enqueue_task(Task::WhenComplete {
                callback,
                then_future,
                result: result.clone(),
            }),
            Continuation::Forward(outer) => enqueue_task(Task::Forward {
                future: outer,
                result: result.clone(),
            }),
            Continuation::StreamOp {
                sub,
                ops,
                index,
                rest_ops,
                rest_index,
            } => enqueue_task(Task::StreamOpResume {
                sub,
                ops,
                index,
                rest_ops,
                rest_index,
                result: result.clone(),
            }),
            Continuation::StreamEmit { state } => enqueue_task(Task::StreamEmit {
                state,
                result: result.clone(),
            }),
            Continuation::AsyncStarDone { state } => match result.clone() {
                Ok(_) => enqueue_task(Task::StreamDone { state }),
                Err(e) => {
                    enqueue_task(Task::StreamError {
                        state: state.clone(),
                        error: e,
                    });
                    enqueue_task(Task::StreamDone { state });
                }
            },
        }
    }
}

/// Whether a Timer instance is still active (not canceled). Timer
/// instances are bridged lists `[active, tick]`.
pub fn timer_active(timer: &Value) -> bool {
    match timer {
        Value::Bridged(b) => match &b.borrow().native {
            Value::List(l) => match l.borrow().first() {
                Some(Value::Bool(active)) => *active,
                _ => false,
            },
            _ => false,
        },
        _ => false,
    }
}

/// Marks a timer inactive after its callback fires.
pub fn mark_timer_inactive(timer: &Value) {
    if let Value::Bridged(b) = timer
        && let Value::List(l) = &b.borrow().native
    {
        l.borrow_mut()[0] = Value::Bool(false);
    }
}

/// Increments the tick counter of an active periodic timer.
pub fn bump_timer_tick(timer: &Value) {
    if let Value::Bridged(b) = timer
        && let Value::List(l) = &b.borrow().native
    {
        let mut l = l.borrow_mut();
        if l.len() > 1 {
            let t = h::as_int(&l[1]).unwrap_or(0);
            l[1] = Value::Int(t + 1);
        }
    }
}

/// Result of a completed future as `Ok(value)` / `Err(error)`.
pub fn future_result(f: &FutureState) -> Result<Value, Value> {
    if let Some(v) = &f.value {
        Ok(v.clone())
    } else if let Some(e) = &f.error {
        Err(e.clone())
    } else {
        Ok(Value::Null)
    }
}

/// Extracts the underlying [`FutureState`] from a raw `Value::Future` or
/// a bridged wrapper around one.
pub fn unwrap_future(v: &Value) -> Option<Rc<RefCell<FutureState>>> {
    match v {
        Value::Future(f) => Some(f.clone()),
        Value::Bridged(b) => match &b.borrow().native {
            Value::Future(f) => Some(f.clone()),
            _ => None,
        },
        _ => None,
    }
}

/// Converts an `InterpError` into a catchable error value.
pub fn exception_value(e: InterpError) -> Value {
    match e {
        InterpError::Throw(v) => v,
        other => Value::str(other.to_string()),
    }
}

/// Completes `outer` when `v` completes; if `v` is not a future, completes
/// `outer` with `v` directly.
pub fn chain_into(v: &Value, outer: &Rc<RefCell<FutureState>>) {
    match unwrap_future(v) {
        Some(inner) => {
            let mut st = inner.borrow_mut();
            if st.completed {
                let result = future_result(&st);
                drop(st);
                enqueue_task(Task::Forward {
                    future: outer.clone(),
                    result,
                });
            } else {
                st.continuations.push(Continuation::Forward(outer.clone()));
            }
        }
        None => complete(outer, Ok(v.clone())),
    }
}

/// Runs one queued task. May enqueue further tasks (via `complete`).
pub fn process_task(visitor: &mut InterpreterVisitor, task: Task) {
    match task {
        Task::Complete {
            future,
            value,
            delay_us,
        } => {
            advance_virtual(delay_us);
            complete(&future, Ok(value));
        }
        Task::CompleteError { future, error } => complete(&future, Err(error)),
        Task::Forward { future, result } => complete(&future, result),
        Task::Resume(cont) => match visitor.resume_continuation(&cont) {
            Ok(v) => complete(&cont.completer, Ok(v)),
            Err(InterpError::Suspend) => {}
            Err(e) => complete(&cont.completer, Err(exception_value(e))),
        },
        Task::Then {
            callback,
            on_error,
            then_future,
            result,
        } => {
            let call_result: Result<Value, InterpError> = match (&result, &callback) {
                (Ok(v), Some(cb)) => visitor.call_value(cb, vec![v.clone()], HashMap::new()),
                (Ok(v), None) => Ok(v.clone()),
                (Err(e), _) => match &on_error {
                    Some(oe) => visitor.call_value(oe, vec![e.clone()], HashMap::new()),
                    None => Err(InterpError::Throw(e.clone())),
                },
            };
            match call_result {
                Ok(v) => chain_into(&v, &then_future),
                Err(InterpError::Throw(e)) => complete(&then_future, Err(e)),
                Err(e) => complete(&then_future, Err(Value::str(e.to_string()))),
            }
        }
        Task::WhenComplete {
            callback,
            then_future,
            result,
        } => {
            let r = visitor.call_value(&callback, vec![], HashMap::new());
            let final_result = match r {
                Ok(_) => result,
                Err(InterpError::Throw(e)) => Err(e),
                Err(e) => Err(Value::str(e.to_string())),
            };
            complete(&then_future, final_result);
        }
        Task::Timer { callback, timer } => {
            if let Some(t) = &timer {
                if !timer_active(t) {
                    return;
                }
                // A one-shot timer is inactive once its callback begins.
                mark_timer_inactive(t);
            }
            let _ = visitor.call_value(&callback, vec![], HashMap::new());
        }
        Task::TimerPeriodic { timer, callback } => {
            if timer_active(&timer) {
                let _ = visitor.call_value(&callback, vec![timer.clone()], HashMap::new());
                if timer_active(&timer) {
                    bump_timer_tick(&timer);
                    enqueue_task(Task::TimerPeriodic { timer, callback });
                }
            }
        }
        Task::FutureTimeout {
            source,
            target,
            delay_us,
            on_timeout,
            error,
        } => {
            if !source.borrow().completed {
                advance_virtual(delay_us);
                if on_timeout.is_null() {
                    complete(&target, Err(error));
                } else {
                    match visitor.call_value(&on_timeout, vec![], HashMap::new()) {
                        Ok(v) => chain_into(&v, &target),
                        Err(InterpError::Throw(e)) => complete(&target, Err(e)),
                        Err(e) => complete(&target, Err(Value::str(e.to_string()))),
                    }
                }
            }
        }
        Task::Timeout {
            sub,
            ops,
            index,
            gen_id,
        } => crate::stdlib::stream_lib::handle_stream_timeout(visitor, &sub, &ops, index, gen_id),
        Task::StreamEvent { state, event } => {
            if std::env::var("D4RT_DBG").is_ok() {
                eprintln!("[dbg] processing StreamEvent");
            }
            crate::stdlib::stream_lib::handle_stream_event(visitor, &state, &event);
        }
        Task::StreamEventSub { sub, event } => {
            crate::stdlib::stream_lib::handle_stream_event_sub(visitor, &sub, &event);
        }
        Task::StreamError { state, error } => {
            crate::stdlib::stream_lib::handle_stream_error(visitor, &state, &error);
        }
        Task::StreamDone { state } => {
            crate::stdlib::stream_lib::handle_stream_done(visitor, &state);
        }
        Task::StreamTick { state } => {
            crate::stdlib::stream_lib::handle_stream_tick(visitor, &state);
        }
        Task::StreamOpResume {
            sub,
            ops,
            index,
            rest_ops,
            rest_index,
            result,
        } => crate::stdlib::stream_lib::handle_stream_op_resume(
            visitor, &sub, &ops, index, &rest_ops, rest_index, result,
        ),
        Task::StreamEmit { state, result } => {
            crate::stdlib::stream_lib::handle_stream_emit(visitor, &state, result);
        }
        #[cfg(feature = "io")]
        Task::ProcessPoll {
            pid,
            exit_future,
            stdout_ctrl,
            stderr_ctrl,
        } => {
            let keep = crate::stdlib::process::poll_process(
                visitor,
                pid,
                &exit_future,
                &stdout_ctrl,
                &stderr_ctrl,
            );
            if keep {
                enqueue_task(Task::ProcessPoll {
                    pid,
                    exit_future,
                    stdout_ctrl,
                    stderr_ctrl,
                });
            }
        }
        #[cfg(feature = "io")]
        Task::SocketConnectPoll {
            state,
            host,
            port,
            id_rx,
            data_rx,
            class,
        } => match id_rx.try_recv() {
            Ok(Ok((id, rhost, rport))) => {
                let ctrl = crate::stdlib::stream_lib::make_controller_value(false, false);
                let stream = crate::stdlib::stream_lib::controller_stream_value(&ctrl);
                let done = Rc::new(RefCell::new(FutureState::new()));
                let value = crate::stdlib::http::wrap_socket_value(
                    id, &host, port, &rhost, rport, &ctrl, &stream, &done, &class,
                );
                complete(&state, Ok(value));
                enqueue_task(Task::SocketPoll {
                    id,
                    rx: data_rx,
                    ctrl,
                    done,
                });
            }
            Ok(Err(e)) => {
                complete(
                    &state,
                    Err(crate::stdlib::http::socket_exception(&e, &host, port)),
                );
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => {
                enqueue_task(Task::SocketConnectPoll {
                    state,
                    host,
                    port,
                    id_rx,
                    data_rx,
                    class,
                });
            }
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                complete(
                    &state,
                    Err(crate::stdlib::http::socket_exception(
                        "connection refused",
                        &host,
                        port,
                    )),
                );
            }
        },
        #[cfg(feature = "io")]
        Task::SocketPoll { id, rx, ctrl, done } => {
            let keep = crate::stdlib::http::poll_socket(visitor, id, &rx, &ctrl, &done);
            if keep {
                enqueue_task(Task::SocketPoll { id, rx, ctrl, done });
            }
        }
        #[cfg(feature = "io")]
        Task::UdpPoll { id, rx, ctrl, done } => {
            let keep = crate::stdlib::http::poll_udp(visitor, id, &rx, &ctrl, &done);
            if keep {
                enqueue_task(Task::UdpPoll { id, rx, ctrl, done });
            }
        }
        #[cfg(feature = "io")]
        Task::ServerPoll { id, rx, ctrl } => {
            let keep = crate::stdlib::http::poll_server(visitor, id, &rx, &ctrl);
            if keep {
                enqueue_task(Task::ServerPoll { id, rx, ctrl });
            }
        }
        #[cfg(feature = "io")]
        Task::HttpClientPoll { state, rx, ctrl } => {
            let keep = crate::stdlib::http::poll_http_client(visitor, &state, &rx, &ctrl);
            if keep {
                enqueue_task(Task::HttpClientPoll { state, rx, ctrl });
            }
        }
        Task::PortEvent { ctrl, message } => {
            crate::stdlib::stream_lib::controller_add_value(visitor, &ctrl, message);
        }
        #[cfg(feature = "io")]
        Task::WatchPoll { id, rx, ctrl } => {
            let keep = crate::stdlib::io::poll_watch(visitor, id, &rx, &ctrl);
            if keep {
                enqueue_task(Task::WatchPoll { id, rx, ctrl });
            }
        }
    }
}

/// Debug helper: stringifies a single task.
pub fn debug_dump_one(t: &Task) -> String {
    match t {
        Task::Resume(c) => format!("Resume(node={})", c.resume_node),
        Task::StreamEvent { .. } => "StreamEvent".into(),
        Task::StreamError { .. } => "StreamError".into(),
        Task::StreamDone { .. } => "StreamDone".into(),
        Task::Timer { .. } => "Timer".into(),
        Task::TimerPeriodic { .. } => "TimerPeriodic".into(),
        Task::FutureTimeout { .. } => "FutureTimeout".into(),
        Task::Timeout { .. } => "Timeout".into(),
        Task::Complete { delay_us, .. } => format!("Complete(delay={delay_us})"),
        Task::CompleteError { .. } => "CompleteError".into(),
        Task::Forward { .. } => "Forward".into(),
        Task::Then { .. } => "Then".into(),
        Task::WhenComplete { .. } => "WhenComplete".into(),
        Task::StreamEventSub { .. } => "StreamEventSub".into(),
        Task::StreamTick { .. } => "StreamTick".into(),
        Task::StreamOpResume { .. } => "StreamOpResume".into(),
        Task::StreamEmit { .. } => "StreamEmit".into(),
        #[cfg(feature = "io")]
        Task::ProcessPoll { .. } => "ProcessPoll".into(),
        #[cfg(feature = "io")]
        Task::SocketConnectPoll { .. } => "SocketConnectPoll".into(),
        #[cfg(feature = "io")]
        Task::SocketPoll { .. } => "SocketPoll".into(),
        #[cfg(feature = "io")]
        Task::UdpPoll { .. } => "UdpPoll".into(),
        #[cfg(feature = "io")]
        Task::ServerPoll { .. } => "ServerPoll".into(),
        #[cfg(feature = "io")]
        Task::HttpClientPoll { .. } => "HttpClientPoll".into(),
        Task::PortEvent { .. } => "PortEvent".into(),
        #[cfg(feature = "io")]
        Task::WatchPoll { .. } => "WatchPoll".into(),
    }
}

/// Debug helper: stringifies the current task queue.
pub fn debug_dump_tasks() -> Vec<String> {
    crate::state::current()
        .tasks
        .borrow()
        .iter()
        .map(debug_dump_one)
        .collect()
}
