//! dart:isolate: Capability, IsolateSpawnException, Isolate, SendPort,
//! ReceivePort, RawReceivePort, RemoteError, TransferableTypedData.
//!
//! All ports are single-threaded: messages are delivered through the
//! event loop's task queue, so the port registries live in thread-local
//! storage (the interpreter is single-threaded).
//!
//! Native representations:
//! - `Capability`: `Value::Null`.
//! - `IsolateSpawnException`: `[Str(message)]`.
//! - `Isolate`: `[SendPort(control), Null(pauseCapability),
//!   Null(terminateCapability), Null(debugName)]`.
//! - `SendPort`: `[Int(port_id)]`.
//! - `ReceivePort`: `[Int(id), ctrl, stream]` (a StreamController stream).
//! - `RawReceivePort`: `[Int(id), handler-or-Null, Bool(keepAlive)]`.
//! - `RemoteError`: `[Str(description), Str(stack)]`.
//! - `TransferableTypedData`: `[List(byte-lists...)]`.

use crate::class::BridgedClassDef;
use crate::env::Environment;
use crate::interp::InterpreterVisitor;
use crate::stdlib::helpers as h;
use crate::stdlib::stream_lib as sl;
use crate::value::Value;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};

fn new_id() -> i64 {
    static NEXT: AtomicI64 = AtomicI64::new(1);
    NEXT.fetch_add(1, Ordering::SeqCst)
}

fn wrap(name: &str, native: Value) -> Value {
    h::wrap(&h::class(name), native)
}

fn future_ok(value: Value) -> Value {
    let st = Rc::new(RefCell::new(crate::async_state::FutureState::new()));
    crate::async_state::complete(&st, Ok(value));
    wrap_future(st)
}

fn wrap_future(state: Rc<RefCell<crate::async_state::FutureState>>) -> Value {
    wrap("Future", Value::Future(state))
}

fn wrap_send_port(port_id: i64) -> Value {
    wrap(
        "SendPort",
        Value::List(Rc::new(RefCell::new(vec![Value::Int(port_id)]))),
    )
}

/// Delivers a message to the ReceivePort/RawReceivePort with `port_id`.
fn port_send(
    visitor: &mut InterpreterVisitor,
    port_id: i64,
    message: Value,
) -> Result<(), crate::error::InterpError> {
    crate::permission::require_isolate()?;
    let state = crate::state::current();
    if let Some(ctrl) = state.receive_ports.borrow().get(&port_id).cloned() {
        crate::async_state::enqueue_task(crate::async_state::Task::PortEvent {
            ctrl,
            message: message.clone(),
        });
        return Ok(());
    }
    if let Some((handler, _)) = state.raw_ports.borrow().get(&port_id).cloned()
        && !handler.is_null()
    {
        let _ = visitor.call_value(&handler, vec![message], HashMap::new());
    }
    Ok(())
}

fn make_receive_port(debug_name: &str) -> Result<Value, crate::error::InterpError> {
    crate::permission::require_isolate()?;
    let _ = debug_name;
    let id = new_id();
    let ctrl = sl::make_controller_value(false, false);
    let stream = sl::controller_stream_value(&ctrl);
    crate::state::current()
        .receive_ports
        .borrow_mut()
        .insert(id, ctrl.clone());
    Ok(wrap(
        "ReceivePort",
        Value::List(Rc::new(RefCell::new(vec![Value::Int(id), ctrl, stream]))),
    ))
}

fn make_raw_port(handler: Value) -> Result<Value, crate::error::InterpError> {
    crate::permission::require_isolate()?;
    let id = new_id();
    let keep_alive = Rc::new(RefCell::new(true));
    crate::state::current()
        .raw_ports
        .borrow_mut()
        .insert(id, (handler, keep_alive.clone()));
    Ok(wrap(
        "RawReceivePort",
        Value::List(Rc::new(RefCell::new(vec![
            Value::Int(id),
            Value::Null,
            Value::Bool(true),
        ]))),
    ))
}

fn capability_definition() -> BridgedClassDef {
    let mut bc = BridgedClassDef::new("Capability");
    bc.native_type = Some("d4rt::isolate::Capability".to_string());
    bc.constructors.insert(
        "".to_string(),
        h::ctor(|_v, _args, _n| Ok(wrap("Capability", Value::Null))),
    );
    bc
}

fn isolate_spawn_exception_definition() -> BridgedClassDef {
    let mut bc = BridgedClassDef::new("IsolateSpawnException");
    bc.native_type = Some("d4rt::isolate::IsolateSpawnException".to_string());
    bc.constructors.insert(
        "".to_string(),
        h::ctor(|_v, args, _n| {
            let message = args.first().map(h::display).unwrap_or_default();
            Ok(wrap(
                "IsolateSpawnException",
                Value::List(Rc::new(RefCell::new(vec![Value::str(&message)]))),
            ))
        }),
    );
    bc.getters.insert(
        "message".to_string(),
        h::getter(|_v, recv| Ok(list_get(&recv, 0).unwrap_or(Value::Null))),
    );
    bc.methods.insert(
        "toString".to_string(),
        h::method(|_v, recv, _args, _n| {
            let message = list_get(&recv, 0)
                .map(|v| h::display(&v))
                .unwrap_or_default();
            Ok(Value::str(format!("IsolateSpawnException: {message}")))
        }),
    );
    bc
}

fn send_port_definition() -> BridgedClassDef {
    let mut bc = BridgedClassDef::new("SendPort");
    bc.native_type = Some("d4rt::isolate::SendPort".to_string());
    bc.methods.insert(
        "send".to_string(),
        h::method(|visitor, recv, args, _n| {
            let port_id = list_get(&recv, 0).and_then(|i| h::as_int(&i)).unwrap_or(0);
            let message = args.first().cloned().unwrap_or(Value::Null);
            port_send(visitor, port_id, message)?;
            Ok(Value::Null)
        }),
    );
    bc
}

fn receive_port_definition() -> BridgedClassDef {
    let mut bc = BridgedClassDef::new("ReceivePort");
    bc.native_type = Some("d4rt::isolate::ReceivePort".to_string());
    bc.constructors.insert(
        "".to_string(),
        h::ctor(|_v, args, _n| {
            let debug_name = args
                .first()
                .map(|v| h::as_str(v).unwrap_or("").to_string())
                .unwrap_or_default();
            make_receive_port(&debug_name)
        }),
    );
    bc.constructors.insert(
        "fromRawReceivePort".to_string(),
        h::ctor(|_v, args, _n| {
            let raw = args.first().cloned().unwrap_or(Value::Null);
            let id = list_get(&raw, 0).and_then(|i| h::as_int(&i)).unwrap_or(0);
            let taken = crate::state::current()
                .receive_ports
                .borrow()
                .contains_key(&id);
            if taken {
                return Err(h::err(
                    "ReceivePort.fromRawReceivePort: port already has a listener",
                ));
            }
            let ctrl = sl::make_controller_value(false, false);
            let stream = sl::controller_stream_value(&ctrl);
            let state = crate::state::current();
            state.receive_ports.borrow_mut().insert(id, ctrl.clone());
            state.raw_ports.borrow_mut().remove(&id);
            Ok(wrap(
                "ReceivePort",
                Value::List(Rc::new(RefCell::new(vec![Value::Int(id), ctrl, stream]))),
            ))
        }),
    );
    bc.getters.insert(
        "sendPort".to_string(),
        h::getter(|_v, recv| {
            let id = list_get(&recv, 0).and_then(|i| h::as_int(&i)).unwrap_or(0);
            Ok(wrap_send_port(id))
        }),
    );
    bc.getters.insert(
        "isBroadcast".to_string(),
        h::getter(|_v, _r| Ok(Value::Bool(true))),
    );
    bc.methods.insert(
        "close".to_string(),
        h::method(|visitor, recv, _a, _n| {
            if let Some(ctrl) = list_get(&recv, 1) {
                let _ = sl::controller_close_value(visitor, &ctrl);
            }
            if let Some(id) = list_get(&recv, 0).and_then(|i| h::as_int(&i)) {
                crate::state::current()
                    .receive_ports
                    .borrow_mut()
                    .remove(&id);
            }
            Ok(Value::Null)
        }),
    );
    sl::delegate_stream(&mut bc, 2);
    bc
}

fn raw_receive_port_definition() -> BridgedClassDef {
    let mut bc = BridgedClassDef::new("RawReceivePort");
    bc.native_type = Some("d4rt::isolate::RawReceivePort".to_string());
    bc.constructors.insert(
        "".to_string(),
        h::ctor(|_v, args, _n| {
            let handler = args.first().cloned().unwrap_or(Value::Null);
            make_raw_port(handler)
        }),
    );
    bc.getters.insert(
        "sendPort".to_string(),
        h::getter(|_v, recv| {
            let id = list_get(&recv, 0).and_then(|i| h::as_int(&i)).unwrap_or(0);
            Ok(wrap_send_port(id))
        }),
    );
    bc.getters.insert(
        "keepIsolateAlive".to_string(),
        h::getter(|_v, recv| {
            let id = list_get(&recv, 0).and_then(|i| h::as_int(&i)).unwrap_or(0);
            let keep = crate::state::current()
                .raw_ports
                .borrow()
                .get(&id)
                .map(|(_, k)| *k.borrow())
                .unwrap_or(false);
            Ok(Value::Bool(keep))
        }),
    );
    bc.setters.insert(
        "keepIsolateAlive".to_string(),
        h::setter(|_v, recv, val| {
            let keep = h::as_bool(&val).unwrap_or(false);
            if let Some(id) = list_get(&recv, 0).and_then(|i| h::as_int(&i)) {
                let state = crate::state::current();
                if let Some((_, k)) = state.raw_ports.borrow().get(&id) {
                    *k.borrow_mut() = keep;
                }
            }
            Ok(())
        }),
    );
    bc.setters.insert(
        "handler".to_string(),
        h::setter(|_v, recv, val| {
            if let Some(id) = list_get(&recv, 0).and_then(|i| h::as_int(&i)) {
                let state = crate::state::current();
                if let Some((handler, _)) = state.raw_ports.borrow_mut().get_mut(&id) {
                    *handler = val;
                }
            }
            Ok(())
        }),
    );
    bc.methods.insert(
        "close".to_string(),
        h::method(|_v, recv, _a, _n| {
            if let Some(id) = list_get(&recv, 0).and_then(|i| h::as_int(&i)) {
                crate::state::current().raw_ports.borrow_mut().remove(&id);
            }
            Ok(Value::Null)
        }),
    );
    bc
}

fn remote_error_definition() -> BridgedClassDef {
    let mut bc = BridgedClassDef::new("RemoteError");
    bc.native_type = Some("d4rt::isolate::RemoteError".to_string());
    bc.constructors.insert(
        "".to_string(),
        h::ctor(|_v, args, _n| {
            let description = args.first().map(h::display).unwrap_or_default();
            let stack = args.get(1).map(h::display).unwrap_or_default();
            Ok(wrap(
                "RemoteError",
                Value::List(Rc::new(RefCell::new(vec![
                    Value::str(&description),
                    Value::str(&stack),
                ]))),
            ))
        }),
    );
    bc.getters.insert(
        "stackTrace".to_string(),
        h::getter(|_v, recv| Ok(list_get(&recv, 1).unwrap_or(Value::Null))),
    );
    bc.methods.insert(
        "toString".to_string(),
        h::method(|_v, recv, _args, _n| {
            let description = list_get(&recv, 0)
                .map(|v| h::display(&v))
                .unwrap_or_default();
            Ok(Value::str(format!("RemoteError: {description}")))
        }),
    );
    bc
}

/// Extracts bytes from a Uint8List native (`[bytes, offset, length]`) or a
/// plain list of ints.
fn typed_data_bytes(v: &Value) -> Vec<u8> {
    let list = h::as_list(v)
        .map(|l| l.borrow().clone())
        .unwrap_or_default();
    let (bytes, offset) = match list.first() {
        Some(inner) if h::as_list(inner).is_some() => {
            let b = h::as_list(inner)
                .map(|l| l.borrow().clone())
                .unwrap_or_default();
            let off = list.get(1).and_then(h::as_int).unwrap_or(0) as usize;
            (b, off)
        }
        _ => (list, 0),
    };
    bytes
        .iter()
        .skip(offset)
        .filter_map(|b| h::as_int(b).map(|i| i as u8))
        .collect()
}

fn transferable_typed_data_definition() -> BridgedClassDef {
    let mut bc = BridgedClassDef::new("TransferableTypedData");
    bc.native_type = Some("d4rt::isolate::TransferableTypedData".to_string());
    bc.constructors.insert(
        "fromList".to_string(),
        h::ctor(|_v, args, _n| {
            let list = args
                .first()
                .and_then(h::as_list)
                .map(|l| l.borrow().clone())
                .unwrap_or_default();
            let byte_lists: Vec<Value> = list
                .iter()
                .map(|item| {
                    let bytes: Vec<Value> = typed_data_bytes(item)
                        .into_iter()
                        .map(|b| Value::Int(b as i64))
                        .collect();
                    Value::List(Rc::new(RefCell::new(bytes)))
                })
                .collect();
            Ok(wrap(
                "TransferableTypedData",
                Value::List(Rc::new(RefCell::new(vec![Value::List(Rc::new(
                    RefCell::new(byte_lists),
                ))]))),
            ))
        }),
    );
    bc.methods.insert(
        "materialize".to_string(),
        h::method(|_v, recv, _a, _n| {
            let chunks = list_get(&recv, 0)
                .and_then(|c| h::as_list(&c))
                .map(|l| l.borrow().clone())
                .unwrap_or_default();
            let mut all: Vec<Value> = Vec::new();
            for chunk in &chunks {
                if let Some(l) = h::as_list(chunk) {
                    all.extend(l.borrow().clone());
                }
            }
            let len = all.len();
            let bytes = Rc::new(RefCell::new(all));
            let native = Value::List(Rc::new(RefCell::new(vec![
                Value::List(bytes),
                Value::Int(0),
                Value::Int(len as i64),
            ])));
            Ok(h::wrap(&h::class("Uint8List"), native))
        }),
    );
    bc
}

fn list_get(v: &Value, index: usize) -> Option<Value> {
    h::as_list(v).and_then(|l| l.borrow().get(index).cloned())
}

pub fn load(env: &Rc<RefCell<Environment>>) {
    for bc in [
        capability_definition(),
        isolate_spawn_exception_definition(),
        isolate_definition(),
        send_port_definition(),
        receive_port_definition(),
        raw_receive_port_definition(),
        remote_error_definition(),
        transferable_typed_data_definition(),
    ] {
        let bc = Arc::new(bc);
        h::register_class(bc.clone());
        Environment::define_bridge(env, bc);
    }
}

fn isolate_cell(v: &Value, index: usize) -> Option<Value> {
    h::as_list(v).and_then(|l| l.borrow().get(index).cloned())
}

fn set_isolate_cell(v: &Value, index: usize, value: Value) {
    if let Some(l) = h::as_list(v) {
        let mut c = l.borrow_mut();
        if c.len() > index {
            c[index] = value;
        }
    }
}

fn pending_future() -> Value {
    wrap(
        "Future",
        Value::Future(Rc::new(
            RefCell::new(crate::async_state::FutureState::new()),
        )),
    )
}

fn isolate_control(control: Value, debug_name: &str) -> Value {
    Value::List(Rc::new(RefCell::new(vec![
        control,                                        // 0 control port
        Value::str(debug_name),                         // 1 debug name
        Value::Bool(false),                             // 2 paused
        Value::Bool(true),                              // 3 errorsFatal
        Value::Null,                                    // 4 pause capability
        Value::Null,                                    // 5 terminate capability
        Value::List(Rc::new(RefCell::new(Vec::new()))), // 6 exit listeners
        Value::List(Rc::new(RefCell::new(Vec::new()))), // 7 error listeners
        Value::Null,                                    // 8 pause future
        Value::Bool(false),                             // 9 killed
    ])))
}

fn isolate_native() -> Value {
    isolate_control(wrap_send_port(0), "")
}

/// Fires the isolate's registered on-exit listeners, sending each its
/// registered response message.
fn isolate_fire_exit(visitor: &mut InterpreterVisitor, recv: &Value) {
    let listeners = isolate_cell(recv, 6).and_then(|x| h::as_list(&x));
    if let Some(listeners) = listeners {
        let items: Vec<Value> = listeners.borrow().clone();
        for item in items {
            if let (Some(port), Some(response)) = (isolate_cell(&item, 0), isolate_cell(&item, 1))
                && let Some(port_id) = isolate_cell(&port, 0).and_then(|i| h::as_int(&i))
            {
                let _ = port_send(visitor, port_id, response);
            }
        }
    }
}

/// Removes a SendPort entry from an isolate listener list (cell `index`).
fn isolate_remove_port(v: &Value, index: usize, port: &Value) {
    let port_id = isolate_cell(port, 0).and_then(|i| h::as_int(&i));
    if let Some(listeners) = isolate_cell(v, index).and_then(|x| h::as_list(&x)) {
        let mut ls = listeners.borrow_mut();
        ls.retain(|entry| {
            let entry_id = isolate_cell(entry, 0)
                .and_then(|x| isolate_cell(&x, 0).and_then(|i| h::as_int(&i)));
            match (port_id, entry_id) {
                (Some(a), Some(b)) => a != b,
                _ => true,
            }
        });
    }
}

fn isolate_definition() -> BridgedClassDef {
    let mut bc = BridgedClassDef::new("Isolate");
    bc.native_type = Some("d4rt::isolate::Isolate".to_string());
    bc.constructors.insert(
        "".to_string(),
        h::ctor(|_v, args, named| {
            let control = args.first().cloned().unwrap_or_else(|| wrap_send_port(0));
            let debug_name = named
                .get("debugName")
                .and_then(|n| h::as_str(n))
                .map(|s| s.to_string())
                .unwrap_or_default();
            let native = isolate_control(control, &debug_name);
            let pause_cap = named.get("pauseCapability").cloned();
            let term_cap = named.get("terminateCapability").cloned();
            if let Some(c) = pause_cap {
                set_isolate_cell(&native, 4, c);
            }
            if let Some(c) = term_cap {
                set_isolate_cell(&native, 5, c);
            }
            Ok(wrap("Isolate", native))
        }),
    );
    bc.static_methods.insert(
        "run".to_string(),
        h::static_method(|visitor, args, _n| {
            let computation = args.first().cloned().unwrap_or(Value::Null);
            crate::permission::require_isolate()?;
            match visitor.call_value(&computation, vec![], HashMap::new()) {
                Ok(v) => {
                    if crate::async_state::unwrap_future(&v).is_some() {
                        Ok(v)
                    } else {
                        Ok(future_ok(v))
                    }
                }
                Err(e) => Ok(future_err(crate::async_state::exception_value(e))),
            }
        }),
    );
    bc.static_methods.insert(
        "spawn".to_string(),
        h::static_method(|visitor, args, named| {
            let entry = args.first().cloned().unwrap_or(Value::Null);
            let message = args.get(1).cloned().unwrap_or(Value::Null);
            crate::permission::require_isolate()?;
            let _ = visitor.call_value(&entry, vec![message], HashMap::new());
            let debug_name = named
                .get("debugName")
                .and_then(|n| h::as_str(n))
                .map(|s| s.to_string())
                .unwrap_or_default();
            Ok(future_ok(wrap(
                "Isolate",
                isolate_control(wrap_send_port(0), &debug_name),
            )))
        }),
    );
    bc.static_methods.insert(
        "exit".to_string(),
        h::static_method(|visitor, args, _n| {
            if let Some(port) = args.first()
                && let Some(port_id) = list_get(port, 0).and_then(|i| h::as_int(&i))
            {
                let message = args.get(1).cloned().unwrap_or(Value::Null);
                port_send(visitor, port_id, message)?;
            }
            Ok(Value::Null)
        }),
    );
    bc.static_methods.insert(
        "resolvePackageUri".to_string(),
        h::static_method(|_v, _args, _n| Ok(future_ok(Value::Null))),
    );
    bc.static_methods.insert(
        "resolvePackageUriSync".to_string(),
        h::static_method(|_v, _args, _n| Ok(Value::Null)),
    );
    bc.static_getters.insert(
        "current".to_string(),
        h::static_getter(|_v| Ok(wrap("Isolate", isolate_native()))),
    );
    bc.static_getters.insert(
        "packageConfig".to_string(),
        h::static_getter(|_v| Ok(future_ok(Value::Null))),
    );
    bc.static_getters.insert(
        "packageConfigSync".to_string(),
        h::static_getter(|_v| Ok(Value::Null)),
    );
    bc.static_getters.insert(
        "immediate".to_string(),
        h::static_getter(|_v| Ok(Value::Int(1))),
    );
    bc.static_getters.insert(
        "beforeNextEvent".to_string(),
        h::static_getter(|_v| Ok(Value::Int(0))),
    );
    bc.getters.insert(
        "controlPort".to_string(),
        h::getter(|_v, recv| Ok(list_get(&recv, 0).unwrap_or(Value::Null))),
    );
    bc.getters.insert(
        "pauseCapability".to_string(),
        h::getter(|_v, recv| Ok(isolate_cell(&recv, 4).unwrap_or(Value::Null))),
    );
    bc.getters.insert(
        "terminateCapability".to_string(),
        h::getter(|_v, recv| Ok(isolate_cell(&recv, 5).unwrap_or(Value::Null))),
    );
    bc.getters.insert(
        "debugName".to_string(),
        h::getter(|_v, recv| Ok(isolate_cell(&recv, 1).unwrap_or(Value::Null))),
    );
    bc.methods.insert(
        "pause".to_string(),
        h::method(|_v, recv, _a, _n| {
            let f = pending_future();
            set_isolate_cell(&recv, 2, Value::Bool(true));
            set_isolate_cell(&recv, 8, f.clone());
            Ok(f)
        }),
    );
    bc.methods.insert(
        "resume".to_string(),
        h::method(|_v, recv, args, _n| {
            let cap = args.first().cloned().unwrap_or(Value::Null);
            let f = isolate_cell(&recv, 8).unwrap_or(Value::Null);
            set_isolate_cell(&recv, 2, Value::Bool(false));
            set_isolate_cell(&recv, 8, Value::Null);
            if let Some(st) = crate::async_state::unwrap_future(&f) {
                crate::async_state::complete(&st, Ok(cap));
            }
            Ok(Value::Null)
        }),
    );
    bc.methods.insert(
        "addOnExitListener".to_string(),
        h::method(|_v, recv, args, named| {
            if let Some(port) = args.first()
                && let Some(listeners) = isolate_cell(&recv, 6).and_then(|x| h::as_list(&x))
            {
                let response = named.get("response").cloned().unwrap_or(Value::Null);
                listeners
                    .borrow_mut()
                    .push(Value::List(Rc::new(RefCell::new(vec![
                        port.clone(),
                        response,
                    ]))));
            }
            Ok(Value::Null)
        }),
    );
    bc.methods.insert(
        "removeOnExitListener".to_string(),
        h::method(|_v, recv, args, _n| {
            if let Some(port) = args.first() {
                isolate_remove_port(&recv, 6, port);
            }
            Ok(Value::Null)
        }),
    );
    bc.methods.insert(
        "setErrorsFatal".to_string(),
        h::method(|_v, recv, args, _n| {
            let errors_fatal = args.first().and_then(h::as_bool).unwrap_or(true);
            set_isolate_cell(&recv, 3, Value::Bool(errors_fatal));
            Ok(Value::Null)
        }),
    );
    bc.methods.insert(
        "kill".to_string(),
        h::method(|visitor, recv, _a, _n| {
            set_isolate_cell(&recv, 9, Value::Bool(true));
            let f = isolate_cell(&recv, 8).unwrap_or(Value::Null);
            set_isolate_cell(&recv, 8, Value::Null);
            if let Some(st) = crate::async_state::unwrap_future(&f) {
                crate::async_state::complete(&st, Err(Value::Null));
            }
            isolate_fire_exit(visitor, &recv);
            Ok(Value::Null)
        }),
    );
    bc.methods.insert(
        "ping".to_string(),
        h::method(|visitor, recv, args, named| {
            let _ = recv;
            if let Some(port) = args.first()
                && let Some(port_id) = list_get(port, 0).and_then(|i| h::as_int(&i))
            {
                let response = named.get("response").cloned().unwrap_or(Value::Null);
                port_send(visitor, port_id, response)?;
            }
            Ok(Value::Null)
        }),
    );
    bc.methods.insert(
        "addErrorListener".to_string(),
        h::method(|_v, recv, args, _n| {
            if let Some(port) = args.first()
                && let Some(listeners) = isolate_cell(&recv, 7).and_then(|x| h::as_list(&x))
            {
                listeners.borrow_mut().push(port.clone());
            }
            Ok(Value::Null)
        }),
    );
    bc.methods.insert(
        "removeErrorListener".to_string(),
        h::method(|_v, recv, args, _n| {
            if let Some(port) = args.first() {
                isolate_remove_port(&recv, 7, port);
            }
            Ok(Value::Null)
        }),
    );
    bc
}

fn future_err(error: Value) -> Value {
    let st = Rc::new(RefCell::new(crate::async_state::FutureState::new()));
    crate::async_state::complete(&st, Err(error));
    wrap_future(st)
}
