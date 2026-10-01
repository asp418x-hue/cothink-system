//! dart:async: Future, Completer, Timer and Duration.
//!
//! Native representations:
//! - `Future` instance: `Value::Future(state)` (wrapped so instance
//!   methods like `.then` are available)
//! - `Completer` instance: `[Value::Future(state)]`
//! - `Timer` instance: `null`
//! - `Duration` instance: `Value::Int(microseconds)`

use super::helpers as h;
use crate::async_state;
use crate::callable::NativeFunction;
use crate::class::BridgedClassDef;
use crate::env::Environment;
use crate::error::InterpError;
use crate::rt::FunctionRuntimeType;
use crate::value::Value;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

fn define_bridge(env: &Rc<RefCell<Environment>>, bc: BridgedClassDef) {
    let bc = Arc::new(bc);
    h::register_class(bc.clone());
    Environment::define_bridge(env, bc);
}

pub fn load(env: &Rc<RefCell<Environment>>) {
    define_bridge(env, future_definition());
    define_bridge(env, completer_definition());
    define_bridge(env, timer_definition());
    define_bridge(env, timeout_exception_definition());
    Environment::define(
        env,
        "unawaited",
        Value::NativeFn(Rc::new(NativeFunction {
            name: Some("unawaited".to_string()),
            min_arity: 0,
            max_arity: None,
            is_async: false,
            is_generator: false,
            closure: Box::new(|_v, _args, _named| Ok(Value::Null)),
            callable_runtime_type: FunctionRuntimeType::untyped(),
        })),
    );
}

fn wrap_future(state: Rc<RefCell<async_state::FutureState>>) -> Value {
    h::wrap(&h::class("Future"), Value::Future(state))
}

fn unwrap_state(recv: &Value) -> Result<Rc<RefCell<async_state::FutureState>>, InterpError> {
    async_state::unwrap_future(recv).ok_or_else(|| InterpError::runtime("Expected a Future"))
}

fn call_or_error(
    visitor: &mut crate::interp::InterpreterVisitor,
    callee: &Value,
    args: Vec<Value>,
) -> Value {
    match visitor.call_value(callee, args, HashMap::new()) {
        Ok(v) => v,
        Err(InterpError::Throw(v)) => v,
        Err(e) => Value::str(e.to_string()),
    }
}

fn future_definition() -> BridgedClassDef {
    let mut bc = BridgedClassDef::new("Future");
    bc.native_type = Some("Future".to_string());
    bc.constructors.insert(
        "".to_string(),
        h::ctor(|visitor, args, _n| {
            let state = Rc::new(RefCell::new(async_state::FutureState::new()));
            let value = match args.first() {
                Some(c) => call_or_error(visitor, c, vec![]),
                None => Value::Null,
            };
            async_state::enqueue_task(async_state::Task::Complete {
                future: state.clone(),
                value,
                delay_us: 0,
            });
            Ok(wrap_future(state))
        }),
    );
    bc.static_methods.insert(
        "value".to_string(),
        h::static_method(|_v, args, _n| {
            let state = Rc::new(RefCell::new(async_state::FutureState::new()));
            let value = args.first().cloned().unwrap_or(Value::Null);
            async_state::complete(&state, Ok(value));
            Ok(wrap_future(state))
        }),
    );
    bc.static_methods.insert(
        "wait".to_string(),
        h::static_method(|_v, args, _n| {
            let wait_state = Rc::new(RefCell::new(async_state::FutureState::new()));
            let items: Vec<Value> = match args.first().cloned().unwrap_or(Value::Null) {
                Value::List(l) => l.borrow().clone(),
                Value::Set(s) => s.borrow().items.clone(),
                other => vec![other],
            };
            let n = items.len();
            if n == 0 {
                async_state::complete(
                    &wait_state,
                    Ok(Value::List(Rc::new(RefCell::new(Vec::new())))),
                );
                return Ok(wrap_future(wait_state));
            }
            let results: Rc<RefCell<Vec<Option<Value>>>> = Rc::new(RefCell::new(vec![None; n]));
            let remaining: Rc<RefCell<usize>> = Rc::new(RefCell::new(n));
            for (i, item) in items.into_iter().enumerate() {
                if let Some(inner) = async_state::unwrap_future(&item) {
                    let mut st = inner.borrow_mut();
                    if st.completed {
                        let result = async_state::future_result(&st);
                        drop(st);
                        apply_wait_result(result, i, &results, &remaining, &wait_state);
                    } else {
                        st.continuations.push(async_state::Continuation::Then {
                            callback: Some(wait_value_callback(
                                i,
                                results.clone(),
                                remaining.clone(),
                                wait_state.clone(),
                            )),
                            on_error: Some(wait_error_callback(wait_state.clone())),
                            then_future: Rc::new(RefCell::new(async_state::FutureState::new())),
                        });
                    }
                } else {
                    apply_wait_result(Ok(item), i, &results, &remaining, &wait_state);
                }
            }
            Ok(wrap_future(wait_state))
        }),
    );
    bc.static_methods.insert(
        "any".to_string(),
        h::static_method(|_v, args, _n| {
            let state = Rc::new(RefCell::new(async_state::FutureState::new()));
            let items: Vec<Value> = match args.first().cloned().unwrap_or(Value::Null) {
                Value::List(l) => l.borrow().clone(),
                Value::Set(s) => s.borrow().items.clone(),
                other => vec![other],
            };
            if items.is_empty() {
                async_state::complete(&state, Err(Value::str("Bad state: No element")));
                return Ok(wrap_future(state));
            }
            let remaining = Rc::new(RefCell::new(items.len()));
            let last_error: Rc<RefCell<Option<Value>>> = Rc::new(RefCell::new(None));
            for item in items {
                if let Some(inner) = async_state::unwrap_future(&item) {
                    let mut st = inner.borrow_mut();
                    if st.completed {
                        let result = async_state::future_result(&st);
                        drop(st);
                        any_apply(result, &state, &remaining, &last_error);
                    } else {
                        st.continuations.push(async_state::Continuation::Then {
                            callback: Some(any_value_callback(state.clone())),
                            on_error: Some(any_error_callback(
                                state.clone(),
                                remaining.clone(),
                                last_error.clone(),
                            )),
                            then_future: Rc::new(RefCell::new(async_state::FutureState::new())),
                        });
                    }
                } else {
                    async_state::complete(&state, Ok(item));
                }
            }
            Ok(wrap_future(state))
        }),
    );
    bc.static_methods.insert(
        "forEach".to_string(),
        h::static_method(|_v, args, _n| {
            let state = Rc::new(RefCell::new(async_state::FutureState::new()));
            let items: Vec<Value> = match args.first().cloned().unwrap_or(Value::Null) {
                Value::List(l) => l.borrow().clone(),
                Value::Set(s) => s.borrow().items.clone(),
                other => vec![other],
            };
            let action = args.get(1).cloned().unwrap_or(Value::Null);
            let index: Rc<RefCell<usize>> = Rc::new(RefCell::new(0));
            let items_rc = Rc::new(RefCell::new(items));
            let state_cb = state.clone();
            let error_cb = loop_error_callback(state.clone());
            let error_cb_outer = error_cb.clone();
            let holder: Rc<RefCell<Option<Value>>> = Rc::new(RefCell::new(None));
            let h2 = holder.clone();
            let runner = Value::NativeFn(Rc::new(NativeFunction {
                name: Some("<Future.forEach loop>".to_string()),
                min_arity: 0,
                max_arity: None,
                is_async: false,
                is_generator: false,
                closure: Box::new(move |visitor, _args, _n| {
                    loop {
                        let i = {
                            let mut idx = index.borrow_mut();
                            let i = *idx;
                            *idx += 1;
                            i
                        };
                        if i >= items_rc.borrow().len() {
                            async_state::complete(&state_cb, Ok(Value::Null));
                            return Ok(Value::Null);
                        }
                        let item = items_rc.borrow()[i].clone();
                        match visitor.call_value(&action, vec![item], HashMap::new()) {
                            Ok(v) => {
                                if let Some(inner) = async_state::unwrap_future(&v) {
                                    let mut st = inner.borrow_mut();
                                    if st.completed {
                                        let res = async_state::future_result(&st);
                                        drop(st);
                                        if let Err(e) = res {
                                            async_state::complete(&state_cb, Err(e));
                                            return Ok(Value::Null);
                                        }
                                    } else {
                                        st.continuations.push(async_state::Continuation::Then {
                                            callback: h2.borrow().clone(),
                                            on_error: Some(error_cb.clone()),
                                            then_future: Rc::new(RefCell::new(
                                                async_state::FutureState::new(),
                                            )),
                                        });
                                        return Ok(Value::Null);
                                    }
                                }
                            }
                            Err(InterpError::Throw(e)) => {
                                async_state::complete(&state_cb, Err(e));
                                return Ok(Value::Null);
                            }
                            Err(e) => {
                                async_state::complete(&state_cb, Err(Value::str(e.to_string())));
                                return Ok(Value::Null);
                            }
                        }
                    }
                }),
                callable_runtime_type: FunctionRuntimeType::untyped(),
            }));
            *holder.borrow_mut() = Some(runner.clone());
            async_state::enqueue_task(async_state::Task::Then {
                callback: Some(runner),
                on_error: Some(error_cb_outer),
                then_future: Rc::new(RefCell::new(async_state::FutureState::new())),
                result: Ok(Value::Null),
            });
            Ok(wrap_future(state))
        }),
    );
    bc.static_methods.insert(
        "doWhile".to_string(),
        h::static_method(|_v, args, _n| {
            let state = Rc::new(RefCell::new(async_state::FutureState::new()));
            let action = args.first().cloned().unwrap_or(Value::Null);
            let state_cb = state.clone();
            let error_cb = loop_error_callback(state.clone());
            let error_cb_outer = error_cb.clone();
            let holder: Rc<RefCell<Option<Value>>> = Rc::new(RefCell::new(None));
            let h2 = holder.clone();
            let started: Rc<RefCell<bool>> = Rc::new(RefCell::new(false));
            let runner = Value::NativeFn(Rc::new(NativeFunction {
                name: Some("<Future.doWhile loop>".to_string()),
                min_arity: 0,
                max_arity: None,
                is_async: false,
                is_generator: false,
                closure: Box::new(move |visitor, args, _n| {
                    let mut cur: Option<Value> = args.first().cloned();
                    loop {
                        let first = {
                            let mut s = started.borrow_mut();
                            if *s {
                                false
                            } else {
                                *s = true;
                                true
                            }
                        };
                        if !first && let Some(prev) = cur.take() {
                            match crate::value::is_truthy(&prev) {
                                Ok(true) => {}
                                Ok(false) => {
                                    async_state::complete(&state_cb, Ok(Value::Null));
                                    return Ok(Value::Null);
                                }
                                Err(e) => {
                                    async_state::complete(
                                        &state_cb,
                                        Err(crate::async_state::exception_value(e)),
                                    );
                                    return Ok(Value::Null);
                                }
                            }
                        }
                        match visitor.call_value(&action, vec![], HashMap::new()) {
                            Ok(v) => match async_state::unwrap_future(&v) {
                                Some(inner) => {
                                    let mut st = inner.borrow_mut();
                                    if st.completed {
                                        let res = async_state::future_result(&st);
                                        drop(st);
                                        match res {
                                            Ok(val) => cur = Some(val),
                                            Err(e) => {
                                                async_state::complete(&state_cb, Err(e));
                                                return Ok(Value::Null);
                                            }
                                        }
                                    } else {
                                        st.continuations.push(async_state::Continuation::Then {
                                            callback: h2.borrow().clone(),
                                            on_error: Some(error_cb.clone()),
                                            then_future: Rc::new(RefCell::new(
                                                async_state::FutureState::new(),
                                            )),
                                        });
                                        return Ok(Value::Null);
                                    }
                                }
                                None => cur = Some(v),
                            },
                            Err(InterpError::Throw(e)) => {
                                async_state::complete(&state_cb, Err(e));
                                return Ok(Value::Null);
                            }
                            Err(e) => {
                                async_state::complete(&state_cb, Err(Value::str(e.to_string())));
                                return Ok(Value::Null);
                            }
                        }
                    }
                }),
                callable_runtime_type: FunctionRuntimeType::untyped(),
            }));
            *holder.borrow_mut() = Some(runner.clone());
            async_state::enqueue_task(async_state::Task::Then {
                callback: Some(runner),
                on_error: Some(error_cb_outer),
                then_future: Rc::new(RefCell::new(async_state::FutureState::new())),
                result: Ok(Value::Null),
            });
            Ok(wrap_future(state))
        }),
    );
    bc.static_methods.insert(
        "error".to_string(),
        h::static_method(|_v, args, _n| {
            let state = Rc::new(RefCell::new(async_state::FutureState::new()));
            let error = args.first().cloned().unwrap_or(Value::Null);
            async_state::complete(&state, Err(error));
            Ok(wrap_future(state))
        }),
    );
    bc.static_methods.insert(
        "delayed".to_string(),
        h::static_method(|visitor, args, _n| {
            let state = Rc::new(RefCell::new(async_state::FutureState::new()));
            let value = match args.get(1) {
                Some(c) => call_or_error(visitor, c, vec![]),
                None => Value::Null,
            };
            let delay_us = args
                .first()
                .cloned()
                .and_then(|d| h::as_int(&d))
                .unwrap_or(0);
            async_state::enqueue_task(async_state::Task::Complete {
                future: state.clone(),
                value,
                delay_us,
            });
            Ok(wrap_future(state))
        }),
    );
    for name in ["microtask", "sync"] {
        bc.static_methods.insert(
            name.to_string(),
            h::static_method(move |visitor, args, _n| {
                let state = Rc::new(RefCell::new(async_state::FutureState::new()));
                let value = match args.first() {
                    Some(c) => call_or_error(visitor, c, vec![]),
                    None => Value::Null,
                };
                async_state::enqueue_task(async_state::Task::Complete {
                    future: state.clone(),
                    value,
                    delay_us: 0,
                });
                Ok(wrap_future(state))
            }),
        );
    }
    bc.methods.insert(
        "then".to_string(),
        h::method(|_v, recv, args, _n| {
            let state = unwrap_state(&recv)?;
            let callback = args.first().cloned().unwrap_or(Value::Null);
            let on_error = args.get(1).cloned();
            let then_future = Rc::new(RefCell::new(async_state::FutureState::new()));
            let mut st = state.borrow_mut();
            if st.completed {
                let result = async_state::future_result(&st);
                drop(st);
                async_state::enqueue_task(async_state::Task::Then {
                    callback: Some(callback),
                    on_error,
                    then_future: then_future.clone(),
                    result,
                });
            } else {
                st.continuations.push(async_state::Continuation::Then {
                    callback: Some(callback),
                    on_error,
                    then_future: then_future.clone(),
                });
            }
            Ok(wrap_future(then_future))
        }),
    );
    bc.methods.insert(
        "catchError".to_string(),
        h::method(|_v, recv, args, _n| {
            let state = unwrap_state(&recv)?;
            let on_error = args.first().cloned();
            let then_future = Rc::new(RefCell::new(async_state::FutureState::new()));
            let mut st = state.borrow_mut();
            if st.completed {
                let result = async_state::future_result(&st);
                drop(st);
                async_state::enqueue_task(async_state::Task::Then {
                    callback: None,
                    on_error,
                    then_future: then_future.clone(),
                    result,
                });
            } else {
                st.continuations.push(async_state::Continuation::Then {
                    callback: None,
                    on_error,
                    then_future: then_future.clone(),
                });
            }
            Ok(wrap_future(then_future))
        }),
    );
    bc.methods.insert(
        "whenComplete".to_string(),
        h::method(|_v, recv, args, _n| {
            let state = unwrap_state(&recv)?;
            let callback = args.first().cloned().unwrap_or(Value::Null);
            let then_future = Rc::new(RefCell::new(async_state::FutureState::new()));
            let mut st = state.borrow_mut();
            if st.completed {
                let result = async_state::future_result(&st);
                drop(st);
                async_state::enqueue_task(async_state::Task::WhenComplete {
                    callback,
                    then_future: then_future.clone(),
                    result,
                });
            } else {
                st.continuations
                    .push(async_state::Continuation::WhenComplete {
                        callback,
                        then_future: then_future.clone(),
                    });
            }
            Ok(wrap_future(then_future))
        }),
    );
    bc.methods.insert(
        "timeout".to_string(),
        h::method(|_v, recv, args, named| {
            let source = unwrap_state(&recv)?;
            let delay_us = args
                .first()
                .cloned()
                .and_then(|d| h::as_int(&d))
                .unwrap_or(0);
            let on_timeout = named.get("onTimeout").cloned().unwrap_or(Value::Null);
            let target = Rc::new(RefCell::new(async_state::FutureState::new()));
            let error = h::wrap(
                &h::class("TimeoutException"),
                Value::List(Rc::new(RefCell::new(vec![
                    Value::str("Future not completed within the time limit"),
                    Value::Int(delay_us),
                ]))),
            );
            let mut st = source.borrow_mut();
            if st.completed {
                let result = async_state::future_result(&st);
                drop(st);
                async_state::enqueue_task(async_state::Task::Forward {
                    future: target.clone(),
                    result,
                });
            } else {
                st.continuations
                    .push(async_state::Continuation::Forward(target.clone()));
                drop(st);
                async_state::enqueue_task(async_state::Task::FutureTimeout {
                    source,
                    target: target.clone(),
                    delay_us,
                    on_timeout,
                    error,
                });
            }
            Ok(wrap_future(target))
        }),
    );
    bc.methods.insert(
        "onError".to_string(),
        h::method(|_v, recv, args, _n| {
            let state = unwrap_state(&recv)?;
            let on_error = args.first().cloned();
            let then_future = Rc::new(RefCell::new(async_state::FutureState::new()));
            let mut st = state.borrow_mut();
            if st.completed {
                let result = async_state::future_result(&st);
                drop(st);
                async_state::enqueue_task(async_state::Task::Then {
                    callback: None,
                    on_error,
                    then_future: then_future.clone(),
                    result,
                });
            } else {
                st.continuations.push(async_state::Continuation::Then {
                    callback: None,
                    on_error,
                    then_future: then_future.clone(),
                });
            }
            Ok(wrap_future(then_future))
        }),
    );
    bc.methods.insert(
        "asStream".to_string(),
        h::method(|_v, recv, _args, _n| Ok(crate::stdlib::stream_lib::stream_from_future(&recv))),
    );
    bc.methods.insert(
        "toString".to_string(),
        h::method(|_v, _recv, _args, _n| Ok(Value::str("Instance of 'Future'"))),
    );
    bc
}

fn wait_value_callback(
    index: usize,
    results: Rc<RefCell<Vec<Option<Value>>>>,
    remaining: Rc<RefCell<usize>>,
    wait_state: Rc<RefCell<async_state::FutureState>>,
) -> Value {
    Value::NativeFn(Rc::new(NativeFunction {
        name: Some("<Future.wait callback>".to_string()),
        min_arity: 1,
        max_arity: Some(1),
        is_async: false,
        is_generator: false,
        closure: Box::new(move |_v, args, _n| {
            let value = args.first().cloned().unwrap_or(Value::Null);
            apply_wait_result(Ok(value), index, &results, &remaining, &wait_state);
            Ok(Value::Null)
        }),
        callable_runtime_type: FunctionRuntimeType::untyped(),
    }))
}

fn wait_error_callback(wait_state: Rc<RefCell<async_state::FutureState>>) -> Value {
    Value::NativeFn(Rc::new(NativeFunction {
        name: Some("<Future.wait onError>".to_string()),
        min_arity: 1,
        max_arity: Some(1),
        is_async: false,
        is_generator: false,
        closure: Box::new(move |_v, args, _n| {
            let error = args.first().cloned().unwrap_or(Value::Null);
            async_state::complete(&wait_state, Err(error));
            Ok(Value::Null)
        }),
        callable_runtime_type: FunctionRuntimeType::untyped(),
    }))
}

fn apply_wait_result(
    result: Result<Value, Value>,
    index: usize,
    results: &Rc<RefCell<Vec<Option<Value>>>>,
    remaining: &Rc<RefCell<usize>>,
    wait_state: &Rc<RefCell<async_state::FutureState>>,
) {
    match result {
        Ok(value) => {
            results.borrow_mut()[index] = Some(value);
            let mut rem = remaining.borrow_mut();
            *rem -= 1;
            if *rem == 0 {
                drop(rem);
                let values: Vec<Value> = results
                    .borrow()
                    .iter()
                    .map(|r| r.clone().unwrap_or(Value::Null))
                    .collect();
                async_state::complete(wait_state, Ok(Value::List(Rc::new(RefCell::new(values)))));
            }
        }
        Err(error) => async_state::complete(wait_state, Err(error)),
    }
}

/// Applies one `Future.any` result: the first value wins; if every future
/// fails, the *last* error completes the result future.
fn any_apply(
    result: Result<Value, Value>,
    state: &Rc<RefCell<async_state::FutureState>>,
    remaining: &Rc<RefCell<usize>>,
    last_error: &Rc<RefCell<Option<Value>>>,
) {
    match result {
        Ok(value) => async_state::complete(state, Ok(value)),
        Err(error) => {
            *last_error.borrow_mut() = Some(error.clone());
            let mut rem = remaining.borrow_mut();
            *rem -= 1;
            if *rem == 0 {
                drop(rem);
                async_state::complete(state, Err(error));
            }
        }
    }
}

fn any_value_callback(state: Rc<RefCell<async_state::FutureState>>) -> Value {
    Value::NativeFn(Rc::new(NativeFunction {
        name: Some("<Future.any callback>".to_string()),
        min_arity: 1,
        max_arity: Some(1),
        is_async: false,
        is_generator: false,
        closure: Box::new(move |_v, args, _n| {
            let value = args.first().cloned().unwrap_or(Value::Null);
            async_state::complete(&state, Ok(value));
            Ok(Value::Null)
        }),
        callable_runtime_type: FunctionRuntimeType::untyped(),
    }))
}

fn any_error_callback(
    state: Rc<RefCell<async_state::FutureState>>,
    remaining: Rc<RefCell<usize>>,
    last_error: Rc<RefCell<Option<Value>>>,
) -> Value {
    Value::NativeFn(Rc::new(NativeFunction {
        name: Some("<Future.any onError>".to_string()),
        min_arity: 1,
        max_arity: Some(1),
        is_async: false,
        is_generator: false,
        closure: Box::new(move |_v, args, _n| {
            let error = args.first().cloned().unwrap_or(Value::Null);
            any_apply(Err(error), &state, &remaining, &last_error);
            Ok(Value::Null)
        }),
        callable_runtime_type: FunctionRuntimeType::untyped(),
    }))
}

fn loop_error_callback(state: Rc<RefCell<async_state::FutureState>>) -> Value {
    Value::NativeFn(Rc::new(NativeFunction {
        name: Some("<Future loop onError>".to_string()),
        min_arity: 1,
        max_arity: Some(1),
        is_async: false,
        is_generator: false,
        closure: Box::new(move |_v, args, _n| {
            let error = args.first().cloned().unwrap_or(Value::Null);
            async_state::complete(&state, Err(error));
            Ok(Value::Null)
        }),
        callable_runtime_type: FunctionRuntimeType::untyped(),
    }))
}

fn completer_definition() -> BridgedClassDef {
    let mut bc = BridgedClassDef::new("Completer");
    bc.constructors.insert(
        "".to_string(),
        h::ctor(|_v, _args, _named| {
            let state = Rc::new(RefCell::new(async_state::FutureState::new()));
            Ok(h::wrap(
                &h::class("Completer"),
                Value::List(Rc::new(RefCell::new(vec![Value::Future(state)]))),
            ))
        }),
    );
    bc.constructors.insert(
        "sync".to_string(),
        h::ctor(|_v, _args, _named| {
            let state = Rc::new(RefCell::new(async_state::FutureState::new()));
            Ok(h::wrap(
                &h::class("Completer"),
                Value::List(Rc::new(RefCell::new(vec![Value::Future(state)]))),
            ))
        }),
    );
    bc.getters.insert(
        "future".to_string(),
        h::getter(|_v, native| match &native {
            Value::List(l) => Ok(l.borrow()[0].clone()),
            _ => Err(InterpError::runtime("Invalid Completer")),
        }),
    );
    bc.getters.insert(
        "isCompleted".to_string(),
        h::getter(|_v, recv| {
            let state = completer_state(&recv)?;
            Ok(Value::Bool(state.borrow().completed))
        }),
    );
    bc.methods.insert(
        "complete".to_string(),
        h::method(|_v, recv, args, _n| {
            let state = completer_state(&recv)?;
            let value = args.first().cloned().unwrap_or(Value::Null);
            async_state::complete(&state, Ok(value));
            Ok(Value::Null)
        }),
    );
    bc.methods.insert(
        "completeError".to_string(),
        h::method(|_v, recv, args, _n| {
            let state = completer_state(&recv)?;
            let error = args.first().cloned().unwrap_or(Value::Null);
            async_state::complete(&state, Err(error));
            Ok(Value::Null)
        }),
    );
    bc
}

fn completer_state(recv: &Value) -> Result<Rc<RefCell<async_state::FutureState>>, InterpError> {
    match recv {
        Value::List(l) => {
            let inner = l.borrow();
            async_state::unwrap_future(inner.first().unwrap_or(&Value::Null))
                .ok_or_else(|| InterpError::runtime("Invalid Completer"))
        }
        _ => Err(InterpError::runtime("Invalid Completer")),
    }
}

fn timer_definition() -> BridgedClassDef {
    let mut bc = BridgedClassDef::new("Timer");
    bc.constructors.insert(
        "".to_string(),
        h::ctor(|_v, args, _n| {
            let callback = args.get(1).cloned().unwrap_or(Value::Null);
            let timer = make_timer();
            async_state::enqueue_task(async_state::Task::Timer {
                callback,
                timer: Some(timer.clone()),
            });
            Ok(timer)
        }),
    );
    bc.static_methods.insert(
        "run".to_string(),
        h::static_method(|_v, args, _n| {
            let callback = args.first().cloned().unwrap_or(Value::Null);
            async_state::enqueue_task(async_state::Task::Timer {
                callback,
                timer: None,
            });
            Ok(Value::Null)
        }),
    );
    bc.static_methods.insert(
        "periodic".to_string(),
        h::static_method(|_v, args, _n| {
            let callback = args.get(1).cloned().unwrap_or(Value::Null);
            let timer = make_timer();
            async_state::enqueue_task(async_state::Task::TimerPeriodic {
                timer: timer.clone(),
                callback,
            });
            Ok(timer)
        }),
    );
    bc.getters.insert(
        "isActive".to_string(),
        h::getter(|_v, recv| Ok(Value::Bool(timer_active_of(&recv)))),
    );
    bc.getters.insert(
        "tick".to_string(),
        h::getter(|_v, recv| {
            let tick = match &recv {
                Value::List(l) => {
                    h::as_int(&l.borrow().get(1).cloned().unwrap_or(Value::Int(0))).unwrap_or(0)
                }
                Value::Bridged(b) => match &b.borrow().native {
                    Value::List(l) => {
                        h::as_int(&l.borrow().get(1).cloned().unwrap_or(Value::Int(0))).unwrap_or(0)
                    }
                    _ => 0,
                },
                _ => 0,
            };
            Ok(Value::Int(tick))
        }),
    );
    bc.methods.insert(
        "cancel".to_string(),
        h::method(|_v, recv, _args, _n| match recv {
            Value::List(l) => {
                if let Some(slot) = l.borrow_mut().get_mut(0) {
                    *slot = Value::Bool(false);
                }
                Ok(Value::Null)
            }
            Value::Bridged(b) => match &b.borrow().native {
                Value::List(l) => {
                    if let Some(slot) = l.borrow_mut().get_mut(0) {
                        *slot = Value::Bool(false);
                    }
                    Ok(Value::Null)
                }
                _ => Err(InterpError::runtime("Invalid Timer")),
            },
            _ => Err(InterpError::runtime("Invalid Timer")),
        }),
    );
    bc
}

/// A live timer instance: `[active, tick]`.
fn make_timer() -> Value {
    h::wrap(
        &h::class("Timer"),
        Value::List(Rc::new(RefCell::new(vec![
            Value::Bool(true),
            Value::Int(0),
        ]))),
    )
}

/// Whether a timer is active, handling both the raw native and the bridged
/// wrapper (instance getters receive the native value).
fn timer_active_of(recv: &Value) -> bool {
    match recv {
        Value::List(l) => matches!(l.borrow().first(), Some(Value::Bool(true))),
        other => async_state::timer_active(other),
    }
}

fn timeout_exception_definition() -> BridgedClassDef {
    let mut bc = BridgedClassDef::new("TimeoutException");
    bc.constructors.insert(
        "".to_string(),
        h::ctor(|_v, args, _named| {
            let message = args
                .first()
                .map(|v| h::as_str(v).unwrap_or_default().to_string())
                .unwrap_or_default();
            let duration = args.get(1).cloned().unwrap_or(Value::Null);
            Ok(h::wrap(
                &h::class("TimeoutException"),
                Value::List(Rc::new(RefCell::new(vec![Value::str(message), duration]))),
            ))
        }),
    );
    bc.methods.insert(
        "toString".into(),
        h::method(|_v, t, _a, _n| {
            let message = h::item_at(&t, 0).unwrap_or(Value::Null);
            Ok(Value::str(format!(
                "TimeoutException after {}: {}",
                h::display(&h::item_at(&t, 1).unwrap_or(Value::Null)),
                h::display(&message),
            )))
        }),
    );
    bc.getters.insert(
        "message".into(),
        h::getter(|_v, t| Ok(h::item_at(&t, 0).unwrap_or(Value::Null))),
    );
    bc.getters.insert(
        "duration".into(),
        h::getter(|_v, t| Ok(h::item_at(&t, 1).unwrap_or(Value::Null))),
    );
    bc.getters.insert(
        "hashCode".into(),
        h::getter(|_v, t| Ok(Value::Int(h::hash_code(&t)))),
    );
    bc.getters.insert(
        "runtimeType".into(),
        h::getter(|_v, t| Ok(h::runtime_type(&t))),
    );
    bc
}
