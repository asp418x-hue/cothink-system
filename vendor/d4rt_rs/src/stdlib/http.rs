//! dart:io — Socket, ServerSocket, InternetAddress, InternetAddressType,
//! SocketException, HttpClient, HttpClientRequest, HttpClientResponse,
//! HttpServer, HttpRequest, HttpResponse, HttpHeaders, ContentType, Cookie.
//!
//! Real TCP/HTTP on worker threads, mirroring the Process design:
//! worker threads push events into mpsc channels drained by poll tasks
//! enqueued on the interpreter's event loop.
//!
//! Native representations:
//! - InternetAddress: `Value::Str(address)`.
//! - InternetAddressType: `Value::Str(name)`.
//! - SocketException: `[Str(message), Str(host), Int(port)]`.
//! - Socket: `[Int(id), ctrl, stream, done, Str(host), Int(port),
//!   Str(remote_host), Int(remote_port)]`.
//! - ServerSocket: `[Int(id), ctrl, stream, done, Int(port), Str(host)]`.
//! - HttpClient: `Value::Null`.
//! - HttpClientRequest: `[Int(id), Str(method), Str(host), Int(port),
//!   Str(path), headers, done]`.
//! - HttpClientResponse: `[stream, Int(status), Str(reason), headers]`.
//! - HttpServer: `[Int(id), ctrl, stream, done, Int(port), Str(host)]`.
//! - HttpRequest: `[stream, Str(method), uri, headers, response]`.
//! - HttpResponse: `[Int(conn_id), Int(status), headers, done]`.
//! - HttpHeaders: `Value::Map` name -> list of values.
//! - ContentType: `[Str(primary), Str(sub), Str-or-Null(charset)]`.
//! - Cookie: `[Str(name), Str(value), Int(maxAge), Str(path), Bool(secure),
//!   Bool(httpOnly)]`.

use super::helpers as h;
use super::stream_lib as sl;
use crate::async_state;
use crate::callable::NativeFunction;
use crate::class::BridgedClassDef;
use crate::env::Environment;
use crate::error::InterpError;
use crate::interp::InterpreterVisitor;
use crate::value::{DartMap, Value};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet, VecDeque};
use std::io::{Read, Write};
use std::net::{Shutdown, TcpListener, TcpStream, UdpSocket};
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::sync::{LazyLock, Mutex};

/// Received UDP datagrams: id -> [(bytes, host, port)].
type UdpQueue = HashMap<i64, VecDeque<(Vec<u8>, String, i64)>>;
/// UDP per-socket options: (multicastLoopback, multicastHops, broadcast).
type UdpOpts = HashMap<i64, (bool, i64, bool)>;
/// Parsed HTTP head: method, path, headers.
type HttpHead = (String, String, Vec<(String, String)>);

enum SocketCtrl {
    Write(Vec<u8>),
    Close,
}

pub enum SocketData {
    Read(Vec<u8>),
    Eof,
    Error(String),
}

enum ConnCtrl {
    Write(Vec<u8>),
}

/// Commands for a UDP socket's writer thread.
pub enum UdpCtrl {
    Send(Vec<u8>, String, i64),
    Close,
}

/// Events produced by a UDP socket's reader thread.
pub enum UdpData {
    Packet {
        bytes: Vec<u8>,
        from: String,
        port: i64,
    },
    Closed,
}

pub enum ServerData {
    /// An HTTP request parsed by the server worker (HttpServer mode).
    Accepted {
        conn_id: i64,
        method: String,
        path: String,
        headers: Vec<(String, String)>,
        body: Vec<u8>,
        peer: String,
    },
    /// A raw accepted connection (ServerSocket mode).
    SocketAccepted {
        id: i64,
        peer: String,
        data_rx: Receiver<SocketData>,
    },
}

pub enum HttpData {
    Response {
        status: i64,
        reason: String,
        headers: Vec<(String, String)>,
        body: Vec<u8>,
    },
    Error(String),
}

static NEXT_ID: AtomicI64 = AtomicI64::new(1);
/// Client sockets and their write handles.
static SOCKETS: LazyLock<Mutex<HashMap<i64, Sender<SocketCtrl>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));
/// RawSocket receive buffers (RawSocket.read/available), keyed by socket id.
static RAW_BUFS: LazyLock<Mutex<HashMap<i64, Vec<u8>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));
/// Server ids created via RawServerSocket.bind (accepted sockets become
/// RawSocket values instead of Socket values).
static RAW_SERVERS: LazyLock<Mutex<HashSet<i64>>> = LazyLock::new(|| Mutex::new(HashSet::new()));
/// UDP sockets: id -> writer/close handle.
static UDP_SOCKS: LazyLock<Mutex<HashMap<i64, Sender<UdpCtrl>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));
/// Received datagrams, drained by Task::UdpPoll: id -> [(bytes, host, port)].
static UDP_QUEUE: LazyLock<Mutex<UdpQueue>> = LazyLock::new(|| Mutex::new(HashMap::new()));
/// UDP per-socket options: (multicastLoopback, multicastHops, broadcast).
static UDP_OPTS: LazyLock<Mutex<UdpOpts>> = LazyLock::new(|| Mutex::new(HashMap::new()));
/// HttpServer connection response handles.
static SERVER_CONNS: LazyLock<Mutex<HashMap<i64, Sender<ConnCtrl>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));
/// HttpServer accept-loop stop handles.
static SERVER_CLOSE: LazyLock<Mutex<HashMap<i64, Sender<()>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));
/// Pending HttpClientRequest bodies (keyed by request id).
static REQUEST_BUFS: LazyLock<Mutex<HashMap<i64, Vec<u8>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));
/// Buffered HttpServer response bodies (keyed by connection id).
static RESPONSE_BUFS: LazyLock<Mutex<HashMap<i64, Vec<u8>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// Resource kinds tracked in [`OWNERS`], so a dropped `Context` can close
/// every socket/server/udp handle it created.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum HttpResourceKind {
    Socket,
    RawServer,
    Udp,
    Server,
    ServerConn,
    RequestBuf,
    ResponseBuf,
}

/// resource id -> (context id, kind) ownership map.
static OWNERS: LazyLock<Mutex<HashMap<i64, (i64, HttpResourceKind)>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

fn claim(ctx_id: i64, id: i64, kind: HttpResourceKind) {
    OWNERS.lock().unwrap().insert(id, (ctx_id, kind));
}

/// Closes and forgets every resource owned by `ctx_id`.
pub(crate) fn cleanup_context(ctx_id: i64) {
    let owned: Vec<(i64, HttpResourceKind)> = OWNERS
        .lock()
        .unwrap()
        .iter()
        .filter(|(_, (c, _))| *c == ctx_id)
        .map(|(id, (_, k))| (*id, *k))
        .collect();
    for (id, kind) in owned {
        OWNERS.lock().unwrap().remove(&id);
        match kind {
            HttpResourceKind::Socket => {
                if let Some(tx) = SOCKETS.lock().unwrap().remove(&id) {
                    let _ = tx.send(SocketCtrl::Close);
                }
                RAW_BUFS.lock().unwrap().remove(&id);
            }
            HttpResourceKind::RawServer => {
                RAW_SERVERS.lock().unwrap().remove(&id);
            }
            HttpResourceKind::Udp => {
                if let Some(tx) = UDP_SOCKS.lock().unwrap().remove(&id) {
                    let _ = tx.send(UdpCtrl::Close);
                }
                UDP_QUEUE.lock().unwrap().remove(&id);
                UDP_OPTS.lock().unwrap().remove(&id);
            }
            HttpResourceKind::Server | HttpResourceKind::ServerConn => {
                if let Some(tx) = SERVER_CLOSE.lock().unwrap().remove(&id) {
                    let _ = tx.send(());
                }
                if let Some(tx) = SERVER_CONNS.lock().unwrap().remove(&id) {
                    drop(tx);
                }
            }
            HttpResourceKind::RequestBuf => {
                REQUEST_BUFS.lock().unwrap().remove(&id);
            }
            HttpResourceKind::ResponseBuf => {
                RESPONSE_BUFS.lock().unwrap().remove(&id);
            }
        }
    }
}

fn new_id() -> i64 {
    NEXT_ID.fetch_add(1, Ordering::SeqCst)
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

fn bytes_value(bytes: &[u8]) -> Value {
    Value::List(Rc::new(RefCell::new(
        bytes.iter().map(|b| Value::Int(*b as i64)).collect(),
    )))
}

fn ints_bytes(v: &Value) -> Vec<u8> {
    h::as_list(v)
        .map(|l| {
            l.borrow()
                .iter()
                .filter_map(|b| h::as_int(b).map(|i| i as u8))
                .collect()
        })
        .unwrap_or_default()
}

fn headers_value(headers: Vec<(String, String)>) -> Value {
    let entries = headers
        .into_iter()
        .map(|(k, v)| {
            (
                Value::str(k.to_ascii_lowercase()),
                Value::List(Rc::new(RefCell::new(vec![Value::str(v)]))),
            )
        })
        .collect();
    wrap(
        "HttpHeaders",
        Value::Map(Rc::new(RefCell::new(DartMap { entries }))),
    )
}

fn headers_of(v: &Value) -> Vec<(String, String)> {
    h::as_list(v)
        .and_then(|l| l.borrow().get(5).cloned())
        .and_then(|hv| h::as_map(&hv))
        .map(|m| {
            m.borrow()
                .entries
                .iter()
                .filter_map(|(k, val)| {
                    let name = h::as_str(k)?.to_string();
                    let value = h::as_list(val)
                        .and_then(|l| l.borrow().first().cloned())
                        .map(|v| h::display(&v))
                        .unwrap_or_default();
                    Some((name, value))
                })
                .collect()
        })
        .unwrap_or_default()
}

pub fn socket_exception(message: &str, host: &str, port: i64) -> Value {
    wrap(
        "SocketException",
        Value::List(Rc::new(RefCell::new(vec![
            Value::str(message),
            Value::str(host),
            Value::Int(port),
        ]))),
    )
}

fn list_get(v: &Value, index: usize) -> Option<Value> {
    h::as_list(v).and_then(|l| l.borrow().get(index).cloned())
}

fn socket_send(v: &Value, bytes: &[u8]) {
    if let Some(id) = list_get(v, 0).and_then(|i| h::as_int(&i)) {
        let reg = SOCKETS.lock().unwrap();
        if let Some(tx) = reg.get(&id) {
            let _ = tx.send(SocketCtrl::Write(bytes.to_vec()));
        }
    }
}

fn socket_ctrl(v: &Value, ctrl: SocketCtrl) {
    if let Some(id) = list_get(v, 0).and_then(|i| h::as_int(&i))
        && let Some(tx) = SOCKETS.lock().unwrap().get(&id)
    {
        let _ = tx.send(ctrl);
    }
}

/// Spawns reader/writer threads for a connected stream; the reader thread
/// forwards chunks into `data_tx`. Returns the socket id.
fn spawn_socket_pair(stream: TcpStream, data_tx: Sender<SocketData>, ctx_id: i64) -> i64 {
    let id = new_id();
    let (ctrl_tx, ctrl_rx) = mpsc::channel();
    SOCKETS.lock().unwrap().insert(id, ctrl_tx);
    claim(ctx_id, id, HttpResourceKind::Socket);
    let data_tx_w = data_tx.clone();
    let mut w = stream
        .try_clone()
        .unwrap_or_else(|_| stream.try_clone().unwrap());
    std::thread::spawn(move || {
        loop {
            match ctrl_rx.try_recv() {
                Ok(SocketCtrl::Write(b)) => {
                    let _ = w.write_all(&b);
                }
                Ok(SocketCtrl::Close) => {
                    let _ = w.shutdown(Shutdown::Both);
                    break;
                }
                Err(TryRecvError::Empty) => std::thread::sleep(std::time::Duration::from_millis(1)),
                Err(TryRecvError::Disconnected) => {
                    break;
                }
            }
        }
    });
    let mut r = stream;
    std::thread::spawn(move || {
        let mut buf = [0u8; 8192];
        loop {
            match r.read(&mut buf) {
                Ok(0) => {
                    let _ = data_tx_w.send(SocketData::Eof);
                    break;
                }
                Ok(n) => {
                    if data_tx_w.send(SocketData::Read(buf[..n].to_vec())).is_err() {
                        break;
                    }
                }
                Err(e) => {
                    let _ = data_tx_w.send(SocketData::Error(e.to_string()));
                    break;
                }
            }
        }
    });
    id
}

// ---------- sockets ----------

/// Shared Socket.connect / RawSocket.connect implementation.
fn socket_connect_parts(
    args: &[Value],
    named: &HashMap<String, Value>,
    class: &str,
) -> (Value, Rc<RefCell<async_state::FutureState>>) {
    let host = args
        .first()
        .and_then(h::as_str)
        .unwrap_or("localhost")
        .to_string();
    let port = args.get(1).and_then(h::as_int).unwrap_or(0);
    let _timeout = named.get("timeout");
    if let Err(e) = crate::permission::require_network(&host, port as u16) {
        let state = Rc::new(RefCell::new(async_state::FutureState::new()));
        async_state::complete(&state, Err(Value::str(e.to_string())));
        let fut = wrap_future(state.clone());
        return (fut, state);
    }
    let state = Rc::new(RefCell::new(async_state::FutureState::new()));
    let state_p = state.clone();
    let (id_tx, id_rx) = mpsc::channel();
    let (data_tx, data_rx) = mpsc::channel();
    let th_host = host.clone();
    let ctx_id = crate::state::current().id;
    std::thread::spawn(
        move || match TcpStream::connect((th_host.as_str(), port as u16)) {
            Ok(stream) => {
                let peer = stream
                    .peer_addr()
                    .map(|p| (p.ip().to_string(), p.port() as i64))
                    .unwrap_or((String::new(), 0));
                let id = spawn_socket_pair(stream, data_tx, ctx_id);
                let _ = id_tx.send(Ok((id, peer.0, peer.1)));
            }
            Err(e) => {
                let _ = id_tx.send(Err(e.to_string()));
            }
        },
    );
    async_state::enqueue_task(async_state::Task::SocketConnectPoll {
        state: state_p,
        host: host.clone(),
        port,
        id_rx,
        data_rx,
        class: class.to_string(),
    });
    let fut = wrap_future(state.clone());
    (fut, state)
}

fn socket_connect_value(args: &[Value], named: &HashMap<String, Value>, class: &str) -> Value {
    socket_connect_parts(args, named, class).0
}

/// A ConnectionTask value wrapping an in-flight socket future.
fn connection_task(socket_future: Value) -> Value {
    let cancel = Rc::new(RefCell::new(async_state::FutureState::new()));
    wrap(
        "ConnectionTask",
        Value::List(Rc::new(RefCell::new(vec![
            socket_future,
            wrap_future(cancel),
        ]))),
    )
}

fn socket_definition() -> BridgedClassDef {
    let mut bc = BridgedClassDef::new("Socket");
    bc.native_type = Some("d4rt::io::Socket".to_string());
    bc.constructors.insert(
        "".to_string(),
        h::ctor(|_v, _args, _n| {
            Err(crate::error::InterpError::Runtime(
                "Socket can't be constructed directly; use Socket.connect".into(),
            ))
        }),
    );
    bc.static_methods.insert(
        "connect".to_string(),
        h::static_method(|_v, args, named| Ok(socket_connect_value(&args, &named, "Socket"))),
    );
    bc.static_methods.insert(
        "startConnect".to_string(),
        h::static_method(|_v, args, named| {
            let (fut, _st) = socket_connect_parts(&args, &named, "Socket");
            Ok(connection_task(fut))
        }),
    );
    bc.methods.insert(
        "write".to_string(),
        h::method(|_v, recv, args, _n| {
            let s = args.first().map(h::display).unwrap_or_default();
            socket_send(&recv, s.as_bytes());
            Ok(Value::Null)
        }),
    );
    bc.methods.insert(
        "writeln".to_string(),
        h::method(|_v, recv, args, _n| {
            let s = args.first().map(h::display).unwrap_or_default();
            socket_send(&recv, format!("{s}\n").as_bytes());
            Ok(Value::Null)
        }),
    );
    bc.methods.insert(
        "writeAll".to_string(),
        h::method(|_v, recv, args, _n| {
            let sep = args
                .get(1)
                .map(|s| h::as_str(s).unwrap_or("").to_string())
                .unwrap_or_default();
            let items: Vec<String> = args
                .first()
                .and_then(h::as_list)
                .map(|l| l.borrow().iter().map(h::display).collect())
                .unwrap_or_default();
            socket_send(&recv, items.join(&sep).as_bytes());
            Ok(Value::Null)
        }),
    );
    bc.methods.insert(
        "writeCharCode".to_string(),
        h::method(|_v, recv, args, _n| {
            let c = args.first().and_then(h::as_int).unwrap_or(0) as u8;
            socket_send(&recv, &[c]);
            Ok(Value::Null)
        }),
    );
    bc.methods.insert(
        "add".to_string(),
        h::method(|_v, recv, args, _n| {
            if let Some(l) = args.first() {
                socket_send(&recv, &ints_bytes(l));
            }
            Ok(Value::Null)
        }),
    );
    bc.methods.insert(
        "flush".to_string(),
        h::method(|_v, _r, _a, _n| Ok(future_ok(Value::Null))),
    );
    bc.methods.insert(
        "close".to_string(),
        h::method(|_v, recv, _a, _n| {
            socket_ctrl(&recv, SocketCtrl::Close);
            if let Some(id) = list_get(&recv, 0).and_then(|i| h::as_int(&i)) {
                SOCKETS.lock().unwrap().remove(&id);
            }
            Ok(future_ok(Value::Null))
        }),
    );
    bc.methods.insert(
        "destroy".to_string(),
        h::method(|_v, recv, _a, _n| {
            socket_ctrl(&recv, SocketCtrl::Close);
            if let Some(id) = list_get(&recv, 0).and_then(|i| h::as_int(&i)) {
                SOCKETS.lock().unwrap().remove(&id);
            }
            Ok(Value::Null)
        }),
    );
    bc.methods.insert(
        "addError".to_string(),
        h::method(|_v, _r, _a, _n| Ok(Value::Null)),
    );
    bc.methods
        .insert("setOption".to_string(), socket_set_option_method(8));
    bc.methods
        .insert("getRawOption".to_string(), socket_get_raw_option_method(8));
    bc.methods
        .insert("setRawOption".to_string(), socket_set_raw_option_method(8));
    bc.methods.insert(
        "addStream".to_string(),
        h::method(|visitor, recv, args, _n| {
            let stream = args.first().cloned().unwrap_or(Value::Null);
            let recv_c = recv.clone();
            let on_data = Value::NativeFn(Rc::new(crate::callable::NativeFunction {
                name: Some("<Socket.addStream data>".to_string()),
                min_arity: 1,
                max_arity: Some(1),
                is_async: false,
                is_generator: false,
                closure: Box::new(move |_visitor, args, _n| {
                    socket_send(&recv_c, &ints_bytes(args.first().unwrap_or(&Value::Null)));
                    Ok(Value::Null)
                }),
                callable_runtime_type: crate::rt::FunctionRuntimeType::untyped(),
            }));
            let done = Rc::new(RefCell::new(async_state::FutureState::new()));
            let done_out = done.clone();
            let on_done = Value::NativeFn(Rc::new(crate::callable::NativeFunction {
                name: Some("<Socket.addStream done>".to_string()),
                min_arity: 0,
                max_arity: Some(0),
                is_async: false,
                is_generator: false,
                closure: Box::new(move |_visitor, _args, _n| {
                    async_state::complete(&done_out, Ok(Value::Null));
                    Ok(Value::Null)
                }),
                callable_runtime_type: crate::rt::FunctionRuntimeType::untyped(),
            }));
            sl::stream_subscribe_value(visitor, &stream, on_data, Value::Null, on_done);
            Ok(wrap_future(done))
        }),
    );
    bc.getters.insert(
        "port".to_string(),
        h::getter(|_v, recv| Ok(list_get(&recv, 5).unwrap_or(Value::Int(0)))),
    );
    bc.getters.insert(
        "remotePort".to_string(),
        h::getter(|_v, recv| Ok(list_get(&recv, 7).unwrap_or(Value::Int(0)))),
    );
    bc.getters.insert(
        "address".to_string(),
        h::getter(|_v, recv| {
            Ok(list_get(&recv, 4)
                .map(|a| wrap("InternetAddress", a))
                .unwrap_or(Value::Null))
        }),
    );
    bc.getters.insert(
        "remoteAddress".to_string(),
        h::getter(|_v, recv| {
            Ok(list_get(&recv, 6)
                .map(|a| wrap("InternetAddress", a))
                .unwrap_or(Value::Null))
        }),
    );
    bc.getters.insert(
        "done".to_string(),
        h::getter(|_v, recv| Ok(list_get(&recv, 3).unwrap_or_else(|| future_ok(Value::Null)))),
    );
    bc.getters.insert(
        "encoding".to_string(),
        h::getter(|_v, _r| Ok(wrap("Utf8Codec", Value::Null))),
    );
    delegate_stream(&mut bc, 2);
    bc
}

// ---------- raw sockets ----------

/// Shared template for Str-backed enum classes (SocketOption, SocketDirection,
fn enum_definition(name: String, values: Vec<String>) -> BridgedClassDef {
    let mut bc = BridgedClassDef::new(&name);
    for value in values {
        let name_c = name.clone();
        bc.static_getters.insert(
            value.clone(),
            h::static_getter(move |_v| Ok(wrap(&name_c, Value::str(value.clone())))),
        );
    }
    bc.getters.insert(
        "name".to_string(),
        h::getter(|_v, recv| Ok(Value::str(h::as_str(&recv).unwrap_or_default()))),
    );
    let name_c = name.clone();
    bc.methods.insert(
        "toString".into(),
        h::method(move |_v, t, _a, _n| {
            Ok(Value::str(format!(
                "{}({})",
                name_c,
                h::as_str(&t).unwrap_or_default()
            )))
        }),
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

fn socket_option_definition() -> BridgedClassDef {
    enum_definition("SocketOption".to_string(), vec!["tcpNoDelay".to_string()])
}

fn socket_direction_definition() -> BridgedClassDef {
    enum_definition(
        "SocketDirection".to_string(),
        vec!["receive".into(), "send".into(), "both".into()],
    )
}

fn raw_socket_event_definition() -> BridgedClassDef {
    enum_definition(
        "RawSocketEvent".to_string(),
        vec![
            "read".into(),
            "write".into(),
            "readClosed".into(),
            "closed".into(),
        ],
    )
}

fn raw_socket_option_native(level: i64, option: i64, value: &[u8]) -> Value {
    wrap(
        "RawSocketOption",
        Value::List(Rc::new(RefCell::new(vec![
            Value::Int(level),
            Value::Int(option),
            bytes_value(value),
        ]))),
    )
}

fn default_raw_socket_option() -> Value {
    raw_socket_option_native(0, 0, &[])
}

fn raw_socket_option_definition() -> BridgedClassDef {
    let mut bc = BridgedClassDef::new("RawSocketOption");
    bc.native_type = Some("d4rt::io::RawSocketOption".to_string());
    bc.constructors.insert(
        "".to_string(),
        h::ctor(|_v, args, _n| {
            let level = args.first().and_then(h::as_int).unwrap_or(0);
            let option = args.get(1).and_then(h::as_int).unwrap_or(0);
            let value = args.get(2).map(ints_bytes).unwrap_or_default();
            Ok(raw_socket_option_native(level, option, &value))
        }),
    );
    bc.constructors.insert(
        "fromInt".to_string(),
        h::ctor(|_v, args, _n| {
            let level = args.first().and_then(h::as_int).unwrap_or(0);
            let option = args.get(1).and_then(h::as_int).unwrap_or(0);
            let value = args.get(2).and_then(h::as_int).unwrap_or(0);
            let bytes = (value as u32).to_le_bytes();
            Ok(raw_socket_option_native(level, option, &bytes))
        }),
    );
    bc.constructors.insert(
        "fromBool".to_string(),
        h::ctor(|_v, args, _n| {
            let level = args.first().and_then(h::as_int).unwrap_or(0);
            let option = args.get(1).and_then(h::as_int).unwrap_or(0);
            let enabled = args.get(2).and_then(h::as_bool).unwrap_or(false);
            Ok(raw_socket_option_native(level, option, &[enabled as u8]))
        }),
    );
    for (name, value) in [("levelSocket", 1), ("levelTcp", 6), ("levelUdp", 17)] {
        bc.static_getters.insert(
            name.to_string(),
            h::static_getter(move |_v| Ok(Value::Int(value))),
        );
    }
    bc.getters.insert(
        "level".to_string(),
        h::getter(|_v, recv| Ok(list_get(&recv, 0).unwrap_or(Value::Int(0)))),
    );
    bc.getters.insert(
        "option".to_string(),
        h::getter(|_v, recv| Ok(list_get(&recv, 1).unwrap_or(Value::Int(0)))),
    );
    bc.getters.insert(
        "value".to_string(),
        h::getter(|_v, recv| Ok(list_get(&recv, 2).unwrap_or(Value::Null))),
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

fn connection_task_definition() -> BridgedClassDef {
    let mut bc = BridgedClassDef::new("ConnectionTask");
    bc.native_type = Some("d4rt::io::ConnectionTask".to_string());
    bc.getters.insert(
        "socket".to_string(),
        h::getter(|_v, recv| Ok(list_get(&recv, 0).unwrap_or(Value::Null))),
    );
    bc.methods.insert(
        "cancel".to_string(),
        h::method(|_v, recv, _a, _n| {
            let cancel = list_get(&recv, 1)
                .and_then(|f| async_state::unwrap_future(&f))
                .unwrap_or_else(|| Rc::new(RefCell::new(async_state::FutureState::new())));
            async_state::complete(&cancel, Ok(Value::Null));
            Ok(wrap_future(cancel))
        }),
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

/// Wires a RawSocket.listen-style event stream: forwards each data event as a
/// `RawSocketEvent.read`, and closes with `readClosed` then `closed`.
fn raw_listen_value(
    visitor: &mut InterpreterVisitor,
    stream: &Value,
    args: &[Value],
    named: &HashMap<String, Value>,
) -> Result<Value, InterpError> {
    let on_data = args.first().cloned().unwrap_or(Value::Null);
    let on_error = named.get("onError").cloned().unwrap_or(Value::Null);
    let on_done = named.get("onDone").cloned().unwrap_or(Value::Null);
    let read_ev = wrap("RawSocketEvent", Value::str("read"));
    let read_closed_ev = wrap("RawSocketEvent", Value::str("readClosed"));
    let closed_ev = wrap("RawSocketEvent", Value::str("closed"));
    let on_data_c = on_data.clone();
    let read_ev_c = read_ev.clone();
    let data_wrapper = Value::NativeFn(Rc::new(NativeFunction {
        name: Some("<raw listen data>".to_string()),
        min_arity: 1,
        max_arity: Some(1),
        is_async: false,
        is_generator: false,
        closure: Box::new(move |visitor, _args, _named| {
            if !on_data_c.is_null() {
                visitor.call_value(&on_data_c, vec![read_ev_c.clone()], HashMap::new())?;
            }
            Ok(Value::Null)
        }),
        callable_runtime_type: crate::rt::FunctionRuntimeType::untyped(),
    }));
    let on_data_d = on_data.clone();
    let on_done_c = on_done.clone();
    let read_closed_c = read_closed_ev.clone();
    let closed_c = closed_ev.clone();
    let done_wrapper = Value::NativeFn(Rc::new(NativeFunction {
        name: Some("<raw listen done>".to_string()),
        min_arity: 0,
        max_arity: Some(0),
        is_async: false,
        is_generator: false,
        closure: Box::new(move |visitor, _args, _named| {
            if !on_data_d.is_null() {
                visitor.call_value(&on_data_d, vec![read_closed_c.clone()], HashMap::new())?;
                visitor.call_value(&on_data_d, vec![closed_c.clone()], HashMap::new())?;
            }
            if !on_done_c.is_null() {
                visitor.call_value(&on_done_c, vec![], HashMap::new())?;
            }
            Ok(Value::Null)
        }),
        callable_runtime_type: crate::rt::FunctionRuntimeType::untyped(),
    }));
    let mut named2 = named.clone();
    named2.insert("onError".to_string(), on_error);
    named2.insert("onDone".to_string(), done_wrapper);
    sl::stream_listen_value(visitor, stream, &[data_wrapper], &named2)
}

fn raw_socket_definition() -> BridgedClassDef {
    let mut bc = BridgedClassDef::new("RawSocket");
    bc.native_type = Some("d4rt::io::RawSocket".to_string());
    bc.static_methods.insert(
        "connect".to_string(),
        h::static_method(|_v, args, named| Ok(socket_connect_value(&args, &named, "RawSocket"))),
    );
    bc.static_methods.insert(
        "startConnect".to_string(),
        h::static_method(|_v, args, named| {
            let (fut, _st) = socket_connect_parts(&args, &named, "RawSocket");
            Ok(connection_task(fut))
        }),
    );
    bc.methods.insert(
        "available".to_string(),
        h::method(|_v, recv, _a, _n| {
            let id = list_get(&recv, 0).and_then(|i| h::as_int(&i)).unwrap_or(0);
            let len = RAW_BUFS
                .lock()
                .unwrap()
                .get(&id)
                .map(|b| b.len() as i64)
                .unwrap_or(0);
            Ok(future_ok(Value::Int(len)))
        }),
    );
    bc.methods.insert(
        "read".to_string(),
        h::method(|_v, recv, args, _n| {
            let id = list_get(&recv, 0).and_then(|i| h::as_int(&i)).unwrap_or(0);
            let len = args.first().and_then(h::as_int).unwrap_or(1).max(1) as usize;
            let taken = RAW_BUFS
                .lock()
                .unwrap()
                .get_mut(&id)
                .map(|b| {
                    let n = b.len().min(len);

                    b.drain(..n).collect::<Vec<u8>>()
                })
                .filter(|out| !out.is_empty());
            match taken {
                Some(bytes) => Ok(future_ok(bytes_value(&bytes))),
                None => Ok(future_ok(Value::Null)),
            }
        }),
    );
    bc.methods.insert(
        "write".to_string(),
        h::method(|_v, recv, args, _n| {
            let data = args.first().map(ints_bytes).unwrap_or_default();
            let n = data.len() as i64;
            socket_send(&recv, &data);
            Ok(Value::Int(n))
        }),
    );
    bc.methods.insert(
        "close".to_string(),
        h::method(|_v, recv, _a, _n| {
            socket_ctrl(&recv, SocketCtrl::Close);
            if let Some(id) = list_get(&recv, 0).and_then(|i| h::as_int(&i)) {
                SOCKETS.lock().unwrap().remove(&id);
            }
            Ok(future_ok(Value::Null))
        }),
    );
    bc.methods.insert(
        "shutdown".to_string(),
        h::method(|_v, recv, _a, _n| {
            socket_ctrl(&recv, SocketCtrl::Close);
            Ok(Value::Null)
        }),
    );
    bc.methods
        .insert("setOption".to_string(), socket_set_option_method(8));
    bc.methods
        .insert("getRawOption".to_string(), socket_get_raw_option_method(8));
    bc.methods
        .insert("setRawOption".to_string(), socket_set_raw_option_method(8));
    bc.methods.insert(
        "listen".to_string(),
        h::method(|visitor, recv, args, named| {
            let stream = list_get(&recv, 2).unwrap_or(Value::Null);
            raw_listen_value(visitor, &stream, &args, &named)
        }),
    );
    bc.getters.insert(
        "port".to_string(),
        h::getter(|_v, recv| Ok(list_get(&recv, 5).unwrap_or(Value::Int(0)))),
    );
    bc.getters.insert(
        "remotePort".to_string(),
        h::getter(|_v, recv| Ok(list_get(&recv, 7).unwrap_or(Value::Int(0)))),
    );
    bc.getters.insert(
        "address".to_string(),
        h::getter(|_v, recv| {
            Ok(list_get(&recv, 4)
                .map(|a| wrap("InternetAddress", a))
                .unwrap_or(Value::Null))
        }),
    );
    bc.getters.insert(
        "remoteAddress".to_string(),
        h::getter(|_v, recv| {
            Ok(list_get(&recv, 6)
                .map(|a| wrap("InternetAddress", a))
                .unwrap_or(Value::Null))
        }),
    );
    bc.getters.insert(
        "readEventsEnabled".to_string(),
        h::getter(|_v, _r| Ok(Value::Bool(true))),
    );
    bc.getters.insert(
        "writeEventsEnabled".to_string(),
        h::getter(|_v, _r| Ok(Value::Bool(true))),
    );
    bc.setters.insert(
        "readEventsEnabled".to_string(),
        h::setter(|_v, _r, _val| Ok(())),
    );
    bc.setters.insert(
        "writeEventsEnabled".to_string(),
        h::setter(|_v, _r, _val| Ok(())),
    );
    bc.getters.insert(
        "first".to_string(),
        h::getter(|_v, recv| {
            let stream = list_get(&recv, 2).unwrap_or(Value::Null);
            sl::stream_future_op_getter(&stream, sl::FUTURE_OP_FIRST)
        }),
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

fn raw_server_socket_definition() -> BridgedClassDef {
    let mut bc = BridgedClassDef::new("RawServerSocket");
    bc.native_type = Some("d4rt::io::RawServerSocket".to_string());
    bc.static_methods.insert(
        "bind".to_string(),
        h::static_method(|_v, args, named| Ok(server_bind_value(&args, &named, "RawServerSocket"))),
    );
    bc.methods.insert(
        "close".to_string(),
        h::method(|visitor, recv, _a, _n| {
            close_server(&recv, visitor);
            Ok(future_ok(Value::Null))
        }),
    );
    bc.methods.insert(
        "listen".to_string(),
        h::method(|visitor, recv, args, named| {
            let stream = list_get(&recv, 2).unwrap_or(Value::Null);
            sl::stream_listen_value(visitor, &stream, &args, &named)
        }),
    );
    bc.getters.insert(
        "port".to_string(),
        h::getter(|_v, recv| Ok(list_get(&recv, 4).unwrap_or(Value::Int(0)))),
    );
    bc.getters.insert(
        "address".to_string(),
        h::getter(|_v, recv| {
            Ok(list_get(&recv, 5)
                .map(|a| wrap("InternetAddress", a))
                .unwrap_or(Value::Null))
        }),
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

// ---------- UDP ----------

/// Binds a UDP socket (RawDatagramSocket.bind), returning a future.
fn udp_bind_value(args: &[Value], _named: &HashMap<String, Value>) -> Value {
    let host = args
        .first()
        .map(|v| h::as_str(v).unwrap_or("0.0.0.0").to_string())
        .unwrap_or_else(|| "0.0.0.0".to_string());
    let port = args.get(1).and_then(h::as_int).unwrap_or(0);
    if let Err(e) = crate::permission::require_network(&host, port as u16) {
        return future_err(Value::str(e.to_string()));
    }
    match UdpSocket::bind((host.as_str(), port as u16)) {
        Ok(sock) => {
            let id = new_id();
            let local = sock
                .local_addr()
                .map(|a| (a.ip().to_string(), a.port() as i64))
                .unwrap_or((host, 0));
            let (data_tx, data_rx) = mpsc::channel();
            let (ctrl_tx, ctrl_rx) = mpsc::channel::<UdpCtrl>();
            UDP_SOCKS.lock().unwrap().insert(id, ctrl_tx);
            UDP_OPTS.lock().unwrap().insert(id, (true, 1, false));
            claim(crate::state::current().id, id, HttpResourceKind::Udp);
            let ctrl = sl::make_controller_value(false, false);
            let stream = sl::controller_stream_value(&ctrl);
            let done = Rc::new(RefCell::new(async_state::FutureState::new()));
            let sock_r = sock
                .try_clone()
                .unwrap_or_else(|_| sock.try_clone().unwrap());
            std::thread::spawn(move || {
                let mut buf = [0u8; 65535];
                loop {
                    match sock_r.recv_from(&mut buf) {
                        Ok((n, src)) => {
                            if data_tx
                                .send(UdpData::Packet {
                                    bytes: buf[..n].to_vec(),
                                    from: src.ip().to_string(),
                                    port: src.port() as i64,
                                })
                                .is_err()
                            {
                                break;
                            }
                        }
                        Err(_) => {
                            let _ = data_tx.send(UdpData::Closed);
                            break;
                        }
                    }
                }
            });
            let sock_w = sock;
            std::thread::spawn(move || {
                loop {
                    match ctrl_rx.try_recv() {
                        Ok(UdpCtrl::Send(data, host, port)) => {
                            let _ = sock_w.send_to(&data, (host.as_str(), port as u16));
                        }
                        Ok(UdpCtrl::Close) => break,
                        Err(TryRecvError::Empty) => {
                            std::thread::sleep(std::time::Duration::from_millis(1))
                        }
                        Err(TryRecvError::Disconnected) => break,
                    }
                }
            });
            async_state::enqueue_task(async_state::Task::UdpPoll {
                id,
                rx: data_rx,
                ctrl: ctrl.clone(),
                done: done.clone(),
            });
            future_ok(wrap(
                "RawDatagramSocket",
                Value::List(Rc::new(RefCell::new(vec![
                    Value::Int(id),
                    Value::Int(local.1),
                    Value::str(local.0),
                    stream,
                    Value::Null,                                    // 4
                    Value::Null,                                    // 5
                    Value::Null,                                    // 6
                    Value::Null,                                    // 7
                    Value::List(Rc::new(RefCell::new(Vec::new()))), // 8 raw options
                    Value::List(Rc::new(RefCell::new(Vec::new()))), // 9 multicast memberships
                ]))),
            ))
        }
        Err(e) => future_err(Value::str(e.to_string())),
    }
}

/// Drains a UDP socket's datagram channel into its queue and event stream.
pub fn poll_udp(
    visitor: &mut InterpreterVisitor,
    id: i64,
    rx: &Receiver<UdpData>,
    ctrl: &Value,
    done: &Rc<RefCell<async_state::FutureState>>,
) -> bool {
    loop {
        match rx.try_recv() {
            Ok(UdpData::Packet { bytes, from, port }) => {
                UDP_QUEUE
                    .lock()
                    .unwrap()
                    .entry(id)
                    .or_default()
                    .push_back((bytes, from, port));
                sl::controller_add_value(visitor, ctrl, wrap("RawSocketEvent", Value::str("read")));
            }
            Ok(UdpData::Closed) => {
                let _ = sl::controller_close_value(visitor, ctrl);
                async_state::complete(done, Ok(Value::Null));
                UDP_SOCKS.lock().unwrap().remove(&id);
                UDP_QUEUE.lock().unwrap().remove(&id);
                UDP_OPTS.lock().unwrap().remove(&id);
                return false;
            }
            Err(TryRecvError::Empty) => break,
            Err(TryRecvError::Disconnected) => {
                UDP_SOCKS.lock().unwrap().remove(&id);
                UDP_QUEUE.lock().unwrap().remove(&id);
                UDP_OPTS.lock().unwrap().remove(&id);
                return false;
            }
        }
    }
    true
}

fn datagram_definition() -> BridgedClassDef {
    let mut bc = BridgedClassDef::new("Datagram");
    bc.native_type = Some("d4rt::io::Datagram".to_string());
    bc.constructors.insert(
        "".to_string(),
        h::ctor(|_v, args, _n| {
            let data = args.first().map(ints_bytes).unwrap_or_default();
            let address = args
                .get(1)
                .map(|a| wrap("InternetAddress", a.clone()))
                .unwrap_or_else(|| wrap("InternetAddress", Value::str("0.0.0.0")));
            let port = args.get(2).and_then(h::as_int).unwrap_or(0);
            Ok(wrap(
                "Datagram",
                Value::List(Rc::new(RefCell::new(vec![
                    bytes_value(&data),
                    address,
                    Value::Int(port),
                ]))),
            ))
        }),
    );
    bc.getters.insert(
        "data".to_string(),
        h::getter(|_v, recv| Ok(list_get(&recv, 0).unwrap_or(Value::Null))),
    );
    bc.getters.insert(
        "address".to_string(),
        h::getter(|_v, recv| Ok(list_get(&recv, 1).unwrap_or(Value::Null))),
    );
    bc.getters.insert(
        "port".to_string(),
        h::getter(|_v, recv| Ok(list_get(&recv, 2).unwrap_or(Value::Int(0)))),
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

fn raw_datagram_socket_definition() -> BridgedClassDef {
    let mut bc = BridgedClassDef::new("RawDatagramSocket");
    bc.native_type = Some("d4rt::io::RawDatagramSocket".to_string());
    bc.static_methods.insert(
        "bind".to_string(),
        h::static_method(|_v, args, named| Ok(udp_bind_value(&args, &named))),
    );
    bc.methods.insert(
        "receive".to_string(),
        h::method(|_v, recv, _a, _n| {
            let id = list_get(&recv, 0).and_then(|i| h::as_int(&i)).unwrap_or(0);
            let item = UDP_QUEUE
                .lock()
                .unwrap()
                .get_mut(&id)
                .and_then(|q| q.pop_front());
            match item {
                Some((bytes, from, port)) => Ok(wrap(
                    "Datagram",
                    Value::List(Rc::new(RefCell::new(vec![
                        bytes_value(&bytes),
                        wrap("InternetAddress", Value::str(&from)),
                        Value::Int(port),
                    ]))),
                )),
                None => Ok(Value::Null),
            }
        }),
    );
    bc.methods.insert(
        "send".to_string(),
        h::method(|_v, recv, args, _n| {
            let id = list_get(&recv, 0).and_then(|i| h::as_int(&i)).unwrap_or(0);
            let data = args.first().map(ints_bytes).unwrap_or_default();
            let len = data.len() as i64;
            let host = args
                .get(1)
                .and_then(|a| h::as_str(a))
                .unwrap_or("")
                .to_string();
            let port = args.get(2).and_then(h::as_int).unwrap_or(0);
            if let Some(tx) = UDP_SOCKS.lock().unwrap().get(&id) {
                let _ = tx.send(UdpCtrl::Send(data, host, port));
            }
            Ok(Value::Int(len))
        }),
    );
    bc.methods.insert(
        "close".to_string(),
        h::method(|_v, recv, _a, _n| {
            let id = list_get(&recv, 0).and_then(|i| h::as_int(&i)).unwrap_or(0);
            if let Some(tx) = UDP_SOCKS.lock().unwrap().get(&id) {
                let _ = tx.send(UdpCtrl::Close);
            }
            Ok(future_ok(Value::Null))
        }),
    );
    bc.methods.insert(
        "listen".to_string(),
        h::method(|visitor, recv, args, named| {
            let stream = list_get(&recv, 3).unwrap_or(Value::Null);
            raw_listen_value(visitor, &stream, &args, &named)
        }),
    );
    bc.methods.insert(
        "joinMulticast".to_string(),
        h::method(|_v, recv, args, _n| {
            let group: Option<String> = match args.first().and_then(h::as_list) {
                Some(l) => l
                    .borrow()
                    .first()
                    .cloned()
                    .and_then(|g| h::as_str(&g).map(|s| s.to_string())),
                None => None,
            };
            if let Some(group) = &group {
                socket_toggle_multicast(&recv, group, true);
            }
            Ok(future_ok(Value::Null))
        }),
    );
    bc.methods.insert(
        "leaveMulticast".to_string(),
        h::method(|_v, recv, args, _n| {
            let group: Option<String> = match args.first().and_then(h::as_list) {
                Some(l) => l
                    .borrow()
                    .first()
                    .cloned()
                    .and_then(|g| h::as_str(&g).map(|s| s.to_string())),
                None => None,
            };
            if let Some(group) = &group {
                socket_toggle_multicast(&recv, group, false);
            }
            Ok(future_ok(Value::Null))
        }),
    );
    bc.methods
        .insert("setOption".to_string(), socket_set_option_method(8));
    bc.methods
        .insert("getRawOption".to_string(), socket_get_raw_option_method(8));
    bc.methods
        .insert("setRawOption".to_string(), socket_set_raw_option_method(8));
    bc.getters.insert(
        "address".to_string(),
        h::getter(|_v, recv| {
            Ok(list_get(&recv, 2)
                .map(|a| wrap("InternetAddress", a))
                .unwrap_or(Value::Null))
        }),
    );
    bc.getters.insert(
        "port".to_string(),
        h::getter(|_v, recv| Ok(list_get(&recv, 1).unwrap_or(Value::Int(0)))),
    );
    bc.getters.insert(
        "readEventsEnabled".to_string(),
        h::getter(|_v, _r| Ok(Value::Bool(true))),
    );
    bc.getters.insert(
        "writeEventsEnabled".to_string(),
        h::getter(|_v, _r| Ok(Value::Bool(true))),
    );
    bc.getters.insert(
        "multicastLoopback".to_string(),
        h::getter(|_v, recv| {
            let id = list_get(&recv, 0).and_then(|i| h::as_int(&i)).unwrap_or(0);
            let v = UDP_OPTS
                .lock()
                .unwrap()
                .get(&id)
                .map(|o| o.0)
                .unwrap_or(true);
            Ok(Value::Bool(v))
        }),
    );
    bc.getters.insert(
        "multicastHops".to_string(),
        h::getter(|_v, recv| {
            let id = list_get(&recv, 0).and_then(|i| h::as_int(&i)).unwrap_or(0);
            let v = UDP_OPTS.lock().unwrap().get(&id).map(|o| o.1).unwrap_or(1);
            Ok(Value::Int(v))
        }),
    );
    bc.getters.insert(
        "broadcastEnabled".to_string(),
        h::getter(|_v, recv| {
            let id = list_get(&recv, 0).and_then(|i| h::as_int(&i)).unwrap_or(0);
            let v = UDP_OPTS
                .lock()
                .unwrap()
                .get(&id)
                .map(|o| o.2)
                .unwrap_or(false);
            Ok(Value::Bool(v))
        }),
    );
    bc.setters.insert(
        "readEventsEnabled".to_string(),
        h::setter(|_v, _r, _val| Ok(())),
    );
    bc.setters.insert(
        "writeEventsEnabled".to_string(),
        h::setter(|_v, _r, _val| Ok(())),
    );
    bc.setters.insert(
        "multicastLoopback".to_string(),
        h::setter(|_v, recv, val| {
            let id = list_get(&recv, 0).and_then(|i| h::as_int(&i)).unwrap_or(0);
            if let Some(o) = UDP_OPTS.lock().unwrap().get_mut(&id) {
                o.0 = h::as_bool(&val).unwrap_or(true);
            }
            Ok(())
        }),
    );
    bc.setters.insert(
        "multicastHops".to_string(),
        h::setter(|_v, recv, val| {
            let id = list_get(&recv, 0).and_then(|i| h::as_int(&i)).unwrap_or(0);
            if let Some(o) = UDP_OPTS.lock().unwrap().get_mut(&id) {
                o.1 = h::as_int(&val).unwrap_or(1);
            }
            Ok(())
        }),
    );
    bc.setters.insert(
        "broadcastEnabled".to_string(),
        h::setter(|_v, recv, val| {
            let id = list_get(&recv, 0).and_then(|i| h::as_int(&i)).unwrap_or(0);
            if let Some(o) = UDP_OPTS.lock().unwrap().get_mut(&id) {
                o.2 = h::as_bool(&val).unwrap_or(false);
            }
            Ok(())
        }),
    );
    bc.getters.insert(
        "first".to_string(),
        h::getter(|_v, recv| {
            let stream = list_get(&recv, 3).unwrap_or(Value::Null);
            sl::stream_future_op_getter(&stream, sl::FUTURE_OP_FIRST)
        }),
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

// ---------- NetworkInterface ----------

fn network_interface_value(name: &str, index: i64, addresses: Vec<String>) -> Value {
    let addrs = addresses
        .into_iter()
        .map(|a| wrap("InternetAddress", Value::str(a)))
        .collect();
    wrap(
        "NetworkInterface",
        Value::List(Rc::new(RefCell::new(vec![
            Value::str(name),
            Value::Int(index),
            Value::List(Rc::new(RefCell::new(addrs))),
        ]))),
    )
}

fn network_interface_definition() -> BridgedClassDef {
    let mut bc = BridgedClassDef::new("NetworkInterface");
    bc.native_type = Some("d4rt::io::NetworkInterface".to_string());
    bc.static_methods.insert(
        "list".to_string(),
        h::static_method(|_v, _args, named| {
            crate::permission::require_network_any()?;
            let include_loopback = named
                .get("includeLoopback")
                .and_then(h::as_bool)
                .unwrap_or(false);
            let include_link_local = named
                .get("includeLinkLocal")
                .and_then(h::as_bool)
                .unwrap_or(false);
            let type_name = named
                .get("type")
                .and_then(h::as_str)
                .unwrap_or("any")
                .to_string();
            let ifaces = list_interfaces();
            let list: Vec<Value> = ifaces
                .into_iter()
                .filter_map(|(name, index, mut addrs)| {
                    if !include_link_local {
                        // Drop link-local addresses; an interface with no
                        // addresses left is excluded entirely.
                        addrs.retain(|a| !(a.starts_with("169.254.") || a.starts_with("fe80:")));
                    }
                    if !include_loopback && name == "lo" {
                        return None;
                    }
                    if type_name == "IPv4" && !addrs.iter().any(|a| !a.contains(':')) {
                        return None;
                    }
                    if type_name == "IPv6" && !addrs.iter().any(|a| a.contains(':')) {
                        return None;
                    }
                    if addrs.is_empty() {
                        return None;
                    }
                    Some(network_interface_value(&name, index, addrs))
                })
                .collect();
            Ok(future_ok(Value::List(Rc::new(RefCell::new(list)))))
        }),
    );
    bc.getters.insert(
        "name".to_string(),
        h::getter(|_v, recv| Ok(list_get(&recv, 0).unwrap_or(Value::Null))),
    );
    bc.getters.insert(
        "index".to_string(),
        h::getter(|_v, recv| Ok(list_get(&recv, 1).unwrap_or(Value::Int(0)))),
    );
    bc.getters.insert(
        "addresses".to_string(),
        h::getter(|_v, recv| Ok(list_get(&recv, 2).unwrap_or(Value::Null))),
    );
    bc.methods.insert(
        "toString".into(),
        h::method(|_v, t, _a, _n| {
            let name = list_get(&t, 0)
                .and_then(|n| h::as_str(&n).map(|s| s.to_string()))
                .unwrap_or_default();
            Ok(Value::str(format!("NetworkInterface({name})")))
        }),
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

/// Enumerates network interfaces via getifaddrs (Linux/POSIX).
#[cfg(any(target_os = "linux", target_os = "macos"))]
fn list_interfaces() -> Vec<(String, i64, Vec<String>)> {
    use std::ffi::CStr;
    use std::os::raw::{c_char, c_uint, c_void};

    #[repr(C)]
    struct Ifaddrs {
        ifa_next: *mut Ifaddrs,
        ifa_name: *mut c_char,
        ifa_flags: c_uint,
        ifa_addr: *mut c_void,
        ifa_netmask: *mut c_void,
        ifa_ifu: *mut c_void,
        ifa_data: *mut c_void,
    }

    unsafe extern "C" {
        fn getifaddrs(ifap: *mut *mut Ifaddrs) -> i32;
        fn freeifaddrs(ifap: *mut Ifaddrs);
    }

    const AF_INET: u16 = 2;
    const AF_INET6: u16 = 10;

    let mut out: Vec<(String, i64, Vec<String>)> = Vec::new();
    let mut head: *mut Ifaddrs = std::ptr::null_mut();
    if unsafe { getifaddrs(&mut head) } != 0 {
        return out;
    }
    let mut cur = head;
    while !cur.is_null() {
        unsafe {
            let ifa = &*cur;
            if !ifa.ifa_name.is_null() {
                let name = CStr::from_ptr(ifa.ifa_name).to_string_lossy().to_string();
                let addr = ifa.ifa_addr as *const u8;
                if !addr.is_null() {
                    let family = *(addr as *const u16);
                    let ip = match family {
                        AF_INET => {
                            let b = std::slice::from_raw_parts(addr.add(4), 4);
                            format!("{}.{}.{}.{}", b[0], b[1], b[2], b[3])
                        }
                        AF_INET6 => {
                            let b = std::slice::from_raw_parts(addr.add(8), 16);
                            let mut oct = [0u8; 16];
                            oct.copy_from_slice(b);
                            std::net::Ipv6Addr::from(oct).to_string()
                        }
                        _ => String::new(),
                    };
                    if !ip.is_empty()
                        && let Some((_, _, addrs)) = out.iter_mut().find(|(n, _, _)| *n == name)
                    {
                        addrs.push(ip);
                    } else if !ip.is_empty() {
                        out.push((name.clone(), 0, vec![ip]));
                    }
                }
            }
            cur = ifa.ifa_next;
        }
    }
    unsafe { freeifaddrs(head) };
    for (name, index, _) in &mut out {
        *index = std::fs::read_to_string(format!("/sys/class/net/{name}/ifindex"))
            .ok()
            .and_then(|s| s.trim().parse().ok())
            .unwrap_or(0);
    }
    out
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn list_interfaces() -> Vec<(String, i64, Vec<String>)> {
    Vec::new()
}

fn server_bind_value(args: &[Value], _named: &HashMap<String, Value>, class: &str) -> Value {
    let host = args
        .first()
        .map(|v| h::as_str(v).unwrap_or("0.0.0.0").to_string())
        .unwrap_or_default();
    let port = args.get(1).and_then(h::as_int).unwrap_or(0);
    if let Err(e) = crate::permission::require_network(&host, port as u16) {
        return future_err(Value::str(e.to_string()));
    }
    match TcpListener::bind((host.as_str(), port as u16)) {
        Ok(listener) => {
            let (id, ctrl, stream, done, local_host, local_port, data_rx) =
                start_server(listener, host, true, crate::state::current().id);
            if class == "RawServerSocket" {
                RAW_SERVERS.lock().unwrap().insert(id);
                claim(crate::state::current().id, id, HttpResourceKind::RawServer);
            }
            async_state::enqueue_task(async_state::Task::ServerPoll {
                id,
                rx: data_rx,
                ctrl: ctrl.clone(),
            });
            future_ok(wrap(
                class,
                Value::List(Rc::new(RefCell::new(vec![
                    Value::Int(id),
                    ctrl,
                    stream,
                    done,
                    Value::Int(local_port),
                    Value::str(local_host),
                ]))),
            ))
        }
        Err(e) => future_err(Value::str(e.to_string())),
    }
}

fn server_socket_definition() -> BridgedClassDef {
    let mut bc = BridgedClassDef::new("ServerSocket");
    bc.native_type = Some("d4rt::io::ServerSocket".to_string());
    bc.static_methods.insert(
        "bind".to_string(),
        h::static_method(|_v, args, named| Ok(server_bind_value(&args, &named, "ServerSocket"))),
    );
    bc.methods.insert(
        "close".to_string(),
        h::method(|visitor, recv, _a, _n| {
            close_server(&recv, visitor);
            Ok(future_ok(Value::Null))
        }),
    );
    bc.methods.insert(
        "listen".to_string(),
        h::method(|visitor, recv, args, named| {
            let stream = list_get(&recv, 2).unwrap_or(Value::Null);
            sl::stream_listen_value(visitor, &stream, &args, &named)
        }),
    );
    bc.getters.insert(
        "port".to_string(),
        h::getter(|_v, recv| Ok(list_get(&recv, 4).unwrap_or(Value::Int(0)))),
    );
    bc.getters.insert(
        "address".to_string(),
        h::getter(|_v, recv| {
            Ok(list_get(&recv, 5)
                .map(|a| wrap("InternetAddress", a))
                .unwrap_or(Value::Null))
        }),
    );
    sl::delegate_stream(&mut bc, 2);
    bc.methods.insert(
        "toString".to_string(),
        h::method(|_v, recv, _a, _n| {
            let port = list_get(&recv, 4).and_then(|p| h::as_int(&p)).unwrap_or(0);
            Ok(Value::str(format!("ServerSocket:{port}")))
        }),
    );
    bc
}

/// Binds a listener (called from ServerSocket.bind / HttpServer.bind).
/// Returns the server id, its stream controller pieces, local address,
/// and the channel the interpreter's poll task must drain.
fn start_server(
    listener: TcpListener,
    host: String,
    raw: bool,
    ctx_id: i64,
) -> (i64, Value, Value, Value, String, i64, Receiver<ServerData>) {
    let id = new_id();
    let ctrl = sl::make_controller_value(false, false);
    let stream = sl::controller_stream_value(&ctrl);
    let done = Rc::new(RefCell::new(async_state::FutureState::new()));
    let local = listener
        .local_addr()
        .map(|a| (a.ip().to_string(), a.port() as i64))
        .unwrap_or((host, 0));
    let (data_tx, data_rx) = mpsc::channel();
    let (close_tx, close_rx) = mpsc::channel();
    SERVER_CLOSE.lock().unwrap().insert(id, close_tx);
    claim(ctx_id, id, HttpResourceKind::Server);
    let data_tx_s = data_tx.clone();
    std::thread::spawn(move || {
        let listener = listener;
        let _ = listener.set_nonblocking(true);
        loop {
            if close_rx.try_recv().is_ok() {
                break;
            }
            match listener.accept() {
                Ok((stream, _addr)) => {
                    let dt = data_tx_s.clone();
                    if raw {
                        let peer = stream
                            .peer_addr()
                            .map(|p| p.ip().to_string())
                            .unwrap_or_default();
                        let (sdata_tx, sdata_rx) = mpsc::channel();
                        let id = spawn_socket_pair(stream, sdata_tx, ctx_id);
                        let _ = dt.send(ServerData::SocketAccepted {
                            id,
                            peer,
                            data_rx: sdata_rx,
                        });
                        continue;
                    }
                    std::thread::spawn(move || {
                        // Parse one HTTP request, hand it to the interpreter,
                        // then wait for the response and write it back.
                        let mut stream = stream;
                        let mut buf = Vec::new();
                        let mut tmp = [0u8; 8192];
                        let mut head_end = None;
                        while head_end.is_none() {
                            match stream.read(&mut tmp) {
                                Ok(0) | Err(_) => break,
                                Ok(n) => {
                                    buf.extend_from_slice(&tmp[..n]);
                                    head_end = find_head_end(&buf);
                                }
                            }
                        }
                        let end = match head_end {
                            Some(e) => e,
                            None => return,
                        };
                        let (method, path, headers) = match parse_head(&buf[..end]) {
                            Some(h) => h,
                            None => return,
                        };
                        let body = read_body(&mut stream, &headers, &buf[end..]);
                        let peer = stream
                            .peer_addr()
                            .map(|p| p.ip().to_string())
                            .unwrap_or_default();
                        let conn_id = new_id();
                        let (conn_tx, conn_rx) = mpsc::channel::<ConnCtrl>();
                        SERVER_CONNS.lock().unwrap().insert(conn_id, conn_tx);
                        claim(ctx_id, conn_id, HttpResourceKind::ServerConn);
                        let _ = dt.send(ServerData::Accepted {
                            conn_id,
                            method,
                            path,
                            headers,
                            body,
                            peer,
                        });
                        let mut w = stream
                            .try_clone()
                            .unwrap_or_else(|_| stream.try_clone().unwrap());
                        loop {
                            match conn_rx.try_recv() {
                                Ok(ConnCtrl::Write(bytes)) => {
                                    let _ = w.write_all(&bytes);
                                    break;
                                }
                                Err(_) => std::thread::sleep(std::time::Duration::from_millis(1)),
                            }
                        }
                        let _ = w.shutdown(Shutdown::Both);
                        SERVER_CONNS.lock().unwrap().remove(&conn_id);
                    });
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(std::time::Duration::from_millis(1));
                }
                Err(_) => break,
            }
        }
    });
    (
        id,
        ctrl,
        stream,
        wrap_future(done),
        local.0,
        local.1,
        data_rx,
    )
}

fn find_head_end(buf: &[u8]) -> Option<usize> {
    buf.windows(4).position(|w| w == b"\r\n\r\n").map(|i| i + 4)
}

fn parse_head(head: &[u8]) -> Option<HttpHead> {
    let text = String::from_utf8_lossy(head);
    let mut lines = text.lines();
    let first = lines.next()?;
    let mut parts = first.split_whitespace();
    let method = parts.next()?.to_string();
    let path = parts.next()?.to_string();
    let mut headers = Vec::new();
    for line in lines {
        if let Some((k, v)) = line.split_once(':') {
            headers.push((k.trim().to_string(), v.trim().to_string()));
        }
    }
    Some((method, path, headers))
}

fn read_body(stream: &mut TcpStream, headers: &[(String, String)], rest: &[u8]) -> Vec<u8> {
    let mut body = rest.to_vec();
    let content_length: usize = headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case("content-length"))
        .and_then(|(_, v)| v.trim().parse().ok())
        .unwrap_or(0);
    while body.len() < content_length {
        let mut tmp = [0u8; 8192];
        match stream.read(&mut tmp) {
            Ok(0) | Err(_) => break,
            Ok(n) => body.extend_from_slice(&tmp[..n]),
        }
    }
    body.truncate(content_length);
    body
}

fn close_server(v: &Value, visitor: &mut InterpreterVisitor) {
    if let Some(id) = list_get(v, 0).and_then(|i| h::as_int(&i)) {
        RAW_SERVERS.lock().unwrap().remove(&id);
        if let Some(tx) = SERVER_CLOSE.lock().unwrap().remove(&id) {
            let _ = tx.send(());
        }
    }
    if let Some(ctrl) = list_get(v, 1) {
        let _ = sl::controller_close_value(visitor, &ctrl);
    }
    if let Some(Value::Future(st)) = list_get(v, 3) {
        async_state::complete(&st, Ok(Value::Null));
    }
}

fn internet_address_definition() -> BridgedClassDef {
    let mut bc = BridgedClassDef::new("InternetAddress");
    bc.native_type = Some("d4rt::io::InternetAddress".to_string());
    bc.constructors.insert(
        "".to_string(),
        h::ctor(|_v, args, _n| {
            let addr = args.first().and_then(h::as_str).unwrap_or("").to_string();
            Ok(wrap("InternetAddress", Value::str(addr)))
        }),
    );
    bc.static_methods.insert(
        "parse".to_string(),
        h::static_method(|_v, args, _n| {
            let addr = args.first().and_then(h::as_str).unwrap_or("").to_string();
            Ok(wrap("InternetAddress", Value::str(addr)))
        }),
    );
    for (name, value) in [
        ("anyIPv4", "0.0.0.0"),
        ("anyIPv6", "::"),
        ("loopbackIPv4", "127.0.0.1"),
        ("loopbackIPv6", "::1"),
    ] {
        bc.static_getters.insert(
            name.to_string(),
            h::static_getter(move |_v| Ok(wrap("InternetAddress", Value::str(value)))),
        );
    }
    bc.getters.insert(
        "address".to_string(),
        h::getter(|_v, recv| Ok(h::as_str(&recv).map(Value::str).unwrap_or(Value::Null))),
    );
    bc.getters.insert(
        "type".to_string(),
        h::getter(|_v, recv| {
            let addr = h::as_str(&recv).unwrap_or("");
            let ty = if addr.contains(':') { "IPv6" } else { "IPv4" };
            Ok(wrap("InternetAddressType", Value::str(ty)))
        }),
    );
    bc.getters.insert(
        "isLoopback".to_string(),
        h::getter(|_v, recv| {
            let addr = h::as_str(&recv).unwrap_or("");
            Ok(Value::Bool(
                addr.starts_with("127.") || addr == "::1" || addr == "localhost",
            ))
        }),
    );
    bc.getters.insert(
        "isLinkLocal".to_string(),
        h::getter(|_v, recv| {
            let addr = h::as_str(&recv).unwrap_or("");
            Ok(Value::Bool(
                addr.starts_with("169.254.") || addr.starts_with("fe80"),
            ))
        }),
    );
    bc.getters.insert(
        "isMulticast".to_string(),
        h::getter(|_v, recv| {
            let addr = h::as_str(&recv).unwrap_or("");
            let first: i64 = addr
                .split('.')
                .next()
                .and_then(|s| s.parse().ok())
                .unwrap_or(-1);
            Ok(Value::Bool(
                (224..=239).contains(&first) || addr.starts_with("ff0"),
            ))
        }),
    );
    bc.getters.insert(
        "rawAddress".to_string(),
        h::getter(|_v, recv| {
            let addr = h::as_str(&recv).unwrap_or("");
            if addr.contains(':') {
                Ok(bytes_value(addr.as_bytes()))
            } else {
                let parts: Vec<u8> = addr.split('.').filter_map(|s| s.parse().ok()).collect();
                Ok(bytes_value(&parts))
            }
        }),
    );
    bc.static_methods.insert(
        "lookup".to_string(),
        h::static_method(|_v, args, _n| {
            let addr = args
                .first()
                .and_then(h::as_str)
                .unwrap_or("localhost")
                .to_string();
            Ok(future_ok(Value::List(Rc::new(RefCell::new(vec![wrap(
                "InternetAddress",
                Value::str(addr),
            )])))))
        }),
    );
    bc.static_methods.insert(
        "fromRawAddress".to_string(),
        h::static_method(|_v, args, named| {
            let raw = args.first().cloned().unwrap_or(Value::Null);
            let ty = named
                .get("type")
                .and_then(|t| h::as_str(t))
                .unwrap_or("IPv4");
            let addr = match h::as_list(&raw) {
                Some(l) => {
                    let bytes: Vec<u8> = l
                        .borrow()
                        .iter()
                        .filter_map(|x| h::as_int(x).map(|i| i as u8))
                        .collect();
                    if ty == "IPv6" {
                        let mut s = String::new();
                        for i in (0..bytes.len()).step_by(2) {
                            if i > 0 {
                                s.push(':');
                            }
                            let hi = bytes.get(i).copied().unwrap_or(0) as u16;
                            let lo = bytes.get(i + 1).copied().unwrap_or(0) as u16;
                            s.push_str(&format!("{:x}", (hi << 8) | lo));
                        }
                        s
                    } else {
                        bytes
                            .iter()
                            .map(|b| b.to_string())
                            .collect::<Vec<_>>()
                            .join(".")
                    }
                }
                None => String::new(),
            };
            Ok(wrap("InternetAddress", Value::str(addr)))
        }),
    );
    bc.static_methods.insert(
        "tryParse".to_string(),
        h::static_method(|_v, args, _n| {
            let addr = args.first().and_then(h::as_str).unwrap_or("").to_string();
            Ok(wrap("InternetAddress", Value::str(addr)))
        }),
    );
    bc.methods.insert(
        "reverse".to_string(),
        h::method(|_v, recv, _a, _n| {
            let addr = h::as_str(&recv).unwrap_or("").to_string();
            Ok(future_ok(wrap("InternetAddress", Value::str(addr))))
        }),
    );
    bc.getters.insert(
        "host".to_string(),
        h::getter(|_v, recv| Ok(Value::str(h::as_str(&recv).unwrap_or_default()))),
    );
    bc.methods.insert(
        "toString".to_string(),
        h::method(|_v, recv, _a, _n| Ok(Value::str(h::as_str(&recv).unwrap_or("")))),
    );
    bc
}

fn internet_address_type_definition() -> BridgedClassDef {
    let mut bc = BridgedClassDef::new("InternetAddressType");
    bc.native_type = Some("d4rt::io::InternetAddressType".to_string());
    for name in ["IPv4", "IPv6", "unix"] {
        bc.static_getters.insert(
            name.to_string(),
            h::static_getter(move |_v| Ok(wrap("InternetAddressType", Value::str(name)))),
        );
    }
    bc.getters.insert(
        "name".to_string(),
        h::getter(|_v, recv| Ok(Value::str(h::as_str(&recv).unwrap_or_default()))),
    );
    bc
}

fn socket_exception_definition() -> BridgedClassDef {
    let mut bc = BridgedClassDef::new("SocketException");
    bc.native_type = Some("d4rt::io::SocketException".to_string());
    bc.constructors.insert(
        "".to_string(),
        h::ctor(|_v, args, _n| {
            let message = args
                .first()
                .map(|v| h::as_str(v).unwrap_or("").to_string())
                .unwrap_or_default();
            let host = args
                .get(1)
                .map(|v| h::as_str(v).unwrap_or("").to_string())
                .unwrap_or_default();
            let port = args.get(2).and_then(h::as_int).unwrap_or(0);
            Ok(socket_exception(&message, &host, port))
        }),
    );
    bc.constructors.insert(
        "closed".to_string(),
        h::ctor(|_v, args, _n| {
            let message = args
                .first()
                .map(|v| h::as_str(v).unwrap_or("").to_string())
                .unwrap_or_default();
            let host = args
                .get(1)
                .map(|v| h::as_str(v).unwrap_or("").to_string())
                .unwrap_or_default();
            let port = args.get(2).and_then(h::as_int).unwrap_or(0);
            Ok(socket_exception(&message, &host, port))
        }),
    );
    bc.methods.insert(
        "toString".to_string(),
        h::method(|_v, recv, _a, _n| {
            let l = h::as_list(&recv)
                .map(|l| l.borrow().clone())
                .unwrap_or_default();
            let msg = l
                .first()
                .and_then(|m| h::as_str(m))
                .unwrap_or("SocketException");
            let host = l.get(1).and_then(|m| h::as_str(m)).unwrap_or("unknown");
            let port = l.get(2).and_then(h::as_int).unwrap_or(0);
            Ok(Value::str(format!(
                "SocketException: {msg}, host = {host}, port = {port}"
            )))
        }),
    );
    bc
}

// ---------- http client ----------

fn request_native(method: &str, host: &str, port: i64, path: &str) -> Value {
    let id = new_id();
    REQUEST_BUFS.lock().unwrap().insert(id, Vec::new());
    claim(crate::state::current().id, id, HttpResourceKind::RequestBuf);
    wrap(
        "HttpClientRequest",
        Value::List(Rc::new(RefCell::new(vec![
            Value::Int(id),
            Value::str(method),
            Value::str(host),
            Value::Int(port),
            Value::str(path),
            headers_value(Vec::new()),
            wrap_future(Rc::new(RefCell::new(async_state::FutureState::new()))),
            Value::List(Rc::new(RefCell::new(Vec::new()))), // 7 raw options
        ]))),
    )
}

fn http_client_definition() -> BridgedClassDef {
    let mut bc = BridgedClassDef::new("HttpClient");
    bc.native_type = Some("d4rt::io::HttpClient".to_string());
    bc.constructors.insert(
        "".to_string(),
        h::ctor(|_v, _args, _n| Ok(wrap("HttpClient", Value::Null))),
    );
    for method in ["get", "post", "put", "delete", "head", "patch"] {
        let m = method.to_string();
        bc.methods.insert(
            format!("{method}Url"),
            h::method(move |_v, _recv, args, _n| {
                let uri = args.first().cloned().unwrap_or(Value::Null);
                let (host, port, path) = parse_uri(&uri, &m);
                Ok(future_ok(request_native(&m, &host, port, &path)))
            }),
        );
        let m2 = method.to_string();
        bc.methods.insert(
            method.to_string(),
            h::method(move |_v, _recv, args, _n| {
                let host = args
                    .first()
                    .and_then(h::as_str)
                    .unwrap_or("localhost")
                    .to_string();
                let port = args.get(1).and_then(h::as_int).unwrap_or(80);
                let path = args.get(2).and_then(h::as_str).unwrap_or("/").to_string();
                Ok(future_ok(request_native(&m2, &host, port, &path)))
            }),
        );
    }
    bc.methods.insert(
        "openUrl".to_string(),
        h::method(|_v, _recv, args, _n| {
            let method = args
                .first()
                .and_then(h::as_str)
                .unwrap_or("GET")
                .to_string();
            let uri = args.get(1).cloned().unwrap_or(Value::Null);
            let (host, port, path) = parse_uri(&uri, &method);
            Ok(future_ok(request_native(&method, &host, port, &path)))
        }),
    );
    bc.methods.insert(
        "open".to_string(),
        h::method(|_v, _recv, args, _n| {
            let method = args
                .first()
                .and_then(h::as_str)
                .unwrap_or("GET")
                .to_string();
            let host = args
                .get(1)
                .and_then(h::as_str)
                .unwrap_or("localhost")
                .to_string();
            let port = args.get(2).and_then(h::as_int).unwrap_or(80);
            let path = args.get(3).and_then(h::as_str).unwrap_or("/").to_string();
            Ok(future_ok(request_native(&method, &host, port, &path)))
        }),
    );
    bc.methods.insert(
        "close".to_string(),
        h::method(|_v, _r, _a, _n| Ok(future_ok(Value::Null))),
    );
    for (name, default) in [
        ("idleTimeout", "null"),
        ("connectionTimeout", "null"),
        ("maxConnectionsPerHost", "null"),
        ("autoUncompress", "true"),
        ("userAgent", "dart-io-client/1.0"),
    ] {
        bc.getters.insert(
            name.to_string(),
            h::getter(move |_v, _r| {
                Ok(match default {
                    "null" => Value::Null,
                    "true" => Value::Bool(true),
                    s => Value::str(s),
                })
            }),
        );
        bc.setters
            .insert(name.to_string(), h::setter(|_v, _r, _val| Ok(())));
    }
    bc.static_getters.insert(
        "defaultHttpPort".to_string(),
        h::static_getter(|_v| Ok(Value::Int(80))),
    );
    bc.static_getters.insert(
        "defaultHttpsPort".to_string(),
        h::static_getter(|_v| Ok(Value::Int(443))),
    );
    bc
}

/// Splits a Uri value into (host, port, path).
fn parse_uri(v: &Value, default_method: &str) -> (String, i64, String) {
    let _ = default_method;
    let uri = h::as_str(v).unwrap_or("");
    let rest = uri
        .strip_prefix("http://")
        .or_else(|| uri.strip_prefix("https://"))
        .unwrap_or(uri);
    let (authority, path) = match rest.find('/') {
        Some(i) => (&rest[..i], &rest[i..]),
        None => (rest, "/"),
    };
    let (host, port) = match authority.rsplit_once(':') {
        Some((h, p)) if p.chars().all(|c| c.is_ascii_digit()) => {
            (h.to_string(), p.parse().unwrap_or(80))
        }
        _ => (authority.to_string(), 80),
    };
    (host, port, path.to_string())
}

/// Performs the HTTP exchange for a request; returns the response future.
fn close_request(request: Value) -> Value {
    let l = h::as_list(&request)
        .map(|l| l.borrow().clone())
        .unwrap_or_default();
    let id = l.first().and_then(h::as_int).unwrap_or(0);
    let method = l
        .get(1)
        .and_then(|v| h::as_str(v))
        .unwrap_or("GET")
        .to_string();
    let host = l
        .get(2)
        .and_then(|v| h::as_str(v))
        .unwrap_or("localhost")
        .to_string();
    let port = l.get(3).and_then(h::as_int).unwrap_or(80);
    let path = l
        .get(4)
        .and_then(|v| h::as_str(v))
        .unwrap_or("/")
        .to_string();
    let headers = headers_of(&request);
    let body = REQUEST_BUFS.lock().unwrap().remove(&id).unwrap_or_default();
    if let Err(e) = crate::permission::require_network(&host, port as u16) {
        let state = Rc::new(RefCell::new(async_state::FutureState::new()));
        async_state::complete(&state, Err(Value::str(e.to_string())));
        return wrap_future(state);
    }
    let state = Rc::new(RefCell::new(async_state::FutureState::new()));
    let ctrl = sl::make_controller_value(false, false);
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || http_exchange(&method, &host, port, &path, &headers, &body, tx));
    async_state::enqueue_task(async_state::Task::HttpClientPoll {
        state: state.clone(),
        rx,
        ctrl,
    });
    wrap_future(state)
}

fn http_exchange(
    method: &str,
    host: &str,
    port: i64,
    path: &str,
    headers: &[(String, String)],
    body: &[u8],
    tx: Sender<HttpData>,
) {
    let result = (|| -> Result<(), String> {
        let mut stream = TcpStream::connect((host, port as u16)).map_err(|e| e.to_string())?;
        let method_up = method.to_ascii_uppercase();
        let mut req = format!("{method_up} {path} HTTP/1.1\r\nHost: {host}:{port}\r\n");
        for (k, v) in headers {
            req.push_str(&format!("{k}: {v}\r\n"));
        }
        req.push_str(&format!("Content-Length: {}\r\n\r\n", body.len()));
        stream
            .write_all(req.as_bytes())
            .map_err(|e| e.to_string())?;
        stream.write_all(body).map_err(|e| e.to_string())?;
        let mut resp = Vec::new();
        stream.read_to_end(&mut resp).map_err(|e| e.to_string())?;
        let head_end = find_head_end(&resp);
        let (status, reason, headers) = match head_end {
            Some(end) => {
                let head = String::from_utf8_lossy(&resp[..end]);
                let mut lines = head.lines();
                let status_line = lines.next().unwrap_or("");
                let mut parts = status_line.split_whitespace();
                let _ver = parts.next();
                let status = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0);
                let reason = parts.collect::<Vec<_>>().join(" ");
                let headers: Vec<(String, String)> = lines
                    .filter_map(|l| l.split_once(':'))
                    .map(|(k, v)| (k.trim().to_string(), v.trim().to_string()))
                    .collect();
                (status, reason, headers)
            }
            None => (0, String::new(), Vec::new()),
        };
        let body = match head_end {
            Some(end) => resp[end..].to_vec(),
            None => resp,
        };
        let _ = tx.send(HttpData::Response {
            status,
            reason,
            headers,
            body,
        });
        Ok(())
    })();
    if let Err(e) = result {
        let _ = tx.send(HttpData::Error(e));
    }
}

fn http_client_request_definition() -> BridgedClassDef {
    let mut bc = BridgedClassDef::new("HttpClientRequest");
    bc.native_type = Some("d4rt::io::HttpClientRequest".to_string());
    bc.methods.insert(
        "write".to_string(),
        h::method(|_v, recv, args, _n| {
            let s = args.first().map(h::display).unwrap_or_default();
            request_append(&recv, s.as_bytes());
            Ok(Value::Null)
        }),
    );
    bc.methods.insert(
        "writeln".to_string(),
        h::method(|_v, recv, args, _n| {
            let s = args.first().map(h::display).unwrap_or_default();
            request_append(&recv, format!("{s}\n").as_bytes());
            Ok(Value::Null)
        }),
    );
    bc.methods.insert(
        "writeAll".to_string(),
        h::method(|_v, recv, args, _n| {
            let sep = args
                .get(1)
                .map(|s| h::as_str(s).unwrap_or("").to_string())
                .unwrap_or_default();
            let items: Vec<String> = args
                .first()
                .and_then(h::as_list)
                .map(|l| l.borrow().iter().map(h::display).collect())
                .unwrap_or_default();
            request_append(&recv, items.join(&sep).as_bytes());
            Ok(Value::Null)
        }),
    );
    bc.methods.insert(
        "add".to_string(),
        h::method(|_v, recv, args, _n| {
            if let Some(l) = args.first() {
                request_append(&recv, &ints_bytes(l));
            }
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
                name: Some("<HttpClientRequest.addStream data>".to_string()),
                min_arity: 1,
                max_arity: Some(1),
                is_async: false,
                is_generator: false,
                closure: Box::new(move |_visitor, args, _n| {
                    if let Some(l) = args.first() {
                        request_append(&recv_t, &ints_bytes(l));
                    }
                    Ok(Value::Null)
                }),
                callable_runtime_type: crate::rt::FunctionRuntimeType::untyped(),
            }));
            let on_done = Value::NativeFn(Rc::new(crate::callable::NativeFunction {
                name: Some("<HttpClientRequest.addStream done>".to_string()),
                min_arity: 0,
                max_arity: Some(0),
                is_async: false,
                is_generator: false,
                closure: Box::new(move |_visitor, _args, _n| {
                    async_state::complete(&out_done, Ok(Value::Null));
                    Ok(Value::Null)
                }),
                callable_runtime_type: crate::rt::FunctionRuntimeType::untyped(),
            }));
            sl::stream_subscribe_value(visitor, &stream, on_data, Value::Null, on_done);
            Ok(wrap_future(out))
        }),
    );
    bc.methods.insert(
        "flush".to_string(),
        h::method(|_v, _r, _a, _n| Ok(future_ok(Value::Null))),
    );
    bc.methods.insert(
        "close".to_string(),
        h::method(|_v, recv, _a, _n| Ok(close_request(recv))),
    );
    bc.methods.insert(
        "addError".to_string(),
        h::method(|_v, _r, _a, _n| Ok(Value::Null)),
    );
    bc.methods
        .insert("setOption".to_string(), socket_set_option_method(7));
    bc.methods
        .insert("getRawOption".to_string(), socket_get_raw_option_method(7));
    bc.methods
        .insert("setRawOption".to_string(), socket_set_raw_option_method(7));
    bc.methods.insert(
        "transform".to_string(),
        h::method(|visitor, recv, args, _n| {
            let stream = list_get(&recv, 2).unwrap_or(Value::Null);
            sl::stream_transform_value(visitor, &stream, &args)
        }),
    );
    bc.methods.insert(
        "addStream".to_string(),
        h::method(|_v, recv, args, _n| {
            let src = args.first().cloned().unwrap_or(Value::Null);
            let done = Rc::new(RefCell::new(async_state::FutureState::new()));
            let done_c = done.clone();
            let recv_c = recv.clone();
            let on_data = Value::NativeFn(Rc::new(NativeFunction {
                name: Some("<socket addStream>".to_string()),
                min_arity: 1,
                max_arity: Some(1),
                is_async: false,
                is_generator: false,
                closure: Box::new(move |_visitor, args, _named| {
                    if let Some(l) = args.first() {
                        socket_send(&recv_c, &ints_bytes(l));
                    }
                    Ok(Value::Null)
                }),
                callable_runtime_type: crate::rt::FunctionRuntimeType::untyped(),
            }));
            let on_done = Value::NativeFn(Rc::new(NativeFunction {
                name: Some("<socket addStream done>".to_string()),
                min_arity: 0,
                max_arity: Some(0),
                is_async: false,
                is_generator: false,
                closure: Box::new(move |_visitor, _args, _named| {
                    async_state::complete(&done_c, Ok(Value::Null));
                    Ok(Value::Null)
                }),
                callable_runtime_type: crate::rt::FunctionRuntimeType::untyped(),
            }));
            sl::stream_subscribe_value(_v, &src, on_data, Value::Null, on_done);
            Ok(wrap_future(done))
        }),
    );
    bc.methods.insert(
        "addError".to_string(),
        h::method(|_v, _r, _a, _n| Ok(Value::Null)),
    );
    bc.getters.insert(
        "method".to_string(),
        h::getter(|_v, recv| Ok(list_get(&recv, 1).unwrap_or(Value::Null))),
    );
    bc.getters.insert(
        "headers".to_string(),
        h::getter(|_v, recv| Ok(list_get(&recv, 5).unwrap_or(Value::Null))),
    );
    bc.getters.insert(
        "uri".to_string(),
        h::getter(|_v, recv| {
            let l = h::as_list(&recv)
                .map(|l| l.borrow().clone())
                .unwrap_or_default();
            let host = l.get(2).and_then(|v| h::as_str(v)).unwrap_or("");
            let port = l.get(3).and_then(h::as_int).unwrap_or(80);
            let path = l.get(4).and_then(|v| h::as_str(v)).unwrap_or("/");
            let uri = if port == 80 {
                format!("http://{host}{path}")
            } else {
                format!("http://{host}:{port}{path}")
            };
            Ok(wrap("Uri", Value::str(uri)))
        }),
    );
    bc.getters.insert(
        "contentLength".to_string(),
        h::getter(|_v, recv| {
            let id = list_get(&recv, 0).and_then(|i| h::as_int(&i)).unwrap_or(0);
            let len = REQUEST_BUFS
                .lock()
                .unwrap()
                .get(&id)
                .map(|b| b.len() as i64)
                .unwrap_or(0);
            Ok(Value::Int(len))
        }),
    );
    bc.getters.insert(
        "encoding".to_string(),
        h::getter(|_v, _r| Ok(wrap("Utf8Codec", Value::Null))),
    );
    bc.getters.insert(
        "done".to_string(),
        h::getter(|_v, recv| Ok(list_get(&recv, 6).unwrap_or_else(|| future_ok(Value::Null)))),
    );
    for name in [
        "persistentConnection",
        "followRedirects",
        "maxRedirects",
        "bufferOutput",
    ] {
        bc.setters
            .insert(name.to_string(), h::setter(|_v, _r, _val| Ok(())));
    }
    bc.setters.insert(
        "contentLength".to_string(),
        h::setter(|_v, _r, _val| Ok(())),
    );
    bc.setters
        .insert("encoding".to_string(), h::setter(|_v, _r, _val| Ok(())));
    bc
}

fn request_append(recv: &Value, bytes: &[u8]) {
    if let Some(id) = list_get(recv, 0).and_then(|i| h::as_int(&i))
        && let Some(buf) = REQUEST_BUFS.lock().unwrap().get_mut(&id)
    {
        buf.extend_from_slice(bytes);
    }
}

fn http_client_response_definition() -> BridgedClassDef {
    let mut bc = BridgedClassDef::new("HttpClientResponse");
    bc.native_type = Some("d4rt::io::HttpClientResponse".to_string());
    bc.getters.insert(
        "statusCode".to_string(),
        h::getter(|_v, recv| Ok(list_get(&recv, 1).unwrap_or(Value::Int(0)))),
    );
    bc.getters.insert(
        "reasonPhrase".to_string(),
        h::getter(|_v, recv| Ok(list_get(&recv, 2).unwrap_or(Value::Null))),
    );
    bc.getters.insert(
        "headers".to_string(),
        h::getter(|_v, recv| Ok(list_get(&recv, 3).unwrap_or(Value::Null))),
    );
    bc.getters.insert(
        "contentLength".to_string(),
        h::getter(|_v, recv| {
            let headers = list_get(&recv, 3).unwrap_or(Value::Null);
            let v = http_headers_value(&headers, "content-length");
            Ok(Value::Int(v.and_then(|s| s.parse().ok()).unwrap_or(-1)))
        }),
    );
    bc.getters.insert(
        "isRedirect".to_string(),
        h::getter(|_v, recv| {
            let code = list_get(&recv, 1).and_then(|v| h::as_int(&v)).unwrap_or(0);
            Ok(Value::Bool((300..400).contains(&code)))
        }),
    );
    bc.getters.insert(
        "persistentConnection".to_string(),
        h::getter(|_v, _r| Ok(Value::Bool(true))),
    );
    bc.getters
        .insert("cookies".to_string(), h::getter(|_v, _r| Ok(Value::list())));
    delegate_stream(&mut bc, 0);
    bc
}

// ---------- http server ----------

fn http_server_definition() -> BridgedClassDef {
    let mut bc = BridgedClassDef::new("HttpServer");
    bc.native_type = Some("d4rt::io::HttpServer".to_string());
    bc.static_methods.insert(
        "bind".to_string(),
        h::static_method(|_v, args, _n| {
            let host = args
                .first()
                .map(|v| h::as_str(v).unwrap_or("0.0.0.0").to_string())
                .unwrap_or_default();
            let port = args.get(1).and_then(h::as_int).unwrap_or(0);
            if let Err(e) = crate::permission::require_network(&host, port as u16) {
                return Ok(future_err(Value::str(e.to_string())));
            }
            match TcpListener::bind((host.as_str(), port as u16)) {
                Ok(listener) => {
                    let (id, ctrl, stream, done, local_host, local_port, data_rx) =
                        start_server(listener, host, false, crate::state::current().id);
                    async_state::enqueue_task(async_state::Task::ServerPoll {
                        id,
                        rx: data_rx,
                        ctrl: ctrl.clone(),
                    });
                    Ok(future_ok(wrap(
                        "HttpServer",
                        Value::List(Rc::new(RefCell::new(vec![
                            Value::Int(id),
                            ctrl,
                            stream,
                            done,
                            Value::Int(local_port),
                            Value::str(local_host),
                        ]))),
                    )))
                }
                Err(e) => Ok(future_err(Value::str(e.to_string()))),
            }
        }),
    );
    bc.methods.insert(
        "listen".to_string(),
        h::method(|visitor, recv, args, named| {
            let stream = list_get(&recv, 2).unwrap_or(Value::Null);
            sl::stream_listen_value(visitor, &stream, &args, &named)
        }),
    );
    bc.methods.insert(
        "close".to_string(),
        h::method(|visitor, recv, _a, _n| {
            close_server(&recv, visitor);
            Ok(future_ok(Value::Null))
        }),
    );
    bc.getters.insert(
        "port".to_string(),
        h::getter(|_v, recv| Ok(list_get(&recv, 4).unwrap_or(Value::Int(0)))),
    );
    bc.getters.insert(
        "address".to_string(),
        h::getter(|_v, recv| {
            Ok(list_get(&recv, 5)
                .map(|a| wrap("InternetAddress", a))
                .unwrap_or(Value::Null))
        }),
    );
    bc.getters.insert(
        "autoCompress".to_string(),
        h::getter(|_v, _r| Ok(Value::Bool(false))),
    );
    bc.setters
        .insert("autoCompress".to_string(), h::setter(|_v, _r, _val| Ok(())));
    bc
}

fn http_request_definition() -> BridgedClassDef {
    let mut bc = BridgedClassDef::new("HttpRequest");
    bc.native_type = Some("d4rt::io::HttpRequest".to_string());
    bc.getters.insert(
        "method".to_string(),
        h::getter(|_v, recv| Ok(list_get(&recv, 1).unwrap_or(Value::Null))),
    );
    bc.getters.insert(
        "uri".to_string(),
        h::getter(|_v, recv| Ok(list_get(&recv, 2).unwrap_or(Value::Null))),
    );
    bc.getters.insert(
        "headers".to_string(),
        h::getter(|_v, recv| Ok(list_get(&recv, 3).unwrap_or(Value::Null))),
    );
    bc.getters.insert(
        "response".to_string(),
        h::getter(|_v, recv| Ok(list_get(&recv, 4).unwrap_or(Value::Null))),
    );
    bc.getters.insert(
        "contentLength".to_string(),
        h::getter(|_v, recv| {
            let headers = list_get(&recv, 3).unwrap_or(Value::Null);
            let v = http_headers_value(&headers, "content-length");
            Ok(Value::Int(v.and_then(|s| s.parse().ok()).unwrap_or(-1)))
        }),
    );
    delegate_stream(&mut bc, 0);
    bc
}

fn http_response_definition() -> BridgedClassDef {
    let mut bc = BridgedClassDef::new("HttpResponse");
    bc.native_type = Some("d4rt::io::HttpResponse".to_string());
    bc.constructors.insert(
        "".to_string(),
        h::ctor(|_v, _args, _n| {
            Err(crate::error::InterpError::Runtime(
                "HttpResponse can't be constructed directly; use request.response".into(),
            ))
        }),
    );
    bc.methods.insert(
        "write".to_string(),
        h::method(|_v, recv, args, _n| {
            let s = args.first().map(h::display).unwrap_or_default();
            response_append(&recv, s.as_bytes());
            Ok(Value::Null)
        }),
    );
    bc.methods.insert(
        "writeln".to_string(),
        h::method(|_v, recv, args, _n| {
            let s = args.first().map(h::display).unwrap_or_default();
            response_append(&recv, format!("{s}\n").as_bytes());
            Ok(Value::Null)
        }),
    );
    bc.methods.insert(
        "writeAll".to_string(),
        h::method(|_v, recv, args, _n| {
            let sep = args
                .get(1)
                .map(|s| h::as_str(s).unwrap_or("").to_string())
                .unwrap_or_default();
            let items: Vec<String> = args
                .first()
                .and_then(h::as_list)
                .map(|l| l.borrow().iter().map(h::display).collect())
                .unwrap_or_default();
            response_append(&recv, items.join(&sep).as_bytes());
            Ok(Value::Null)
        }),
    );
    bc.methods.insert(
        "add".to_string(),
        h::method(|_v, recv, args, _n| {
            if let Some(l) = args.first() {
                response_append(&recv, &ints_bytes(l));
            }
            Ok(Value::Null)
        }),
    );
    bc.methods.insert(
        "flush".to_string(),
        h::method(|_v, _r, _a, _n| Ok(future_ok(Value::Null))),
    );
    bc.methods.insert(
        "close".to_string(),
        h::method(|_v, recv, _a, _n| {
            let l = h::as_list(&recv)
                .map(|l| l.borrow().clone())
                .unwrap_or_default();
            let conn_id = l.first().and_then(h::as_int).unwrap_or(0);
            let status = l.get(1).and_then(h::as_int).unwrap_or(200);
            let headers = list_get(&recv, 2).unwrap_or(Value::Null);
            let body = RESPONSE_BUFS
                .lock()
                .unwrap()
                .remove(&conn_id)
                .unwrap_or_default();
            let mut out = format!("HTTP/1.1 {status} {}\r\n", reason_phrase(status));
            let hdrs = http_headers_all(&headers);
            for (k, v) in hdrs {
                out.push_str(&format!("{k}: {v}\r\n"));
            }
            out.push_str(&format!("Content-Length: {}\r\n\r\n", body.len()));
            let mut bytes = out.into_bytes();
            bytes.extend_from_slice(&body);
            if let Some(tx) = SERVER_CONNS.lock().unwrap().get(&conn_id) {
                let _ = tx.send(ConnCtrl::Write(bytes));
            }
            if let Some(done) = l.get(3).cloned() {
                if let Value::Future(st) = done.clone() {
                    async_state::complete(&st, Ok(Value::Null));
                }
                Ok(done)
            } else {
                Ok(future_ok(Value::Null))
            }
        }),
    );
    bc.getters.insert(
        "statusCode".to_string(),
        h::getter(|_v, recv| Ok(list_get(&recv, 1).unwrap_or(Value::Int(200)))),
    );
    bc.getters.insert(
        "headers".to_string(),
        h::getter(|_v, recv| Ok(list_get(&recv, 2).unwrap_or(Value::Null))),
    );
    bc.getters.insert(
        "contentLength".to_string(),
        h::getter(|_v, recv| {
            let conn_id = list_get(&recv, 0).and_then(|v| h::as_int(&v)).unwrap_or(0);
            let n = RESPONSE_BUFS
                .lock()
                .unwrap()
                .get(&conn_id)
                .map(|b| b.len() as i64)
                .unwrap_or(-1);
            Ok(Value::Int(n))
        }),
    );
    bc.setters.insert(
        "statusCode".to_string(),
        h::setter(|_v, recv, value| {
            if let Some(l) = h::as_list(&recv) {
                let status = h::as_int(&value).unwrap_or(200);
                l.borrow_mut()[1] = Value::Int(status);
            }
            Ok(())
        }),
    );
    bc
}

fn response_append(recv: &Value, bytes: &[u8]) {
    if let Some(conn_id) = list_get(recv, 0).and_then(|v| h::as_int(&v))
        && let Some(buf) = RESPONSE_BUFS.lock().unwrap().get_mut(&conn_id)
    {
        buf.extend_from_slice(bytes);
    }
}

fn reason_phrase(status: i64) -> &'static str {
    match status {
        200 => "OK",
        201 => "Created",
        204 => "No Content",
        301 => "Moved Permanently",
        302 => "Found",
        304 => "Not Modified",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        409 => "Conflict",
        418 => "I'm a Teapot",
        500 => "Internal Server Error",
        501 => "Not Implemented",
        503 => "Service Unavailable",
        _ => "OK",
    }
}

// ---------- headers / content type / cookie ----------

fn http_headers_definition() -> BridgedClassDef {
    let mut bc = BridgedClassDef::new("HttpHeaders");
    bc.native_type = Some("d4rt::io::HttpHeaders".to_string());
    for name in [
        "acceptHeader",
        "acceptCharsetHeader",
        "acceptEncodingHeader",
        "acceptLanguageHeader",
        "authorizationHeader",
        "cacheControlHeader",
        "connectionHeader",
        "contentEncodingHeader",
        "contentLengthHeader",
        "contentTypeHeader",
        "cookieHeader",
        "dateHeader",
        "hostHeader",
        "ifModifiedSinceHeader",
        "locationHeader",
        "setCookieHeader",
        "userAgentHeader",
    ] {
        let value = match name {
            "acceptHeader" => "accept",
            "acceptCharsetHeader" => "accept-charset",
            "acceptEncodingHeader" => "accept-encoding",
            "acceptLanguageHeader" => "accept-language",
            "authorizationHeader" => "authorization",
            "cacheControlHeader" => "cache-control",
            "connectionHeader" => "connection",
            "contentEncodingHeader" => "content-encoding",
            "contentLengthHeader" => "content-length",
            "contentTypeHeader" => "content-type",
            "cookieHeader" => "cookie",
            "dateHeader" => "date",
            "hostHeader" => "host",
            "ifModifiedSinceHeader" => "if-modified-since",
            "locationHeader" => "location",
            "setCookieHeader" => "set-cookie",
            "userAgentHeader" => "user-agent",
            _ => "",
        };
        bc.static_getters.insert(
            name.to_string(),
            h::static_getter(move |_v| Ok(Value::str(value))),
        );
    }
    bc.methods.insert(
        "add".to_string(),
        h::method(|_v, recv, args, _n| {
            let name = args
                .first()
                .and_then(h::as_str)
                .unwrap_or("")
                .to_ascii_lowercase();
            let value = args.get(1).map(h::display).unwrap_or_default();
            headers_mut(&recv, |entries| {
                let key = Value::str(&name);
                if let Some((_, list)) = entries.iter_mut().find(|(k, _)| k == &key) {
                    if let Some(l) = h::as_list(list) {
                        l.borrow_mut().push(Value::str(value));
                    }
                } else {
                    entries.push((
                        key,
                        Value::List(Rc::new(RefCell::new(vec![Value::str(value)]))),
                    ));
                }
            });
            Ok(Value::Null)
        }),
    );
    bc.methods.insert(
        "set".to_string(),
        h::method(|_v, recv, args, _n| {
            let name = args
                .first()
                .and_then(h::as_str)
                .unwrap_or("")
                .to_ascii_lowercase();
            let value = args.get(1).map(h::display).unwrap_or_default();
            headers_mut(&recv, |entries| {
                let key = Value::str(&name);
                entries.retain(|(k, _)| k != &key);
                entries.push((
                    key,
                    Value::List(Rc::new(RefCell::new(vec![Value::str(value)]))),
                ));
            });
            Ok(Value::Null)
        }),
    );
    bc.methods.insert(
        "remove".to_string(),
        h::method(|_v, recv, args, _n| {
            let name = args
                .first()
                .and_then(h::as_str)
                .unwrap_or("")
                .to_ascii_lowercase();
            let value = args.get(1).map(h::display).unwrap_or_default();
            headers_mut(&recv, |entries| {
                let key = Value::str(&name);
                if let Some((_, list)) = entries.iter_mut().find(|(k, _)| k == &key)
                    && let Some(l) = h::as_list(list)
                {
                    let mut c = l.borrow_mut();
                    if let Some(i) = c.iter().position(|v| h::display(v) == value) {
                        c.remove(i);
                    }
                }
            });
            Ok(Value::Null)
        }),
    );
    bc.methods.insert(
        "removeAll".to_string(),
        h::method(|_v, recv, args, _n| {
            let name = args
                .first()
                .and_then(h::as_str)
                .unwrap_or("")
                .to_ascii_lowercase();
            headers_mut(&recv, |entries| {
                entries.retain(|(k, _)| h::as_str(k) != Some(name.as_str()));
            });
            Ok(Value::Null)
        }),
    );
    bc.methods.insert(
        "value".to_string(),
        h::method(|_v, recv, args, _n| {
            let name = args
                .first()
                .and_then(h::as_str)
                .unwrap_or("")
                .to_ascii_lowercase();
            Ok(http_headers_value(&recv, &name)
                .map(Value::str)
                .unwrap_or(Value::Null))
        }),
    );
    bc.methods.insert(
        "forEach".to_string(),
        h::method(|visitor, recv, args, _n| {
            let cb = args.first().cloned().unwrap_or(Value::Null);
            if let Some(m) = h::as_map(&recv) {
                let entries = m.borrow().entries.clone();
                for (k, v) in entries {
                    visitor.call_value(&cb, vec![k, v], HashMap::new())?;
                }
            }
            Ok(Value::Null)
        }),
    );
    bc.methods.insert(
        "clear".to_string(),
        h::method(|_v, recv, _a, _n| {
            headers_mut(&recv, |entries| entries.clear());
            Ok(Value::Null)
        }),
    );
    bc.methods.insert(
        "toString".to_string(),
        h::method(|_v, recv, _a, _n| {
            let mut parts = Vec::new();
            if let Some(m) = h::as_map(&recv) {
                for (k, v) in m.borrow().entries.clone() {
                    if let Some(l) = h::as_list(&v) {
                        for item in l.borrow().clone() {
                            parts.push(format!("{}: {}", h::display(&k), h::display(&item)));
                        }
                    }
                }
            }
            Ok(Value::str(parts.join("\n")))
        }),
    );
    bc.getters.insert(
        "contentLength".to_string(),
        h::getter(|_v, recv| {
            Ok(http_headers_value(&recv, "content-length")
                .and_then(|s| s.parse().ok())
                .map(Value::Int)
                .unwrap_or(Value::Int(-1)))
        }),
    );
    bc.getters.insert(
        "contentType".to_string(),
        h::getter(|_v, recv| {
            Ok(http_headers_value(&recv, "content-type")
                .map(|s| {
                    let parts: Vec<&str> = s.split(';').map(|p| p.trim()).collect();
                    let (p, sub) = parts
                        .first()
                        .and_then(|m| m.split_once('/'))
                        .unwrap_or(("application", "octet-stream"));
                    wrap(
                        "ContentType",
                        Value::List(Rc::new(RefCell::new(vec![
                            Value::str(p),
                            Value::str(sub),
                            Value::Null,
                        ]))),
                    )
                })
                .unwrap_or(Value::Null))
        }),
    );
    bc.setters.insert(
        "contentLength".to_string(),
        h::setter(|_v, recv, value| {
            let v = h::as_int(&value).unwrap_or(-1).to_string();
            let name = "content-length".to_string();
            headers_mut(&recv, |entries| {
                let key = Value::str(&name);
                entries.retain(|(k, _)| k != &key);
                entries.push((key, Value::List(Rc::new(RefCell::new(vec![Value::str(v)])))));
            });
            Ok(())
        }),
    );
    bc
}

fn http_headers_value(v: &Value, name: &str) -> Option<String> {
    h::as_map(v).and_then(|m| {
        let entries = m.borrow().entries.clone();
        entries
            .iter()
            .find(|(k, _)| {
                h::as_str(k)
                    .map(|s| s.eq_ignore_ascii_case(name))
                    .unwrap_or(false)
            })
            .and_then(|(_, vals)| h::as_list(vals))
            .and_then(|l| l.borrow().first().cloned())
            .map(|v| h::display(&v))
    })
}

fn http_headers_all(v: &Value) -> Vec<(String, String)> {
    let mut out = Vec::new();
    if let Some(m) = h::as_map(v) {
        for (k, vals) in m.borrow().entries.clone() {
            if let Some(l) = h::as_list(&vals) {
                for item in l.borrow().clone() {
                    out.push((h::display(&k), h::display(&item)));
                }
            }
        }
    }
    out
}

fn headers_mut(v: &Value, f: impl FnOnce(&mut Vec<(Value, Value)>)) {
    if let Some(m) = h::as_map(v) {
        f(&mut m.borrow_mut().entries);
    }
}

fn content_type_definition() -> BridgedClassDef {
    let mut bc = BridgedClassDef::new("ContentType");
    bc.native_type = Some("d4rt::io::ContentType".to_string());
    bc.constructors.insert(
        "".to_string(),
        h::ctor(|_v, args, named| {
            let primary = args
                .first()
                .and_then(h::as_str)
                .unwrap_or("application")
                .to_string();
            let sub = args
                .get(1)
                .and_then(h::as_str)
                .unwrap_or("octet-stream")
                .to_string();
            let charset = named.get("charset").and_then(h::as_str).map(Value::str);
            Ok(wrap(
                "ContentType",
                Value::List(Rc::new(RefCell::new(vec![
                    Value::str(primary),
                    Value::str(sub),
                    charset.unwrap_or(Value::Null),
                ]))),
            ))
        }),
    );
    bc.static_methods.insert(
        "parse".to_string(),
        h::static_method(|_v, args, _n| {
            let s = args.first().and_then(h::as_str).unwrap_or("");
            let charset = s
                .split(';')
                .skip(1)
                .find_map(|p| p.trim().strip_prefix("charset="))
                .map(|c| Value::str(c.trim()));
            let (p, sub) = s
                .split(';')
                .next()
                .and_then(|m| m.trim().split_once('/'))
                .unwrap_or(("application", "octet-stream"));
            Ok(wrap(
                "ContentType",
                Value::List(Rc::new(RefCell::new(vec![
                    Value::str(p),
                    Value::str(sub),
                    charset.unwrap_or(Value::Null),
                ]))),
            ))
        }),
    );
    for (name, value) in [
        ("text", "text/plain"),
        ("html", "text/html"),
        ("json", "application/json"),
        ("binary", "application/octet-stream"),
    ] {
        bc.static_getters.insert(
            name.to_string(),
            h::static_getter(move |_v| {
                let (p, sub) = value
                    .split_once('/')
                    .unwrap_or(("application", "octet-stream"));
                Ok(wrap(
                    "ContentType",
                    Value::List(Rc::new(RefCell::new(vec![
                        Value::str(p),
                        Value::str(sub),
                        Value::Null,
                    ]))),
                ))
            }),
        );
    }
    bc.getters.insert(
        "primaryType".to_string(),
        h::getter(|_v, recv| Ok(list_get(&recv, 0).unwrap_or(Value::Null))),
    );
    bc.getters.insert(
        "subType".to_string(),
        h::getter(|_v, recv| Ok(list_get(&recv, 1).unwrap_or(Value::Null))),
    );
    bc.getters.insert(
        "charset".to_string(),
        h::getter(|_v, recv| Ok(list_get(&recv, 2).unwrap_or(Value::Null))),
    );
    bc.getters.insert(
        "mimeType".to_string(),
        h::getter(|_v, recv| {
            let l = h::as_list(&recv)
                .map(|l| l.borrow().clone())
                .unwrap_or_default();
            let p = l.first().map(h::display).unwrap_or_default();
            let s = l.get(1).map(h::display).unwrap_or_default();
            Ok(Value::str(format!("{p}/{s}")))
        }),
    );
    bc.getters.insert(
        "value".to_string(),
        h::getter(|_v, recv| {
            let l = h::as_list(&recv)
                .map(|l| l.borrow().clone())
                .unwrap_or_default();
            let p = l.first().map(h::display).unwrap_or_default();
            let s = l.get(1).map(h::display).unwrap_or_default();
            let c = l.get(2).and_then(|v| h::as_str(v));
            Ok(Value::str(match c {
                Some(c) => format!("{p}/{s}; charset={c}"),
                None => format!("{p}/{s}"),
            }))
        }),
    );
    bc.getters.insert(
        "parameters".to_string(),
        h::getter(|_v, _r| {
            Ok(Value::Map(Rc::new(RefCell::new(DartMap {
                entries: Vec::new(),
            }))))
        }),
    );
    bc.methods.insert(
        "toString".to_string(),
        h::method(|_v, recv, _a, _n| {
            let l = h::as_list(&recv)
                .map(|l| l.borrow().clone())
                .unwrap_or_default();
            let p = l.first().map(h::display).unwrap_or_default();
            let s = l.get(1).map(h::display).unwrap_or_default();
            let c = l.get(2).and_then(|v| h::as_str(v));
            Ok(Value::str(match c {
                Some(c) => format!("{p}/{s}; charset={c}"),
                None => format!("{p}/{s}"),
            }))
        }),
    );
    bc
}

fn cookie_definition() -> BridgedClassDef {
    let mut bc = BridgedClassDef::new("Cookie");
    bc.native_type = Some("d4rt::io::Cookie".to_string());
    bc.constructors.insert(
        "".to_string(),
        h::ctor(|_v, args, _n| {
            let name = args.first().and_then(h::as_str).unwrap_or("").to_string();
            let value = args.get(1).and_then(h::as_str).unwrap_or("").to_string();
            Ok(wrap(
                "Cookie",
                Value::List(Rc::new(RefCell::new(vec![
                    Value::str(name),
                    Value::str(value),
                    Value::Int(-1),
                    Value::str(""),
                    Value::Bool(false),
                    Value::Bool(false),
                ]))),
            ))
        }),
    );
    for (i, name) in ["name", "value", "maxAge", "path", "secure", "httpOnly"]
        .iter()
        .enumerate()
    {
        let idx = i;
        bc.getters.insert(
            name.to_string(),
            h::getter(move |_v, recv| Ok(list_get(&recv, idx).unwrap_or(Value::Null))),
        );
        bc.setters.insert(
            name.to_string(),
            h::setter(move |_v, recv, value| {
                if let Some(l) = h::as_list(&recv) {
                    l.borrow_mut()[idx] = value;
                }
                Ok(())
            }),
        );
    }
    bc.methods.insert(
        "toString".to_string(),
        h::method(|_v, recv, _a, _n| {
            let l = h::as_list(&recv)
                .map(|l| l.borrow().clone())
                .unwrap_or_default();
            let name = l.first().map(h::display).unwrap_or_default();
            let value = l.get(1).map(h::display).unwrap_or_default();
            Ok(Value::str(format!("{name}={value}")))
        }),
    );
    bc
}

// ---------- stream delegation ----------

/// Adds the common Stream API to a class whose native holds a stream at
/// index 0 (sockets, server sockets, requests, responses).
fn delegate_stream(bc: &mut BridgedClassDef, stream_index: usize) {
    bc.methods.insert(
        "listen".to_string(),
        h::method(move |visitor, recv, args, named| {
            let stream = list_get(&recv, stream_index).unwrap_or(Value::Null);
            sl::stream_listen_value(visitor, &stream, &args, &named)
        }),
    );
    bc.methods.insert(
        "transform".to_string(),
        h::method(move |visitor, recv, args, _n| {
            let stream = list_get(&recv, stream_index).unwrap_or(Value::Null);
            sl::stream_transform_value(visitor, &stream, &args)
        }),
    );
    for (name, kind) in [
        ("toList", sl::FUTURE_OP_COLLECT),
        ("toSet", sl::FUTURE_OP_TOSET),
        ("join", sl::FUTURE_OP_JOIN),
        ("drain", sl::FUTURE_OP_DRAIN),
        ("firstWhere", sl::FUTURE_OP_FIRSTWHERE),
        ("lastWhere", sl::FUTURE_OP_LASTWHERE),
        ("singleWhere", sl::FUTURE_OP_SINGLEWHERE),
        ("elementAt", sl::FUTURE_OP_ELEMENTAT),
        ("any", sl::FUTURE_OP_ANY),
        ("every", sl::FUTURE_OP_EVERY),
        ("contains", sl::FUTURE_OP_CONTAINS),
        ("fold", sl::FUTURE_OP_FOLD),
        ("reduce", sl::FUTURE_OP_REDUCE),
        ("forEach", sl::FUTURE_OP_FOREACH),
    ] {
        bc.methods.insert(
            name.to_string(),
            h::method(move |_v, recv, args, named| {
                let or_else = if kind == sl::FUTURE_OP_FIRSTWHERE
                    || kind == sl::FUTURE_OP_LASTWHERE
                    || kind == sl::FUTURE_OP_SINGLEWHERE
                {
                    named.get("orElse").cloned().unwrap_or(Value::Null)
                } else {
                    Value::Null
                };
                let stream = list_get(&recv, stream_index).unwrap_or(Value::Null);
                sl::stream_future_op(&stream, kind, &args, or_else)
            }),
        );
    }
    for (name, kind) in [
        ("map", sl::OP_MAP),
        ("where", sl::OP_WHERE),
        ("expand", sl::OP_EXPAND),
        ("take", sl::OP_TAKE),
        ("skip", sl::OP_SKIP),
        ("takeWhile", sl::OP_TAKE_WHILE),
        ("skipWhile", sl::OP_SKIP_WHILE),
        ("distinct", sl::OP_DISTINCT),
        ("asyncMap", sl::OP_ASYNC_MAP),
        ("asyncExpand", sl::OP_ASYNC_EXPAND),
    ] {
        bc.methods.insert(
            name.to_string(),
            h::method(move |_v, recv, args, _n| {
                let stream = list_get(&recv, stream_index).unwrap_or(Value::Null);
                sl::stream_unary_op_value(&stream, &args, kind)
            }),
        );
    }
    bc.getters.insert(
        "first".to_string(),
        h::getter(move |_v, recv| {
            let stream = list_get(&recv, stream_index).unwrap_or(Value::Null);
            sl::stream_future_op_getter(&stream, sl::FUTURE_OP_FIRST)
        }),
    );
    bc.getters.insert(
        "isEmpty".to_string(),
        h::getter(move |_v, recv| {
            let stream = list_get(&recv, stream_index).unwrap_or(Value::Null);
            sl::stream_future_op_getter(&stream, sl::FUTURE_OP_ISEMPTY)
        }),
    );
    bc.getters.insert(
        "length".to_string(),
        h::getter(move |_v, recv| {
            let stream = list_get(&recv, stream_index).unwrap_or(Value::Null);
            sl::stream_future_op_getter(&stream, sl::FUTURE_OP_LENGTH)
        }),
    );
    bc.getters.insert(
        "isBroadcast".to_string(),
        h::getter(|_v, _r| Ok(Value::Bool(false))),
    );
}

// ---------- poll tasks (called from async_state) ----------

#[allow(clippy::too_many_arguments)]
pub fn wrap_socket_value(
    id: i64,
    host: &str,
    port: i64,
    rhost: &str,
    rport: i64,
    ctrl: &Value,
    stream: &Value,
    done: &Rc<RefCell<async_state::FutureState>>,
    class: &str,
) -> Value {
    wrap(
        class,
        Value::List(Rc::new(RefCell::new(vec![
            Value::Int(id),
            ctrl.clone(),
            stream.clone(),
            wrap_future(done.clone()),
            Value::str(host),
            Value::Int(port),
            Value::str(rhost),
            Value::Int(rport),
            Value::List(Rc::new(RefCell::new(Vec::new()))), // 8 raw options
            Value::List(Rc::new(RefCell::new(Vec::new()))), // 9 multicast memberships
        ]))),
    )
}

/// Stores a `RawSocketOption` (level/option/value) on a socket-like value,
/// replacing any existing entry with the same (level, option) pair.
fn socket_set_raw_option(
    recv: &Value,
    options_index: usize,
    level: i64,
    option: i64,
    value: Value,
) {
    if let Some(options) = list_get(recv, options_index).and_then(|o| h::as_list(&o)) {
        let mut opts = options.borrow_mut();
        let mut replaced = false;
        for cell in opts.iter_mut() {
            let same = list_get(cell, 0).and_then(|l| h::as_int(&l)) == Some(level)
                && list_get(cell, 1).and_then(|o| h::as_int(&o)) == Some(option);
            if same {
                *cell = value.clone();
                replaced = true;
                break;
            }
        }
        if !replaced {
            opts.push(value);
        }
    }
}

fn socket_get_raw_option(recv: &Value, options_index: usize, level: i64, option: i64) -> Value {
    if let Some(options) = list_get(recv, options_index).and_then(|o| h::as_list(&o)) {
        for cell in options.borrow().iter() {
            let same = list_get(cell, 0).and_then(|l| h::as_int(&l)) == Some(level)
                && list_get(cell, 1).and_then(|o| h::as_int(&o)) == Some(option);
            if same {
                return cell.clone();
            }
        }
    }
    default_raw_socket_option()
}

/// `setOption(SocketOption, value)` — stores the option on the socket so
/// `getRawOption` can read it back (level `Socket`).
fn socket_set_option_method(options_index: usize) -> h::M {
    h::method(move |_v, recv, args, _n| {
        let name = args
            .first()
            .and_then(|a| h::as_str(a))
            .unwrap_or("")
            .to_string();
        let value = args.get(1).cloned().unwrap_or(Value::Null);
        let option = match name.as_str() {
            "tcpNoDelay" => 1,
            _ => 0,
        };
        let bytes: Vec<u8> = match value {
            Value::Bool(b) => vec![b as u8],
            Value::Int(i) => (i as u32).to_le_bytes().to_vec(),
            _ => Vec::new(),
        };
        socket_set_raw_option(
            &recv,
            options_index,
            1,
            option,
            raw_socket_option_native(1, option, &bytes),
        );
        Ok(Value::Null)
    })
}

/// `setRawOption(RawSocketOption)` — stores the given option.
fn socket_set_raw_option_method(options_index: usize) -> h::M {
    h::method(move |_v, recv, args, _n| {
        if let Some(opt) = args.first() {
            let level = list_get(opt, 0).and_then(|x| h::as_int(&x)).unwrap_or(0);
            let option = list_get(opt, 1).and_then(|x| h::as_int(&x)).unwrap_or(0);
            socket_set_raw_option(&recv, options_index, level, option, opt.clone());
        }
        Ok(Value::Null)
    })
}

/// `getRawOption(level, option)` — returns the stored option or a default.
fn socket_get_raw_option_method(options_index: usize) -> h::M {
    h::method(move |_v, recv, args, _n| {
        let level = args.first().and_then(h::as_int).unwrap_or(0);
        let option = args.get(1).and_then(h::as_int).unwrap_or(0);
        Ok(socket_get_raw_option(&recv, options_index, level, option))
    })
}

fn socket_toggle_multicast(recv: &Value, group: &str, join: bool) {
    if let Some(members) = list_get(recv, 9).and_then(|o| h::as_list(&o)) {
        let mut m = members.borrow_mut();
        if join {
            if !m.iter().any(|g| h::as_str(g) == Some(group)) {
                m.push(Value::str(group));
            }
        } else {
            m.retain(|g| h::as_str(g) != Some(group));
        }
    }
}

/// Drains one socket data channel. Returns whether to keep polling.
pub fn poll_socket(
    visitor: &mut InterpreterVisitor,
    id: i64,
    rx: &Receiver<SocketData>,
    ctrl: &Value,
    done: &Rc<RefCell<async_state::FutureState>>,
) -> bool {
    loop {
        match rx.try_recv() {
            Ok(SocketData::Read(b)) => {
                RAW_BUFS
                    .lock()
                    .unwrap()
                    .entry(id)
                    .or_default()
                    .extend_from_slice(&b);
                sl::controller_add_value(visitor, ctrl, bytes_value(&b));
            }
            Ok(SocketData::Eof) => {
                let _ = sl::controller_close_value(visitor, ctrl);
                async_state::complete(done, Ok(Value::Null));
                SOCKETS.lock().unwrap().remove(&id);
                return false;
            }
            Ok(SocketData::Error(e)) => {
                sl::controller_add_error_value(visitor, ctrl, Value::str(&e));
                let _ = sl::controller_close_value(visitor, ctrl);
                async_state::complete(done, Ok(Value::Null));
                SOCKETS.lock().unwrap().remove(&id);
                return false;
            }
            Err(TryRecvError::Empty) => {
                let has =
                    SOCKETS.lock().unwrap().contains_key(&id) || crate::stdlib::io::is_pipe_id(id);
                return has;
            }
            Err(TryRecvError::Disconnected) => {
                SOCKETS.lock().unwrap().remove(&id);
                return false;
            }
        }
    }
}

/// Drains one server accept channel, delivering HttpRequest values.
pub fn poll_server(
    visitor: &mut InterpreterVisitor,
    id: i64,
    rx: &Receiver<ServerData>,
    ctrl: &Value,
) -> bool {
    let id_server = id;
    loop {
        match rx.try_recv() {
            Ok(ServerData::Accepted {
                conn_id,
                method,
                path,
                headers,
                body,
                peer,
            }) => {
                let rctrl = sl::make_controller_value(false, false);
                let rstream = sl::controller_stream_value(&rctrl);
                let uri = wrap("Uri", Value::str(format!("http://{peer}{path}")));
                RESPONSE_BUFS.lock().unwrap().insert(conn_id, Vec::new());
                claim(
                    crate::state::current().id,
                    conn_id,
                    HttpResourceKind::ResponseBuf,
                );
                let done = Rc::new(RefCell::new(async_state::FutureState::new()));
                let response = wrap(
                    "HttpResponse",
                    Value::List(Rc::new(RefCell::new(vec![
                        Value::Int(conn_id),
                        Value::Int(200),
                        headers_value(Vec::new()),
                        wrap_future(done),
                    ]))),
                );
                let request = wrap(
                    "HttpRequest",
                    Value::List(Rc::new(RefCell::new(vec![
                        rstream,
                        Value::str(method),
                        uri,
                        headers_value(headers),
                        response,
                    ]))),
                );
                if !body.is_empty() {
                    sl::controller_add_value(visitor, &rctrl, bytes_value(&body));
                }
                let _ = sl::controller_close_value(visitor, &rctrl);
                sl::controller_add_value(visitor, ctrl, request);
            }
            Ok(ServerData::SocketAccepted { id, peer, data_rx }) => {
                let sctrl = sl::make_controller_value(false, false);
                let sstream = sl::controller_stream_value(&sctrl);
                let sdone = Rc::new(RefCell::new(async_state::FutureState::new()));
                let class = if RAW_SERVERS.lock().unwrap().contains(&id_server) {
                    "RawSocket"
                } else {
                    "Socket"
                };
                let value =
                    wrap_socket_value(id, &peer, 0, &peer, 0, &sctrl, &sstream, &sdone, class);
                async_state::enqueue_task(async_state::Task::SocketPoll {
                    id,
                    rx: data_rx,
                    ctrl: sctrl.clone(),
                    done: sdone,
                });
                sl::controller_add_value(visitor, ctrl, value);
            }
            Err(TryRecvError::Empty) => {
                return SERVER_CLOSE.lock().unwrap().contains_key(&id);
            }
            Err(TryRecvError::Disconnected) => {
                SERVER_CLOSE.lock().unwrap().remove(&id);
                return false;
            }
        }
    }
}

/// Drains one HttpClient exchange channel, completing the response future.
pub fn poll_http_client(
    visitor: &mut InterpreterVisitor,
    state: &Rc<RefCell<async_state::FutureState>>,
    rx: &Receiver<HttpData>,
    ctrl: &Value,
) -> bool {
    match rx.try_recv() {
        Ok(HttpData::Response {
            status,
            reason,
            headers,
            body,
        }) => {
            let hdr = headers_value(headers);
            let rctrl = sl::make_controller_value(false, false);
            let rstream = sl::controller_stream_value(&rctrl);
            let value = wrap(
                "HttpClientResponse",
                Value::List(Rc::new(RefCell::new(vec![
                    rstream,
                    Value::Int(status),
                    Value::str(reason),
                    hdr,
                ]))),
            );
            if !body.is_empty() {
                sl::controller_add_value(visitor, &rctrl, bytes_value(&body));
            }
            let _ = sl::controller_close_value(visitor, &rctrl);
            async_state::complete(state, Ok(value));
            false
        }
        Ok(HttpData::Error(e)) => {
            let _ = sl::controller_close_value(visitor, ctrl);
            async_state::complete(state, Err(Value::str(e)));
            false
        }
        Err(TryRecvError::Empty) => true,
        Err(TryRecvError::Disconnected) => {
            async_state::complete(state, Err(Value::str("connection closed")));
            false
        }
    }
}

pub fn load(env: &Rc<RefCell<Environment>>) {
    let defs = [
        socket_definition(),
        server_socket_definition(),
        internet_address_definition(),
        internet_address_type_definition(),
        socket_exception_definition(),
        socket_option_definition(),
        socket_direction_definition(),
        raw_socket_event_definition(),
        raw_socket_option_definition(),
        connection_task_definition(),
        raw_socket_definition(),
        raw_server_socket_definition(),
        datagram_definition(),
        raw_datagram_socket_definition(),
        network_interface_definition(),
        http_client_definition(),
        http_client_request_definition(),
        http_client_response_definition(),
        http_server_definition(),
        http_request_definition(),
        http_response_definition(),
        http_headers_definition(),
        content_type_definition(),
        cookie_definition(),
        header_value_definition(),
    ];
    for bc in defs {
        let bc = Arc::new(bc);
        h::register_class(bc.clone());
        Environment::define_bridge(env, bc);
    }
}

// ---------------------------------------------------------------------------
// HeaderValue
// ---------------------------------------------------------------------------

fn header_value_native(value: String, params: Vec<(String, Option<String>)>) -> Value {
    let entries: Vec<(Value, Value)> = params
        .into_iter()
        .map(|(k, v)| (Value::str(k), v.map(Value::str).unwrap_or(Value::Null)))
        .collect();
    wrap(
        "HeaderValue",
        Value::List(Rc::new(RefCell::new(vec![
            Value::str(value),
            Value::Map(Rc::new(RefCell::new(crate::value::DartMap { entries }))),
        ]))),
    )
}

fn header_value_unquote(s: &str, preserve_backslash: bool) -> String {
    let mut out = s.trim().to_string();
    if out.len() >= 2 && out.starts_with('"') && out.ends_with('"') {
        out = out[1..out.len() - 1].to_string();
    }
    if preserve_backslash {
        return out;
    }
    let mut res = String::with_capacity(out.len());
    let mut it = out.chars().peekable();
    while let Some(c) = it.next() {
        if c == '\\' {
            if let Some(&n) = it.peek() {
                res.push(n);
                it.next();
            }
        } else {
            res.push(c);
        }
    }
    res
}

/// Splits `text` on `sep`, ignoring separators inside double quotes.
fn split_header_parts(text: &str, sep: char) -> Vec<String> {
    let mut parts = Vec::new();
    let mut cur = String::new();
    let mut in_quote = false;
    for ch in text.chars() {
        if ch == '"' {
            in_quote = !in_quote;
            cur.push(ch);
        } else if ch == sep && !in_quote {
            parts.push(std::mem::take(&mut cur));
        } else {
            cur.push(ch);
        }
    }
    parts.push(cur);
    parts
}

fn header_value_parse(
    raw: &str,
    parameter_separator: char,
    value_separator: Option<char>,
    preserve_backslash: bool,
) -> (String, Vec<(String, Option<String>)>) {
    let (value, param_text): (String, String) = match value_separator {
        Some(vs) => match raw.find(vs) {
            Some(i) => (raw[..i].to_string(), raw[i + vs.len_utf8()..].to_string()),
            None => (raw.to_string(), String::new()),
        },
        None => {
            let parts = split_header_parts(raw, parameter_separator);
            let mut it = parts.into_iter();
            let value = it.next().unwrap_or_default();
            let mut rest = String::new();
            for p in it {
                rest.push(parameter_separator);
                rest.push_str(&p);
            }
            (value, rest)
        }
    };
    let mut params = Vec::new();
    for part in split_header_parts(&param_text, parameter_separator) {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        let (name, val) = match part.find('=') {
            Some(i) => {
                let n = part[..i].trim().to_string();
                let v = header_value_unquote(&part[i + 1..], preserve_backslash);
                (n, Some(v))
            }
            None => (part.to_string(), None),
        };
        if !name.is_empty() {
            params.push((name, val));
        }
    }
    (value, params)
}

fn header_value_definition() -> BridgedClassDef {
    let mut bc = BridgedClassDef::new("HeaderValue");
    bc.native_type = Some("d4rt::io::HeaderValue".to_string());
    bc.constructors.insert(
        "".to_string(),
        h::ctor(|_v, args, _n| {
            let value = args.first().map(h::display).unwrap_or_default();
            let params = match args.get(1).and_then(h::as_map) {
                Some(m) => m
                    .borrow()
                    .entries
                    .iter()
                    .map(|(k, v)| (h::display(k), h::as_str(v).map(|s| s.to_string())))
                    .collect(),
                None => Vec::new(),
            };
            Ok(header_value_native(value, params))
        }),
    );
    bc.static_methods.insert(
        "parse".to_string(),
        h::static_method(|_v, args, named| {
            h::require_args(&args, 1, "parse")?;
            let raw = h::as_str(&args[0])
                .ok_or_else(|| h::err("HeaderValue.parse requires a String argument."))?;
            let sep = named
                .get("parameterSeparator")
                .and_then(h::as_str)
                .and_then(|s| s.chars().next())
                .unwrap_or(';');
            let vsep = named
                .get("valueSeparator")
                .and_then(h::as_str)
                .and_then(|s| s.chars().next());
            let preserve = named
                .get("preserveBackslash")
                .and_then(h::as_bool)
                .unwrap_or(false);
            let (value, params) = header_value_parse(raw, sep, vsep, preserve);
            Ok(header_value_native(value, params))
        }),
    );
    bc.getters.insert(
        "value".to_string(),
        h::getter(|_v, recv| Ok(list_get(&recv, 0).unwrap_or(Value::Null))),
    );
    bc.getters.insert(
        "parameters".to_string(),
        h::getter(|_v, recv| Ok(list_get(&recv, 1).unwrap_or(Value::Null))),
    );
    bc.methods.insert(
        "toString".to_string(),
        h::method(|_v, recv, _a, _n| {
            let mut out = list_get(&recv, 0)
                .map(|v| h::display(&v))
                .unwrap_or_default();
            if let Some(params) = list_get(&recv, 1).and_then(|v| h::as_map(&v)) {
                for (k, v) in params.borrow().entries.iter() {
                    let key = h::display(k);
                    match h::as_str(v) {
                        Some(val) => out.push_str(&format!("; {key}={val}")),
                        None => out.push_str(&format!(";{key}")),
                    }
                }
            }
            Ok(Value::str(out))
        }),
    );
    bc
}
