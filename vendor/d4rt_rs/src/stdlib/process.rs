//! dart:io — Process, ProcessResult, ProcessSignal, ProcessStartMode,
//! ProcessException and IOSink.
//!
//! Native representations:
//! - Process: `[Int(pid), exit_future, stdin(IOSink), stdout(Stream),
//!   stderr(Stream)]`.
//! - ProcessResult: `[Int(exitCode), Int(pid), stdout, stderr]`.
//! - ProcessSignal / ProcessStartMode: `Value::Str(name)`.
//! - ProcessException: `[Str(message), Str(executable), args]`.
//! - IOSink: `[Int(pid)]` — writes route through the process registry.
//!
//! Real OS processes run on a worker thread; byte chunks are shipped back
//! over an mpsc channel and delivered to the stream controllers by a
//! self-re-enqueueing poll task while the interpreter awaits.

use super::helpers as h;
use crate::async_state;
use crate::class::BridgedClassDef;
use crate::env::Environment;
use crate::error::InterpError;
use crate::interp::InterpreterVisitor;
use crate::value::Value;
use std::cell::RefCell;
use std::collections::HashMap;
use std::io::{Read, Write};
use std::process::{Command, Stdio};
use std::rc::Rc;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::mpsc::{Receiver, Sender};

enum Ctrl {
    Write(Vec<u8>),
    CloseStdin,
    Kill,
}

enum Data {
    Stdout(Vec<u8>),
    Stderr(Vec<u8>),
    Exit(i64),
}

/// Per-process channel pair kept for the lifetime of the process.
/// Only touched from the interpreter thread.
struct Proc {
    tx: Sender<Ctrl>,
    rx: Receiver<Data>,
    exited: bool,
    ctx: i64,
}

static PROCESSES: std::sync::LazyLock<Mutex<HashMap<i64, Proc>>> =
    std::sync::LazyLock::new(|| Mutex::new(HashMap::new()));

/// Kills and forgets every child process owned by `ctx_id`.
pub(crate) fn cleanup_context(ctx_id: i64) {
    let mut procs = PROCESSES.lock().unwrap();
    let owned: Vec<i64> = procs
        .iter()
        .filter(|(_, p)| p.ctx == ctx_id)
        .map(|(pid, _)| *pid)
        .collect();
    for pid in owned {
        if let Some(p) = procs.remove(&pid) {
            let _ = p.tx.send(Ctrl::Kill);
        }
    }
}

fn define_bridge(env: &Rc<RefCell<Environment>>, bc: BridgedClassDef) {
    let bc = Arc::new(bc);
    h::register_class(bc.clone());
    Environment::define_bridge(env, bc);
}

fn wrap(bc_name: &str, native: Value) -> Value {
    h::wrap(&h::class(bc_name), native)
}

fn wrap_future(state: Rc<RefCell<async_state::FutureState>>) -> Value {
    wrap("Future", Value::Future(state))
}

fn future_ok(v: Value) -> Value {
    let state = Rc::new(RefCell::new(async_state::FutureState::new()));
    async_state::complete(&state, Ok(v));
    wrap_future(state)
}

fn future_err(e: Value) -> Value {
    let state = Rc::new(RefCell::new(async_state::FutureState::new()));
    async_state::complete(&state, Err(e));
    wrap_future(state)
}

fn bytes_value(bytes: Vec<u8>) -> Value {
    Value::List(Rc::new(RefCell::new(
        bytes.into_iter().map(|b| Value::Int(b as i64)).collect(),
    )))
}

fn str_value_list(v: &Value) -> Vec<String> {
    h::as_list(v)
        .map(|l| {
            l.borrow()
                .iter()
                .filter_map(|x| h::as_str(x).map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default()
}

fn as_str_opt(v: &Value) -> Option<String> {
    h::as_str(v).map(|s| s.to_string())
}

fn decoded(v: &[u8]) -> Value {
    Value::str(String::from_utf8_lossy(v).into_owned())
}

fn process_exception(exe: &str, args: &[String], message: impl Into<String>) -> Value {
    wrap(
        "ProcessException",
        Value::List(Rc::new(RefCell::new(vec![
            Value::str(message.into()),
            Value::str(exe),
            Value::List(Rc::new(RefCell::new(
                args.iter().map(|a| Value::str(a.clone())).collect(),
            ))),
        ]))),
    )
}

/// Builds the `std::process::Command` for a Process.start/run invocation.
fn build_command(exe: &str, arguments: &[String], named: &HashMap<String, Value>) -> Command {
    let run_in_shell = named
        .get("runInShell")
        .map(|v| h::as_bool(v).unwrap_or(false))
        .unwrap_or(false);
    let mut cmd = if run_in_shell {
        let joined = std::iter::once(exe.to_string())
            .chain(arguments.iter().cloned())
            .map(|s| quote_shell(&s))
            .collect::<Vec<_>>()
            .join(" ");
        let mut c = Command::new("sh");
        c.arg("-c").arg(joined);
        c
    } else {
        let mut c = Command::new(exe);
        c.args(arguments);
        c
    };
    cmd.stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(wd) = named.get("workingDirectory").and_then(as_str_opt) {
        cmd.current_dir(wd);
    }
    if let Some(env) = named
        .get("environment")
        .and_then(h::as_map)
        .map(|m| m.borrow().entries.clone())
    {
        for (k, v) in env {
            if let (Some(k), Some(v)) = (h::as_str(&k), h::as_str(&v)) {
                cmd.env(k, v);
            }
        }
    }
    if !named
        .get("includeParentEnvironment")
        .map(|v| h::as_bool(v).unwrap_or(true))
        .unwrap_or(true)
    {
        cmd.env_clear();
    }
    cmd
}

fn quote_shell(s: &str) -> String {
    if s.chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '.' || c == '/')
    {
        s.to_string()
    } else {
        format!("'{}'", s.replace('\'', "'\\''"))
    }
}

/// Spawns the worker threads for a started process and registers it.
fn spawn_worker(
    mut child: std::process::Child,
    exit_future: Rc<RefCell<async_state::FutureState>>,
) -> i64 {
    let pid = child.id() as i64;
    let (ctrl_tx, ctrl_rx) = std::sync::mpsc::channel::<Ctrl>();
    let (data_tx, data_rx) = std::sync::mpsc::channel::<Data>();
    let out_tx = data_tx.clone();
    let err_tx = data_tx.clone();

    if let Some(mut out) = child.stdout.take() {
        std::thread::spawn(move || {
            let mut buf = [0u8; 8192];
            loop {
                match out.read(&mut buf) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        if out_tx.send(Data::Stdout(buf[..n].to_vec())).is_err() {
                            break;
                        }
                    }
                }
            }
        });
    }
    if let Some(mut err) = child.stderr.take() {
        std::thread::spawn(move || {
            let mut buf = [0u8; 8192];
            loop {
                match err.read(&mut buf) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        if err_tx.send(Data::Stderr(buf[..n].to_vec())).is_err() {
                            break;
                        }
                    }
                }
            }
        });
    }

    std::thread::spawn(move || {
        let mut stdin = child.stdin.take();
        loop {
            match ctrl_rx.try_recv() {
                Ok(Ctrl::Write(bytes)) => {
                    if let Some(s) = &mut stdin {
                        let _ = s.write_all(&bytes);
                    }
                }
                Ok(Ctrl::CloseStdin) => {
                    stdin = None;
                }
                Ok(Ctrl::Kill) => {
                    let _ = child.kill();
                }
                Err(_) => {}
            }
            match child.try_wait() {
                Ok(Some(_)) => break,
                _ => std::thread::sleep(std::time::Duration::from_millis(1)),
            }
        }
        drop(stdin);
        use std::os::unix::process::ExitStatusExt;
        let code = match child.wait() {
            Ok(st) => st
                .code()
                .map(|c| c as i64)
                .unwrap_or_else(|| -(st.signal().unwrap_or(0) as i64)),
            Err(_) => -1,
        };
        let _ = data_tx.send(Data::Exit(code));
        let _ = exit_future;
    });

    PROCESSES.lock().unwrap().insert(
        pid,
        Proc {
            tx: ctrl_tx,
            rx: data_rx,
            exited: false,
            ctx: crate::state::current().id,
        },
    );
    pid
}

/// Polls one started process: drains byte chunks into its controllers and
/// completes the exit future when the worker reports exit. Returns whether
/// the process is still running (i.e. the poll task should continue).
pub fn poll_process(
    visitor: &mut InterpreterVisitor,
    pid: i64,
    exit_future: &Rc<RefCell<async_state::FutureState>>,
    stdout_ctrl: &Value,
    stderr_ctrl: &Value,
) -> bool {
    let (exit_code, done) = {
        let mut procs = PROCESSES.lock().unwrap();
        let Some(proc) = procs.get_mut(&pid) else {
            return false;
        };
        let mut exit_code = None;
        loop {
            match proc.rx.try_recv() {
                Ok(Data::Stdout(bytes)) => {
                    crate::stdlib::stream_lib::controller_add_value(
                        visitor,
                        stdout_ctrl,
                        bytes_value(bytes),
                    );
                }
                Ok(Data::Stderr(bytes)) => {
                    crate::stdlib::stream_lib::controller_add_value(
                        visitor,
                        stderr_ctrl,
                        bytes_value(bytes),
                    );
                }
                Ok(Data::Exit(code)) => {
                    exit_code = Some(code);
                }
                Err(_) => break,
            }
        }
        let done = exit_code.is_some();
        if done {
            proc.exited = true;
        }
        (exit_code, done)
    };
    if done {
        let _ = crate::stdlib::stream_lib::controller_close_value(visitor, stdout_ctrl);
        let _ = crate::stdlib::stream_lib::controller_close_value(visitor, stderr_ctrl);
        if let Some(code) = exit_code {
            async_state::complete(exit_future, Ok(Value::Int(code)));
        }
        PROCESSES.lock().unwrap().remove(&pid);
        false
    } else {
        std::thread::sleep(std::time::Duration::from_millis(1));
        true
    }
}

/// Kills the process with the given signal, returning whether the signal
/// was delivered.
pub fn kill_pid(pid: i64, signal: &str) -> bool {
    let sig = signal.trim_start_matches("SIG").to_ascii_lowercase();
    let ok = Command::new("kill")
        .arg("-s")
        .arg(&sig)
        .arg(pid.to_string())
        .status()
        .is_ok_and(|st| st.success());
    if ok && let Some(proc) = PROCESSES.lock().unwrap().get_mut(&pid) {
        let _ = proc.tx.send(Ctrl::Kill);
    }
    ok
}

/// Writes bytes to a process's stdin (best effort; no-op if gone).
pub fn stdin_write(pid: i64, bytes: &[u8]) {
    if let Some(proc) = PROCESSES.lock().unwrap().get(&pid) {
        let _ = proc.tx.send(Ctrl::Write(bytes.to_vec()));
    }
}

/// Closes a process's stdin.
pub fn stdin_close(pid: i64) {
    if let Some(proc) = PROCESSES.lock().unwrap().get(&pid) {
        let _ = proc.tx.send(Ctrl::CloseStdin);
    }
}

// ---------- Process ----------

fn process_definition() -> BridgedClassDef {
    let mut bc = BridgedClassDef::new("Process");
    bc.native_type = Some("d4rt::io::Process".to_string());

    bc.static_methods.insert(
        "start".to_string(),
        h::static_method(|_v, args, named| {
            let exe = args
                .first()
                .map(|v| h::as_str(v).unwrap_or("").to_string())
                .unwrap_or_default();
            let arguments = args
                .get(1)
                .cloned()
                .map(|l| str_value_list(&l))
                .unwrap_or_default();
            if exe.is_empty() {
                return Ok(future_err(Value::str(
                    "Process.start requires executable path",
                )));
            }
            if let Err(e) = crate::permission::require_process(&exe) {
                return Ok(future_err(Value::str(e.to_string())));
            }
            let mut cmd = build_command(&exe, &arguments, &named);
            let mode = named
                .get("mode")
                .and_then(as_str_opt)
                .unwrap_or_else(|| "normal".to_string());
            if mode == "inheritStdio" || mode == "detachedWithStdio" {
                cmd.stdin(Stdio::inherit())
                    .stdout(Stdio::inherit())
                    .stderr(Stdio::inherit());
            }
            match cmd.spawn() {
                Ok(child) => {
                    let exit_state = Rc::new(RefCell::new(async_state::FutureState::new()));
                    let pid = spawn_worker(child, exit_state.clone());
                    let exit_future = wrap_future(exit_state.clone());
                    let stdout_ctrl =
                        crate::stdlib::stream_lib::make_controller_value(false, false);
                    let stderr_ctrl =
                        crate::stdlib::stream_lib::make_controller_value(false, false);
                    let stdout_stream =
                        crate::stdlib::stream_lib::controller_stream_value(&stdout_ctrl);
                    let stderr_stream =
                        crate::stdlib::stream_lib::controller_stream_value(&stderr_ctrl);
                    async_state::enqueue_task(async_state::Task::ProcessPoll {
                        pid,
                        exit_future: exit_state.clone(),
                        stdout_ctrl: stdout_ctrl.clone(),
                        stderr_ctrl: stderr_ctrl.clone(),
                    });
                    let stdin_sink = wrap("IOSink", Value::Int(pid));
                    let p = wrap(
                        "Process",
                        Value::List(Rc::new(RefCell::new(vec![
                            Value::Int(pid),
                            exit_future,
                            stdin_sink,
                            stdout_stream,
                            stderr_stream,
                        ]))),
                    );
                    Ok(future_ok(p))
                }
                Err(e) => Ok(future_err(process_exception(
                    &exe,
                    &arguments,
                    e.to_string(),
                ))),
            }
        }),
    );

    let run_common = |args: &[Value],
                      named: &HashMap<String, Value>,
                      sync: bool|
     -> Result<Value, InterpError> {
        let exe = args
            .first()
            .map(|v| h::as_str(v).unwrap_or("").to_string())
            .unwrap_or_default();
        let arguments = args
            .get(1)
            .cloned()
            .map(|l| str_value_list(&l))
            .unwrap_or_default();
        if exe.is_empty() {
            return Err(h::err("Process.run requires executable path"));
        }
        crate::permission::require_process(&exe)?;
        let mut cmd = build_command(&exe, &arguments, named);
        let (tx, rx) = std::sync::mpsc::channel::<(u32, std::io::Result<std::process::Output>)>();
        let exe_t = exe.clone();
        let args_t = arguments.clone();
        std::thread::spawn(move || match cmd.spawn() {
            Ok(child) => {
                let pid = child.id();
                let out = child.wait_with_output();
                let _ = tx.send((pid, out));
            }
            Err(e) => {
                let _ = tx.send((0, Err(e)));
            }
        });
        let (pid, out) = rx
            .recv()
            .unwrap_or((0, Err(std::io::Error::other("process channel closed"))));
        match out {
            Ok(o) => {
                let result = wrap(
                    "ProcessResult",
                    Value::List(Rc::new(RefCell::new(vec![
                        Value::Int(o.status.code().unwrap_or(0) as i64),
                        Value::Int(pid as i64),
                        decoded(&o.stdout),
                        decoded(&o.stderr),
                    ]))),
                );
                let _ = exe_t;
                let _ = args_t;
                if sync {
                    Ok(result)
                } else {
                    Ok(future_ok(result))
                }
            }
            Err(e) => {
                if sync {
                    Err(InterpError::Throw(process_exception(
                        &exe_t,
                        &args_t,
                        e.to_string(),
                    )))
                } else {
                    Ok(future_err(process_exception(
                        &exe_t,
                        &args_t,
                        e.to_string(),
                    )))
                }
            }
        }
    };

    bc.static_methods.insert(
        "runSync".to_string(),
        h::static_method(move |_v, args, named| run_common(&args, &named, true)),
    );
    bc.static_methods.insert(
        "run".to_string(),
        h::static_method(move |_v, args, named| run_common(&args, &named, false)),
    );
    bc.static_methods.insert(
        "killPid".to_string(),
        h::static_method(|_v, args, named| {
            let pid = args.first().and_then(h::as_int).unwrap_or(0);
            let signal = named
                .get("signal")
                .and_then(as_str_opt)
                .unwrap_or_else(|| "SIGTERM".to_string());
            crate::permission::require_process("kill")?;
            Ok(Value::Bool(kill_pid(pid, &signal)))
        }),
    );

    bc.methods.insert(
        "kill".to_string(),
        h::method(|_v, recv, _args, named| {
            let pid = h::as_list(&recv)
                .and_then(|l| l.borrow().first().cloned())
                .and_then(|p| h::as_int(&p))
                .unwrap_or(0);
            let signal = named
                .get("signal")
                .and_then(as_str_opt)
                .unwrap_or_else(|| "SIGTERM".to_string());
            crate::permission::require_process("kill")?;
            Ok(Value::Bool(kill_pid(pid, &signal)))
        }),
    );

    bc.getters.insert(
        "pid".to_string(),
        h::getter(|_v, recv| {
            Ok(Value::Int(
                h::as_list(&recv)
                    .and_then(|l| l.borrow().first().cloned())
                    .and_then(|p| h::as_int(&p))
                    .unwrap_or(0),
            ))
        }),
    );
    bc.getters.insert(
        "exitCode".to_string(),
        h::getter(|_v, recv| {
            let l = h::as_list(&recv).unwrap();
            Ok(l.borrow().get(1).cloned().unwrap_or(Value::Null))
        }),
    );
    bc.getters.insert(
        "stdin".to_string(),
        h::getter(|_v, recv| {
            let l = h::as_list(&recv).unwrap();
            Ok(l.borrow().get(2).cloned().unwrap_or(Value::Null))
        }),
    );
    bc.getters.insert(
        "stdout".to_string(),
        h::getter(|_v, recv| {
            let l = h::as_list(&recv).unwrap();
            Ok(l.borrow().get(3).cloned().unwrap_or(Value::Null))
        }),
    );
    bc.getters.insert(
        "stderr".to_string(),
        h::getter(|_v, recv| {
            let l = h::as_list(&recv).unwrap();
            Ok(l.borrow().get(4).cloned().unwrap_or(Value::Null))
        }),
    );
    bc
}

// ---------- ProcessResult ----------

fn process_result_definition() -> BridgedClassDef {
    let mut bc = BridgedClassDef::new("ProcessResult");
    bc.native_type = Some("d4rt::io::ProcessResult".to_string());
    for (name, index) in [
        ("exitCode", 0usize),
        ("pid", 1),
        ("stdout", 2),
        ("stderr", 3),
    ] {
        bc.getters.insert(
            name.to_string(),
            h::getter(move |_v, recv| {
                let l = h::as_list(&recv).unwrap();
                Ok(l.borrow().get(index).cloned().unwrap_or(Value::Null))
            }),
        );
    }
    bc
}

// ---------- ProcessSignal ----------

fn process_signal_definition() -> BridgedClassDef {
    let mut bc = BridgedClassDef::new("ProcessSignal");
    bc.native_type = Some("d4rt::io::ProcessSignal".to_string());
    for name in [
        "sighup",
        "sigint",
        "sigquit",
        "sigill",
        "sigtrap",
        "sigabrt",
        "sigbus",
        "sigfpe",
        "sigkill",
        "sigusr1",
        "sigsegv",
        "sigusr2",
        "sigpipe",
        "sigalrm",
        "sigterm",
        "sigchld",
        "sigcont",
        "sigstop",
        "sigtstp",
        "sigttin",
        "sigttou",
        "sigurg",
        "sigxcpu",
        "sigxfsz",
        "sigvtalrm",
        "sigprof",
        "sigwinch",
        "sigpoll",
        "sigsys",
    ] {
        let upper = format!("SIG{}", name[3..].to_ascii_uppercase());
        bc.static_getters.insert(
            name.to_string(),
            h::static_getter(move |_v| Ok(wrap("ProcessSignal", Value::str(upper.clone())))),
        );
    }
    bc.getters.insert(
        "name".to_string(),
        h::getter(|_v, recv| Ok(Value::str(h::as_str(&recv).unwrap_or("")))),
    );
    bc.getters.insert(
        "hashCode".to_string(),
        h::getter(|_v, t| Ok(Value::Int(h::hash_code(&t)))),
    );
    bc.methods.insert(
        "toString".into(),
        h::method(|_v, t, _a, _n| {
            Ok(Value::str(format!(
                "ProcessSignal: {}",
                h::as_str(&t).unwrap_or("")
            )))
        }),
    );
    bc.methods.insert(
        "watch".into(),
        h::method(|_v, _t, _a, _n| {
            let state = Rc::new(RefCell::new(async_state::FutureState::new()));
            Ok(wrap_future(state))
        }),
    );
    bc
}

// ---------- ProcessStartMode ----------

fn process_start_mode_definition() -> BridgedClassDef {
    let mut bc = BridgedClassDef::new("ProcessStartMode");
    bc.native_type = Some("d4rt::io::ProcessStartMode".to_string());
    for (name, value) in [
        ("normal", "normal"),
        ("inheritStdio", "inheritStdio"),
        ("detached", "detached"),
        ("detachedWithStdio", "detachedWithStdio"),
    ] {
        bc.static_getters.insert(
            name.to_string(),
            h::static_getter(move |_v| Ok(wrap("ProcessStartMode", Value::str(value)))),
        );
    }
    bc
}

// ---------- ProcessException ----------

fn process_exception_definition() -> BridgedClassDef {
    let mut bc = BridgedClassDef::new("ProcessException");
    bc.native_type = Some("d4rt::io::ProcessException".to_string());
    bc.constructors.insert(
        "".to_string(),
        h::ctor(|_v, args, _n| {
            let message = args
                .first()
                .map(|v| h::as_str(v).unwrap_or("").to_string())
                .unwrap_or_default();
            let executable = args
                .get(1)
                .map(|v| h::as_str(v).unwrap_or("").to_string())
                .unwrap_or_default();
            let arguments = args
                .get(2)
                .cloned()
                .map(|l| str_value_list(&l))
                .unwrap_or_default();
            Ok(wrap(
                "ProcessException",
                Value::List(Rc::new(RefCell::new(vec![
                    Value::str(message),
                    Value::str(executable),
                    Value::List(Rc::new(RefCell::new(
                        arguments.iter().map(|a| Value::str(a.clone())).collect(),
                    ))),
                ]))),
            ))
        }),
    );
    bc.methods.insert(
        "toString".into(),
        h::method(|_v, t, _a, _n| {
            let l = h::as_list(&t).unwrap();
            let msg = l
                .borrow()
                .first()
                .cloned()
                .and_then(|m| h::as_str(&m).map(|s| s.to_string()))
                .unwrap_or_default();
            let exe = l
                .borrow()
                .get(1)
                .cloned()
                .and_then(|m| h::as_str(&m).map(|s| s.to_string()))
                .unwrap_or_default();
            Ok(Value::str(format!(
                "ProcessException: {msg}, executable = {exe}"
            )))
        }),
    );
    bc
}

// ---------- IOSink ----------

fn sink_pid_of(v: &Value) -> i64 {
    match v {
        Value::Int(i) => *i,
        _ => h::as_list(v)
            .and_then(|l| l.borrow().first().cloned())
            .and_then(|p| h::as_int(&p))
            .unwrap_or(0),
    }
}

fn sink_write_to(recv: &Value, bytes: &[u8]) {
    if crate::stdlib::io::is_pipe_sink(recv) {
        crate::stdlib::io::pipe_sink_write(recv, bytes);
    } else if crate::stdlib::io::is_file_sink(recv) {
        crate::stdlib::io::file_sink_write(recv, bytes);
    } else {
        stdin_write(sink_pid_of(recv), bytes);
    }
}

fn io_sink_definition() -> BridgedClassDef {
    let mut bc = BridgedClassDef::new("IOSink");
    bc.native_type = Some("d4rt::io::IOSink".to_string());
    bc.methods.insert(
        "write".to_string(),
        h::method(move |_v, recv, args, _n| {
            let s = args.first().map(v_to_string).unwrap_or_default();
            sink_write_to(&recv, s.as_bytes());
            Ok(Value::Null)
        }),
    );
    bc.methods.insert(
        "writeln".to_string(),
        h::method(move |_v, recv, args, _n| {
            let s = args.first().map(v_to_string).unwrap_or_default();
            sink_write_to(&recv, format!("{s}\n").as_bytes());
            Ok(Value::Null)
        }),
    );
    bc.methods.insert(
        "writeCharCode".to_string(),
        h::method(move |_v, recv, args, _n| {
            let c = args.first().and_then(h::as_int).unwrap_or(0) as u32;
            if let Some(ch) = char::from_u32(c) {
                sink_write_to(&recv, ch.to_string().as_bytes());
            }
            Ok(Value::Null)
        }),
    );
    bc.methods.insert(
        "writeAll".to_string(),
        h::method(move |_v, recv, args, _n| {
            let sep = args.get(1).and_then(|v| h::as_str(v)).unwrap_or("");
            let items = args
                .first()
                .cloned()
                .map(|l| {
                    h::as_list(&l)
                        .map(|l| l.borrow().iter().map(v_to_string).collect::<Vec<_>>())
                        .unwrap_or_default()
                })
                .unwrap_or_default();
            sink_write_to(&recv, items.join(sep).as_bytes());
            Ok(Value::Null)
        }),
    );
    bc.methods.insert(
        "add".to_string(),
        h::method(move |_v, recv, args, _n| {
            let bytes = args
                .first()
                .cloned()
                .map(|l| {
                    h::as_list(&l)
                        .map(|l| {
                            l.borrow()
                                .iter()
                                .filter_map(|x| h::as_int(x).map(|i| i as u8))
                                .collect::<Vec<u8>>()
                        })
                        .unwrap_or_default()
                })
                .unwrap_or_default();
            sink_write_to(&recv, &bytes);
            Ok(Value::Null)
        }),
    );
    bc.methods.insert(
        "addStream".to_string(),
        h::method(|visitor, recv, args, _n| {
            let stream = args.first().cloned().unwrap_or(Value::Null);
            let recv_t = recv.clone();
            let out = Rc::new(RefCell::new(async_state::FutureState::new()));
            let out_done = out.clone();
            let on_data = Value::NativeFn(Rc::new(crate::callable::NativeFunction {
                name: Some("<IOSink.addStream data>".to_string()),
                min_arity: 1,
                max_arity: Some(1),
                is_async: false,
                is_generator: false,
                closure: Box::new(move |_visitor, args, _n| {
                    if let Some(l) = args.first() {
                        let bytes: Vec<u8> = h::as_list(l)
                            .map(|l| {
                                l.borrow()
                                    .iter()
                                    .filter_map(|x| h::as_int(x).map(|i| i as u8))
                                    .collect()
                            })
                            .unwrap_or_default();
                        sink_write_to(&recv_t, &bytes);
                    }
                    Ok(Value::Null)
                }),
                callable_runtime_type: crate::rt::FunctionRuntimeType::untyped(),
            }));
            let on_error = {
                let out_done = out_done.clone();
                Value::NativeFn(Rc::new(crate::callable::NativeFunction {
                    name: Some("<IOSink.addStream error>".to_string()),
                    min_arity: 1,
                    max_arity: Some(1),
                    is_async: false,
                    is_generator: false,
                    closure: Box::new(move |_visitor, args, _n| {
                        async_state::complete(
                            &out_done.clone(),
                            Err(args.first().cloned().unwrap_or(Value::Null)),
                        );
                        Ok(Value::Null)
                    }),
                    callable_runtime_type: crate::rt::FunctionRuntimeType::untyped(),
                }))
            };
            let on_done = {
                let out_done = out_done.clone();
                Value::NativeFn(Rc::new(crate::callable::NativeFunction {
                    name: Some("<IOSink.addStream done>".to_string()),
                    min_arity: 0,
                    max_arity: Some(0),
                    is_async: false,
                    is_generator: false,
                    closure: Box::new(move |_visitor, _args, _n| {
                        async_state::complete(&out_done.clone(), Ok(Value::Null));
                        Ok(Value::Null)
                    }),
                    callable_runtime_type: crate::rt::FunctionRuntimeType::untyped(),
                }))
            };
            crate::stdlib::stream_lib::stream_subscribe_value(
                visitor, &stream, on_data, on_error, on_done,
            );
            Ok(wrap_future(out))
        }),
    );
    bc.methods.insert(
        "addError".to_string(),
        h::method(|_v, _recv, _args, _n| Ok(Value::Null)),
    );
    bc.methods.insert(
        "flush".to_string(),
        h::method(move |_v, recv, _args, _n| {
            if crate::stdlib::io::is_pipe_sink(&recv) {
                Ok(future_ok(Value::Null))
            } else if crate::stdlib::io::is_file_sink(&recv) {
                match crate::stdlib::io::file_sink_flush(&recv) {
                    Ok(_) => Ok(future_ok(Value::Null)),
                    Err(e) => Ok(future_err(Value::str(e.to_string()))),
                }
            } else {
                Ok(future_ok(Value::Null))
            }
        }),
    );
    bc.methods.insert(
        "close".to_string(),
        h::method(move |_v, recv, _args, _n| {
            if crate::stdlib::io::is_pipe_sink(&recv) {
                crate::stdlib::io::pipe_sink_close(&recv);
                Ok(future_ok(Value::Null))
            } else if crate::stdlib::io::is_file_sink(&recv) {
                match crate::stdlib::io::file_sink_flush(&recv) {
                    Ok(_) => {
                        if let Some(st) = crate::stdlib::io::file_sink_done_state(&recv) {
                            async_state::complete(&st, Ok(Value::Null));
                        }
                        Ok(future_ok(Value::Null))
                    }
                    Err(e) => {
                        if let Some(st) = crate::stdlib::io::file_sink_done_state(&recv) {
                            async_state::complete(&st, Err(Value::str(e.to_string())));
                        }
                        Ok(future_err(Value::str(e.to_string())))
                    }
                }
            } else {
                stdin_close(sink_pid_of(&recv));
                Ok(future_ok(Value::Null))
            }
        }),
    );
    bc.getters.insert(
        "done".to_string(),
        h::getter(|_v, recv| {
            if let Some(st) = crate::stdlib::io::pipe_sink_done_state(&recv) {
                Ok(wrap("Future", Value::Future(st)))
            } else if let Some(st) = crate::stdlib::io::file_sink_done_state(&recv) {
                Ok(wrap("Future", Value::Future(st)))
            } else {
                Ok(future_ok(Value::Null))
            }
        }),
    );
    bc.getters.insert(
        "encoding".to_string(),
        h::getter(|_v, _r| Ok(wrap("Utf8Codec", Value::Null))),
    );
    bc.setters
        .insert("encoding".to_string(), h::setter(|_v, _r, _val| Ok(())));
    bc.methods.insert(
        "addError".to_string(),
        h::method(|_v, _r, _a, _n| Ok(Value::Null)),
    );
    bc.methods.insert(
        "toString".into(),
        h::method(|_v, _t, _a, _n| Ok(Value::str("IOSink"))),
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

fn v_to_string(v: &Value) -> String {
    match v {
        Value::Str(s) => s.to_string(),
        Value::Int(i) => i.to_string(),
        Value::Double(d) => d.to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Null => "null".to_string(),
        other => format!("{other:?}"),
    }
}

pub fn load(env: &Rc<RefCell<Environment>>) {
    define_bridge(env, process_definition());
    define_bridge(env, process_result_definition());
    define_bridge(env, process_signal_definition());
    define_bridge(env, process_start_mode_definition());
    define_bridge(env, process_exception_definition());
    define_bridge(env, io_sink_definition());
}
