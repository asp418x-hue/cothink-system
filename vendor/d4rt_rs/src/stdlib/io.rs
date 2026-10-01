//! dart:io — File, Directory, FileSystemEntity, FileMode, FileStat,
//! FileSystemEntityType, FileSystemException (and Path*Exception
//! subclasses), FileLock, FileSystemEvent, StringSink, Platform, stdio
//! (stdin/stdout/stderr, Stdin, Stdout, StdioType).
//!
//! Native representations:
//! - File / Directory / FileSystemEntity: `Value::Str(path)`.
//! - FileMode / FileSystemEntityType: `Value::Str(name)`.
//! - FileStat: `[Str(type), Int(mode), Int(size), Int(mtime_us),
//!   Int(atime_us), Int(ctime_us)]`.
//! - FileSystemException: `[Str(message), Str(path), os_error]`.
//! - Platform is a static-only class.
//! - Stdin: `Value::Str("stdin")`.
//! - Stdout/Stderr: `[Str(tag), Future(done_state)]`.
//! - StdioType: `Value::Str(name)`.

use super::helpers as h;
use crate::async_state;
use crate::callable::NativeFunction;
use crate::class::BridgedClassDef;
use crate::env::Environment;
use crate::error::InterpError;
use crate::interp::InterpreterVisitor;
use crate::rt::FunctionRuntimeType;
use crate::value::{DartMap, Value};
use std::cell::RefCell;
use std::collections::HashMap;
use std::fs;
use std::io::{IsTerminal, Read, Write};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::sync::LazyLock;
use std::sync::Mutex;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::mpsc;
use std::time::{SystemTime, UNIX_EPOCH};

/// A pipe's write side: (bytes channel, close channel).
type PipeWrites = (mpsc::Sender<Vec<u8>>, mpsc::Sender<()>);

fn define_bridge(env: &Rc<RefCell<Environment>>, bc: BridgedClassDef) {
    let bc = Arc::new(bc);
    h::register_class(bc.clone());
    Environment::define_bridge(env, bc);
}

pub fn load(env: &Rc<RefCell<Environment>>) {
    define_bridge(env, file_system_entity_definition());
    define_bridge(env, file_mode_definition());
    define_bridge(env, file_lock_definition());
    define_bridge(env, file_stat_definition());
    define_bridge(env, file_system_entity_type_definition());
    define_bridge(env, file_system_exception_definition());
    define_bridge(
        env,
        path_exception_definition("PathAccessException", "Cannot open file"),
    );
    define_bridge(
        env,
        path_exception_definition("PathExistsException", "File exists"),
    );
    define_bridge(
        env,
        path_exception_definition("PathNotFoundException", "Cannot open file"),
    );
    define_bridge(env, file_system_event_definition());
    define_bridge(env, file_definition());
    define_bridge(env, random_access_file_definition());
    define_bridge(env, directory_definition());
    define_bridge(env, platform_definition());
    define_bridge(env, stdin_definition());
    define_bridge(env, stdout_definition());
    define_bridge(env, stdio_type_definition());
    define_bridge(env, pipe_definition());
    Environment::define(env, "stdin", wrap("Stdin", Value::str("stdin")));
    Environment::define(
        env,
        "stdout",
        wrap(
            "Stdout",
            Value::List(Rc::new(RefCell::new(vec![
                Value::str("stdout"),
                wrap_future(Rc::new(RefCell::new(async_state::FutureState::new()))),
            ]))),
        ),
    );
    Environment::define(
        env,
        "stderr",
        wrap(
            "Stdout",
            Value::List(Rc::new(RefCell::new(vec![
                Value::str("stderr"),
                wrap_future(Rc::new(RefCell::new(async_state::FutureState::new()))),
            ]))),
        ),
    );
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

fn path_of(v: &Value) -> String {
    h::as_str(v).unwrap_or("").to_string()
}

fn as_str_opt(v: &Value) -> Option<String> {
    h::as_str(v).map(|s| s.to_string())
}

fn fs_exception(message: impl Into<String>, path: &str) -> Value {
    wrap(
        "FileSystemException",
        Value::List(Rc::new(RefCell::new(vec![
            Value::str(message.into()),
            Value::str(path),
            Value::Null,
        ]))),
    )
}

fn throw_fs(message: impl Into<String>, path: &str) -> InterpError {
    InterpError::Throw(fs_exception(message, path))
}

fn io_err_to_exception(e: &std::io::Error, path: &str) -> Value {
    fs_exception(e.to_string(), path)
}

fn absolute_path(p: &str) -> String {
    let path = PathBuf::from(p);
    if path.is_absolute() {
        return path.to_string_lossy().to_string();
    }
    match std::env::current_dir() {
        Ok(cwd) => cwd.join(path).to_string_lossy().to_string(),
        Err(_) => path.to_string_lossy().to_string(),
    }
}

fn now_us() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_micros() as i64)
        .unwrap_or(0)
}

fn system_time_us(t: Option<SystemTime>) -> i64 {
    match t {
        Some(t) => t
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_micros() as i64)
            .unwrap_or(0),
        None => 0,
    }
}

fn mode_name(mode: &Value) -> String {
    as_str_opt(mode).unwrap_or_else(|| "w".to_string())
}

/// Converts a permission error into an `io::Error` so `?` works in
/// `std::io::Result`-returning helpers.
fn perm_io(e: crate::error::InterpError) -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::PermissionDenied, e.to_string())
}

/// Writes `bytes` to `path` honoring the FileMode native (`r/w/a/w+/a+`).
fn write_with_mode(path: &str, bytes: &[u8], mode: &str) -> std::io::Result<()> {
    crate::permission::require_fs_write(path).map_err(perm_io)?;
    let append = mode.starts_with('a');
    if append {
        use std::io::Write;
        let mut f = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)?;
        f.write_all(bytes)
    } else {
        fs::write(path, bytes)
    }
}

fn ensure_parent(path: &str) -> std::io::Result<()> {
    if let Some(parent) = Path::new(path).parent()
        && !parent.as_os_str().is_empty()
        && !parent.exists()
    {
        fs::create_dir_all(parent)?;
    }
    Ok(())
}

// ---------- FileSystemEntity (base class) ----------

fn file_system_entity_definition() -> BridgedClassDef {
    let mut bc = BridgedClassDef::new("FileSystemEntity");
    bc.getters.insert(
        "path".to_string(),
        h::getter(|_v, recv| Ok(Value::str(path_of(&recv)))),
    );
    bc.methods.insert(
        "existsSync".to_string(),
        h::method(|_v, recv, _args, _n| Ok(Value::Bool(Path::new(&path_of(&recv)).exists()))),
    );
    bc.methods.insert(
        "exists".to_string(),
        h::method(|_v, recv, _args, _n| {
            Ok(future_ok(Value::Bool(Path::new(&path_of(&recv)).exists())))
        }),
    );
    bc.getters.insert(
        "uri".to_string(),
        h::getter(|_v, recv| Ok(uri_of(&path_of(&recv)))),
    );
    bc.getters.insert(
        "isAbsolute".to_string(),
        h::getter(|_v, recv| Ok(Value::Bool(Path::new(&path_of(&recv)).is_absolute()))),
    );
    bc.getters.insert(
        "parent".to_string(),
        h::getter(|_v, recv| {
            let path = path_of(&recv);
            let parent = Path::new(&path)
                .parent()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_default();
            Ok(wrap("Directory", Value::str(parent)))
        }),
    );
    bc.methods.insert(
        "identicalSync".to_string(),
        h::method(|_v, recv, args, _n| {
            let other = args.first().map(path_of).unwrap_or_default();
            Ok(Value::Bool(fs_identical(&path_of(&recv), &other)?))
        }),
    );
    bc.methods.insert(
        "identical".to_string(),
        h::method(|_v, recv, args, _n| {
            let other = args.first().map(path_of).unwrap_or_default();
            Ok(future_ok(Value::Bool(fs_identical(
                &path_of(&recv),
                &other,
            )?)))
        }),
    );
    bc.methods.insert(
        "resolveSymbolicLinksSync".to_string(),
        h::method(|_v, recv, _args, _n| {
            let path = path_of(&recv);
            resolve_links(&path)
                .map(Value::str)
                .map_err(|e| throw_fs(e.to_string(), &path))
        }),
    );
    bc.methods.insert(
        "resolveSymbolicLinks".to_string(),
        h::method(|_v, recv, _args, _n| {
            let path = path_of(&recv);
            match resolve_links(&path) {
                Ok(s) => Ok(future_ok(Value::str(s))),
                Err(e) => Ok(future_err(io_err_to_exception(&e, &path))),
            }
        }),
    );
    bc.methods.insert(
        "watch".to_string(),
        h::method(|visitor, recv, _args, named| watch_impl(visitor, &path_of(&recv), &named)),
    );
    bc.static_methods.insert(
        "identicalSync".to_string(),
        h::static_method(|_v, args, _n| {
            let a = args.first().map(path_of).unwrap_or_default();
            let b = args.get(1).map(path_of).unwrap_or_default();
            Ok(Value::Bool(fs_identical(&a, &b)?))
        }),
    );
    bc.static_methods.insert(
        "identical".to_string(),
        h::static_method(|_v, args, _n| {
            let a = args.first().map(path_of).unwrap_or_default();
            let b = args.get(1).map(path_of).unwrap_or_default();
            Ok(future_ok(Value::Bool(fs_identical(&a, &b)?)))
        }),
    );
    bc.static_methods.insert(
        "parentOf".to_string(),
        h::static_method(|_v, args, _n| {
            let path = args
                .first()
                .map(|v| h::as_str(v).unwrap_or(""))
                .unwrap_or_default();
            let parent = Path::new(path)
                .parent()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_default();
            Ok(wrap("Directory", Value::str(parent)))
        }),
    );
    bc.static_getters.insert(
        "isWatchSupported".to_string(),
        h::static_getter(|_v| Ok(Value::Bool(true))),
    );
    bc.methods.insert(
        "statSync".to_string(),
        h::method(|_v, recv, _args, _n| make_stat(&path_of(&recv))),
    );
    bc.methods.insert(
        "stat".to_string(),
        h::method(|_v, recv, _args, _n| Ok(future_ok(make_stat(&path_of(&recv))?))),
    );
    bc.methods.insert(
        "deleteSync".to_string(),
        h::method(|_v, recv, _args, named| {
            let path = path_of(&recv);
            let recursive = named.get("recursive").and_then(h::as_bool).unwrap_or(false);
            crate::permission::require_fs_write(&path)?;
            if Path::new(&path).is_dir() {
                if recursive {
                    fs::remove_dir_all(&path).map_err(|e| throw_fs(e.to_string(), &path))?
                } else {
                    fs::remove_dir(&path).map_err(|e| throw_fs(e.to_string(), &path))?
                }
            } else {
                match fs::remove_file(&path) {
                    Ok(_) => {}
                    Err(e)
                        if e.kind() == std::io::ErrorKind::NotFound
                            && !Path::new(&path).exists() => {}
                    Err(e) => return Err(throw_fs(e.to_string(), &path)),
                }
            }
            Ok(Value::Null)
        }),
    );
    bc.methods.insert(
        "delete".to_string(),
        h::method(|_v, recv, _args, named| {
            let path = path_of(&recv);
            let recursive = named.get("recursive").and_then(h::as_bool).unwrap_or(false);
            crate::permission::require_fs_write(&path)?;
            let result = if Path::new(&path).is_dir() {
                if recursive {
                    fs::remove_dir_all(&path)
                } else {
                    fs::remove_dir(&path)
                }
            } else {
                fs::remove_file(&path)
            };
            match result {
                Ok(_) => Ok(future_ok(Value::Null)),
                Err(e)
                    if e.kind() == std::io::ErrorKind::NotFound && !Path::new(&path).exists() =>
                {
                    Ok(future_ok(Value::Null))
                }
                Err(e) => Ok(future_err(io_err_to_exception(&e, &path))),
            }
        }),
    );
    bc.methods.insert(
        "renameSync".to_string(),
        h::method(|_v, recv, args, _n| {
            let src = path_of(&recv);
            let dst = args.first().map(path_of).unwrap_or_default();
            fs::rename(&src, &dst).map_err(|e| throw_fs(e.to_string(), &src))?;
            Ok(wrap("FileSystemEntity", Value::str(dst)))
        }),
    );
    bc.methods.insert(
        "rename".to_string(),
        h::method(|_v, recv, args, _n| {
            let src = path_of(&recv);
            let dst = args.first().map(path_of).unwrap_or_default();
            match fs::rename(&src, &dst) {
                Ok(_) => Ok(future_ok(wrap("FileSystemEntity", Value::str(dst)))),
                Err(e) => Ok(future_err(io_err_to_exception(&e, &src))),
            }
        }),
    );
    bc.methods.insert(
        "absolute".to_string(),
        h::method(|_v, recv, _args, _n| {
            Ok(wrap(
                "FileSystemEntity",
                Value::str(absolute_path(&path_of(&recv))),
            ))
        }),
    );
    bc.static_methods.insert(
        "type".to_string(),
        h::static_method(|_v, args, _n| {
            let path = args.first().map(path_of).unwrap_or_default();
            Ok(future_ok(type_of_path(&path)))
        }),
    );
    bc.static_methods.insert(
        "typeSync".to_string(),
        h::static_method(|_v, args, _n| {
            let path = args.first().map(path_of).unwrap_or_default();
            Ok(type_of_path(&path))
        }),
    );
    bc.static_methods.insert(
        "isFile".to_string(),
        h::static_method(|_v, args, _n| {
            let path = args.first().map(path_of).unwrap_or_default();
            Ok(future_ok(Value::Bool(Path::new(&path).is_file())))
        }),
    );
    bc.static_methods.insert(
        "isFileSync".to_string(),
        h::static_method(|_v, args, _n| {
            let path = args.first().map(path_of).unwrap_or_default();
            Ok(Value::Bool(Path::new(&path).is_file()))
        }),
    );
    bc.static_methods.insert(
        "isDirectory".to_string(),
        h::static_method(|_v, args, _n| {
            let path = args.first().map(path_of).unwrap_or_default();
            Ok(future_ok(Value::Bool(Path::new(&path).is_dir())))
        }),
    );
    bc.static_methods.insert(
        "isDirectorySync".to_string(),
        h::static_method(|_v, args, _n| {
            let path = args.first().map(path_of).unwrap_or_default();
            Ok(Value::Bool(Path::new(&path).is_dir()))
        }),
    );
    bc.static_methods.insert(
        "isLink".to_string(),
        h::static_method(|_v, args, _n| {
            let path = args.first().map(path_of).unwrap_or_default();
            Ok(future_ok(Value::Bool(
                fs::symlink_metadata(&path)
                    .map(|m| m.file_type().is_symlink())
                    .unwrap_or(false),
            )))
        }),
    );
    bc.static_methods.insert(
        "isLinkSync".to_string(),
        h::static_method(|_v, args, _n| {
            let path = args.first().map(path_of).unwrap_or_default();
            Ok(Value::Bool(
                fs::symlink_metadata(&path)
                    .map(|m| m.file_type().is_symlink())
                    .unwrap_or(false),
            ))
        }),
    );
    bc
}

fn type_of_path(path: &str) -> Value {
    let meta = fs::symlink_metadata(path);
    let name = match &meta {
        Ok(m) if m.file_type().is_symlink() => "link",
        Ok(m) if m.is_dir() => "directory",
        Ok(_) => "file",
        Err(_) => "notFound",
    };
    wrap("FileSystemEntityType", Value::str(name))
}

// ---------- FileMode ----------

fn file_mode_definition() -> BridgedClassDef {
    let mut bc = BridgedClassDef::new("FileMode");
    for (name, value) in [
        ("read", "r"),
        ("write", "w"),
        ("append", "a"),
        ("writeOnly", "w+"),
        ("writeOnlyAppend", "a+"),
    ] {
        bc.static_getters.insert(
            name.to_string(),
            h::static_getter(move |_v| Ok(wrap("FileMode", Value::str(value)))),
        );
    }
    bc.getters.insert(
        "name".to_string(),
        h::getter(|_v, recv| Ok(Value::str(mode_name(&recv)))),
    );
    bc.methods.insert(
        "toString".into(),
        h::method(|_v, t, _a, _n| Ok(Value::str(format!("FileMode: '{}'", mode_name(&t))))),
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

// ---------- FileSystemEntityType ----------

fn file_system_entity_type_definition() -> BridgedClassDef {
    let mut bc = BridgedClassDef::new("FileSystemEntityType");
    for name in [
        "file",
        "directory",
        "link",
        "notFound",
        "pipe",
        "unixDomainSock",
    ] {
        bc.static_getters.insert(
            name.to_string(),
            h::static_getter(move |_v| Ok(wrap("FileSystemEntityType", Value::str(name)))),
        );
    }
    bc.getters.insert(
        "name".to_string(),
        h::getter(|_v, recv| Ok(Value::str(as_str_opt(&recv).unwrap_or_default()))),
    );
    bc.methods.insert(
        "toString".into(),
        h::method(|_v, t, _a, _n| {
            Ok(Value::str(format!(
                "FileSystemEntityType({})",
                as_str_opt(&t).unwrap_or_default()
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

// ---------- FileStat ----------

fn file_stat_definition() -> BridgedClassDef {
    let mut bc = BridgedClassDef::new("FileStat");
    bc.static_methods.insert(
        "statSync".to_string(),
        h::static_method(|_v, args, _n| {
            let path = args.first().map(path_of).unwrap_or_default();
            make_stat(&path)
        }),
    );
    bc.static_methods.insert(
        "stat".to_string(),
        h::static_method(|_v, args, _n| {
            let path = args.first().map(path_of).unwrap_or_default();
            Ok(future_ok(make_stat(&path)?))
        }),
    );
    bc.getters.insert(
        "type".into(),
        h::getter(|_v, recv| {
            Ok(h::item_at(&recv, 0)
                .unwrap_or_else(|| wrap("FileSystemEntityType", Value::str("notFound"))))
        }),
    );
    bc.getters.insert(
        "mode".into(),
        h::getter(|_v, recv| Ok(h::item_at(&recv, 1).unwrap_or(Value::Int(0)))),
    );
    bc.getters.insert(
        "modeString".into(),
        h::getter(|_v, recv| {
            let mode = h::item_at(&recv, 1)
                .and_then(|x| h::as_int(&x))
                .unwrap_or(0);
            Ok(Value::str(mode_string_of(mode)))
        }),
    );
    bc.getters.insert(
        "size".into(),
        h::getter(|_v, recv| Ok(h::item_at(&recv, 2).unwrap_or(Value::Int(0)))),
    );
    bc.getters.insert(
        "modified".into(),
        h::getter(|_v, recv| {
            Ok(us_to_datetime(
                h::item_at(&recv, 3)
                    .and_then(|x| h::as_int(&x))
                    .unwrap_or(0),
            ))
        }),
    );
    bc.getters.insert(
        "accessed".into(),
        h::getter(|_v, recv| {
            Ok(us_to_datetime(
                h::item_at(&recv, 4)
                    .and_then(|x| h::as_int(&x))
                    .unwrap_or(0),
            ))
        }),
    );
    bc.getters.insert(
        "changed".into(),
        h::getter(|_v, recv| {
            Ok(us_to_datetime(
                h::item_at(&recv, 5)
                    .and_then(|x| h::as_int(&x))
                    .unwrap_or(0),
            ))
        }),
    );
    bc.methods.insert(
        "toString".into(),
        h::method(|_v, t, _a, _n| {
            Ok(Value::str(format!(
                "FileStat: size={} mode={}",
                h::display(&h::item_at(&t, 2).unwrap_or(Value::Int(0))),
                h::display(&h::item_at(&t, 1).unwrap_or(Value::Int(0)))
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

fn make_stat(path: &str) -> Result<Value, crate::error::InterpError> {
    crate::permission::require_fs_read(path)?;
    let (type_name, mode, size, mtime, atime, ctime) = match fs::symlink_metadata(path) {
        Ok(m) => {
            let type_name = if m.file_type().is_symlink() {
                "link"
            } else if m.is_dir() {
                "directory"
            } else {
                "file"
            };
            let size = if m.is_dir() { 0 } else { m.len() as i64 };
            let mode = metadata_mode(&m);
            (
                type_name,
                mode,
                size,
                system_time_us(m.modified().ok()),
                system_time_us(m.accessed().ok()),
                system_time_us(m.modified().ok()),
            )
        }
        Err(_) => ("notFound", 0, 0, 0, 0, 0),
    };
    Ok(wrap(
        "FileStat",
        Value::List(Rc::new(RefCell::new(vec![
            wrap("FileSystemEntityType", Value::str(type_name)),
            Value::Int(mode),
            Value::Int(size),
            Value::Int(mtime),
            Value::Int(atime),
            Value::Int(ctime),
        ]))),
    ))
}

#[cfg(unix)]
fn metadata_mode(m: &fs::Metadata) -> i64 {
    use std::os::unix::fs::PermissionsExt;
    m.permissions().mode() as i64
}

#[cfg(not(unix))]
fn metadata_mode(_m: &fs::Metadata) -> i64 {
    0
}

fn us_to_datetime(us: i64) -> Value {
    h::wrap(
        &h::class("DateTime"),
        Value::List(Rc::new(RefCell::new(vec![
            Value::Int(us),
            Value::Bool(false),
        ]))),
    )
}

// ---------- FileSystemException ----------

fn file_system_exception_definition() -> BridgedClassDef {
    let mut bc = BridgedClassDef::new("FileSystemException");
    bc.constructors.insert(
        "".to_string(),
        h::ctor(|_v, args, named| {
            let message = args
                .first()
                .map(|v| h::as_str(v).unwrap_or_default().to_string())
                .unwrap_or_default();
            let path = args
                .get(1)
                .map(|v| h::as_str(v).unwrap_or_default().to_string())
                .or_else(|| {
                    named
                        .get("path")
                        .and_then(|v| h::as_str(v).map(|s| s.to_string()))
                })
                .unwrap_or_default();
            let os_error = args.get(2).cloned().unwrap_or(Value::Null);
            Ok(wrap(
                "FileSystemException",
                Value::List(Rc::new(RefCell::new(vec![
                    Value::str(message),
                    Value::str(path),
                    os_error,
                ]))),
            ))
        }),
    );
    bc.getters.insert(
        "message".into(),
        h::getter(|_v, t| Ok(h::item_at(&t, 0).unwrap_or(Value::Null))),
    );
    bc.getters.insert(
        "path".into(),
        h::getter(|_v, t| Ok(h::item_at(&t, 1).unwrap_or(Value::Null))),
    );
    bc.getters.insert(
        "osError".into(),
        h::getter(|_v, t| Ok(h::item_at(&t, 2).unwrap_or(Value::Null))),
    );
    bc.methods.insert(
        "toString".into(),
        h::method(|_v, t, _a, _n| {
            let message = h::display(&h::item_at(&t, 0).unwrap_or(Value::Null));
            let path = h::display(&h::item_at(&t, 1).unwrap_or(Value::Null));
            Ok(Value::str(format!(
                "FileSystemException: {message}, path = '{path}'"
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

// ---------- File ----------

fn file_definition() -> BridgedClassDef {
    let mut bc = BridgedClassDef::new("File");
    bc.constructors.insert(
        "".to_string(),
        h::ctor(|_v, args, _n| {
            let path = args.first().map(path_of).unwrap_or_default();
            Ok(wrap("File", Value::str(path)))
        }),
    );
    bc.constructors.insert(
        "fromUri".to_string(),
        h::ctor(|_v, args, _n| {
            let path = args.first().map(path_from_uri).unwrap_or_default();
            Ok(wrap("File", Value::str(path)))
        }),
    );
    bc.constructors.insert(
        "fromRawPath".to_string(),
        h::ctor(|_v, args, _n| {
            let path = args.first().map(path_from_raw_bytes).unwrap_or_default();
            Ok(wrap("File", Value::str(path)))
        }),
    );
    bc.getters.insert(
        "path".to_string(),
        h::getter(|_v, recv| Ok(Value::str(path_of(&recv)))),
    );
    bc.getters.insert(
        "absolute".to_string(),
        h::getter(|_v, recv| Ok(wrap("File", Value::str(absolute_path(&path_of(&recv)))))),
    );
    bc.getters.insert(
        "isAbsolute".to_string(),
        h::getter(|_v, recv| Ok(Value::Bool(Path::new(&path_of(&recv)).is_absolute()))),
    );
    bc.getters.insert(
        "uri".to_string(),
        h::getter(|_v, recv| Ok(uri_of(&path_of(&recv)))),
    );
    bc.getters.insert(
        "parent".to_string(),
        h::getter(|_v, recv| {
            let path = path_of(&recv);
            let parent = Path::new(&path)
                .parent()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_default();
            Ok(wrap("Directory", Value::str(parent)))
        }),
    );

    // ---- open ----
    bc.methods.insert(
        "openSync".to_string(),
        h::method(|_v, recv, _args, named| {
            let path = path_of(&recv);
            let mode = named
                .get("mode")
                .map(mode_name)
                .unwrap_or_else(|| "r".to_string());
            open_raf(&path, &mode).map_err(|e| throw_fs(e.to_string(), &path))
        }),
    );
    bc.methods.insert(
        "open".to_string(),
        h::method(|_v, recv, _args, named| {
            let path = path_of(&recv);
            let mode = named
                .get("mode")
                .map(mode_name)
                .unwrap_or_else(|| "r".to_string());
            match open_raf(&path, &mode) {
                Ok(raf) => Ok(future_ok(raf)),
                Err(e) => Ok(future_err(io_err_to_exception(&e, &path))),
            }
        }),
    );

    // ---- sync methods ----
    bc.methods.insert(
        "existsSync".to_string(),
        h::method(|_v, recv, _args, _n| {
            let path = path_of(&recv);
            crate::permission::require_fs_read(&path)?;
            Ok(Value::Bool(Path::new(&path).exists()))
        }),
    );
    bc.methods.insert(
        "readAsStringSync".to_string(),
        h::method(|_v, recv, _args, _n| {
            let path = path_of(&recv);
            crate::permission::require_fs_read(&path)?;
            match fs::read_to_string(&path) {
                Ok(s) => Ok(Value::str(s)),
                Err(e) => Err(throw_fs(e.to_string(), &path)),
            }
        }),
    );
    bc.methods.insert(
        "readAsBytesSync".to_string(),
        h::method(|_v, recv, _args, _n| {
            let path = path_of(&recv);
            crate::permission::require_fs_read(&path)?;
            match fs::read(&path) {
                Ok(bytes) => Ok(bytes_value(bytes)),
                Err(e) => Err(throw_fs(e.to_string(), &path)),
            }
        }),
    );
    bc.methods.insert(
        "readAsLinesSync".to_string(),
        h::method(|_v, recv, _args, _n| {
            let path = path_of(&recv);
            crate::permission::require_fs_read(&path)?;
            match fs::read_to_string(&path) {
                Ok(content) => Ok(lines_value(&content)),
                Err(e) => Err(throw_fs(e.to_string(), &path)),
            }
        }),
    );
    bc.methods.insert(
        "writeAsStringSync".to_string(),
        h::method(|_v, recv, args, named| {
            let path = path_of(&recv);
            let content = args
                .first()
                .map(|v| h::as_str(v).unwrap_or("").to_string())
                .unwrap_or_default();
            let mode = named
                .get("mode")
                .map(mode_name)
                .unwrap_or_else(|| "w".to_string());
            write_with_mode(&path, content.as_bytes(), &mode)
                .map_err(|e| throw_fs(e.to_string(), &path))?;
            Ok(Value::Null)
        }),
    );
    bc.methods.insert(
        "writeAsBytesSync".to_string(),
        h::method(|_v, recv, args, named| {
            let path = path_of(&recv);
            let bytes = args
                .first()
                .cloned()
                .unwrap_or(Value::Null)
                .let_list()
                .map(|l| {
                    l.borrow()
                        .iter()
                        .filter_map(|v| h::as_int(v).map(|i| i as u8))
                        .collect::<Vec<u8>>()
                })
                .unwrap_or_default();
            let mode = named
                .get("mode")
                .map(mode_name)
                .unwrap_or_else(|| "w".to_string());
            write_with_mode(&path, &bytes, &mode).map_err(|e| throw_fs(e.to_string(), &path))?;
            Ok(Value::Null)
        }),
    );
    bc.methods.insert(
        "createSync".to_string(),
        h::method(|_v, recv, _args, named| {
            let path = path_of(&recv);
            let recursive = named.get("recursive").and_then(h::as_bool).unwrap_or(false);
            crate::permission::require_fs_write(&path)?;
            if recursive {
                ensure_parent(&path).map_err(|e| throw_fs(e.to_string(), &path))?;
            }
            fs::OpenOptions::new()
                .create(true)
                .write(true)
                .truncate(true)
                .open(&path)
                .map_err(|e| throw_fs(e.to_string(), &path))?;
            Ok(Value::Null)
        }),
    );
    bc.methods.insert(
        "deleteSync".to_string(),
        h::method(|_v, recv, _args, named| {
            let path = path_of(&recv);
            let recursive = named.get("recursive").and_then(h::as_bool).unwrap_or(false);
            crate::permission::require_fs_write(&path)?;
            if Path::new(&path).is_dir() {
                if recursive {
                    fs::remove_dir_all(&path).map_err(|e| throw_fs(e.to_string(), &path))?
                } else {
                    fs::remove_dir(&path).map_err(|e| throw_fs(e.to_string(), &path))?
                }
            } else {
                match fs::remove_file(&path) {
                    Ok(_) => {}
                    Err(e)
                        if e.kind() == std::io::ErrorKind::NotFound
                            && !Path::new(&path).exists() => {}
                    Err(e) => return Err(throw_fs(e.to_string(), &path)),
                }
            }
            Ok(Value::Null)
        }),
    );
    bc.methods.insert(
        "renameSync".to_string(),
        h::method(|_v, recv, args, _n| {
            let src = path_of(&recv);
            let dst = args.first().map(path_of).unwrap_or_default();
            crate::permission::require_fs_write(&src)?;
            crate::permission::require_fs_write(&dst)?;
            fs::rename(&src, &dst).map_err(|e| throw_fs(e.to_string(), &src))?;
            Ok(wrap("File", Value::str(dst)))
        }),
    );
    bc.methods.insert(
        "copySync".to_string(),
        h::method(|_v, recv, args, _n| {
            let src = path_of(&recv);
            let dst = args.first().map(path_of).unwrap_or_default();
            crate::permission::require_fs_write(&src)?;
            crate::permission::require_fs_write(&dst)?;
            fs::copy(&src, &dst).map_err(|e| throw_fs(e.to_string(), &src))?;
            Ok(wrap("File", Value::str(dst)))
        }),
    );
    bc.methods.insert(
        "lengthSync".to_string(),
        h::method(|_v, recv, _args, _n| {
            let path = path_of(&recv);
            crate::permission::require_fs_read(&path)?;
            match fs::metadata(&path) {
                Ok(m) => Ok(Value::Int(m.len() as i64)),
                Err(e) => Err(throw_fs(e.to_string(), &path)),
            }
        }),
    );
    bc.methods.insert(
        "lastModifiedSync".to_string(),
        h::method(|_v, recv, _args, _n| {
            let path = path_of(&recv);
            crate::permission::require_fs_read(&path)?;
            match fs::metadata(&path).and_then(|m| m.modified()) {
                Ok(t) => Ok(us_to_datetime(system_time_us(Some(t)))),
                Err(e) => Err(throw_fs(e.to_string(), &path)),
            }
        }),
    );
    bc.methods.insert(
        "lastAccessedSync".to_string(),
        h::method(|_v, recv, _args, _n| {
            let path = path_of(&recv);
            crate::permission::require_fs_read(&path)?;
            match fs::metadata(&path).and_then(|m| m.accessed()) {
                Ok(t) => Ok(us_to_datetime(system_time_us(Some(t)))),
                Err(e) => Err(throw_fs(e.to_string(), &path)),
            }
        }),
    );
    bc.methods.insert(
        "setLastModifiedSync".to_string(),
        h::method(|_v, recv, args, _n| {
            let path = path_of(&recv);
            let us = args.first().map(datetime_us).unwrap_or(0);
            set_modified(&path, us).map_err(|e| throw_fs(e.to_string(), &path))?;
            Ok(Value::Null)
        }),
    );
    bc.methods.insert(
        "setLastAccessedSync".to_string(),
        h::method(|_v, recv, args, _n| {
            let path = path_of(&recv);
            let us = args.first().map(datetime_us).unwrap_or(0);
            set_accessed(&path, us).map_err(|e| throw_fs(e.to_string(), &path))?;
            Ok(Value::Null)
        }),
    );
    bc.methods.insert(
        "statSync".to_string(),
        h::method(|_v, recv, _args, _n| make_stat(&path_of(&recv))),
    );

    // ---- async methods ----
    bc.methods.insert(
        "exists".to_string(),
        h::method(|_v, recv, _args, _n| {
            Ok(future_ok(Value::Bool(Path::new(&path_of(&recv)).exists())))
        }),
    );
    bc.methods.insert(
        "readAsString".to_string(),
        h::method(|_v, recv, _args, _n| {
            let path = path_of(&recv);
            crate::permission::require_fs_read(&path)?;
            match fs::read_to_string(&path) {
                Ok(s) => Ok(future_ok(Value::str(s))),
                Err(e) => Ok(future_err(io_err_to_exception(&e, &path))),
            }
        }),
    );
    bc.methods.insert(
        "readAsBytes".to_string(),
        h::method(|_v, recv, _args, _n| {
            let path = path_of(&recv);
            crate::permission::require_fs_read(&path)?;
            match fs::read(&path) {
                Ok(bytes) => Ok(future_ok(bytes_value(bytes))),
                Err(e) => Ok(future_err(io_err_to_exception(&e, &path))),
            }
        }),
    );
    bc.methods.insert(
        "readAsLines".to_string(),
        h::method(|_v, recv, _args, _n| {
            let path = path_of(&recv);
            crate::permission::require_fs_read(&path)?;
            match fs::read_to_string(&path) {
                Ok(content) => Ok(future_ok(lines_value(&content))),
                Err(e) => Ok(future_err(io_err_to_exception(&e, &path))),
            }
        }),
    );
    bc.methods.insert(
        "writeAsString".to_string(),
        h::method(|_v, recv, args, named| {
            let path = path_of(&recv);
            let content = args
                .first()
                .map(|v| h::as_str(v).unwrap_or("").to_string())
                .unwrap_or_default();
            let mode = named
                .get("mode")
                .map(mode_name)
                .unwrap_or_else(|| "w".to_string());
            match write_with_mode(&path, content.as_bytes(), &mode) {
                Ok(_) => Ok(future_ok(Value::Null)),
                Err(e) => Ok(future_err(io_err_to_exception(&e, &path))),
            }
        }),
    );
    bc.methods.insert(
        "writeAsBytes".to_string(),
        h::method(|_v, recv, args, named| {
            let path = path_of(&recv);
            let bytes = args
                .first()
                .cloned()
                .unwrap_or(Value::Null)
                .let_list()
                .map(|l| {
                    l.borrow()
                        .iter()
                        .filter_map(|v| h::as_int(v).map(|i| i as u8))
                        .collect::<Vec<u8>>()
                })
                .unwrap_or_default();
            let mode = named
                .get("mode")
                .map(mode_name)
                .unwrap_or_else(|| "w".to_string());
            match write_with_mode(&path, &bytes, &mode) {
                Ok(_) => Ok(future_ok(Value::Null)),
                Err(e) => Ok(future_err(io_err_to_exception(&e, &path))),
            }
        }),
    );
    bc.methods.insert(
        "create".to_string(),
        h::method(|_v, recv, _args, named| {
            let path = path_of(&recv);
            let recursive = named.get("recursive").and_then(h::as_bool).unwrap_or(false);
            crate::permission::require_fs_write(&path)?;
            let result = if recursive {
                ensure_parent(&path).and_then(|_| {
                    fs::OpenOptions::new()
                        .create(true)
                        .write(true)
                        .truncate(true)
                        .open(&path)
                        .map(|_| ())
                })
            } else {
                fs::OpenOptions::new()
                    .create(true)
                    .write(true)
                    .truncate(true)
                    .open(&path)
                    .map(|_| ())
            };
            match result {
                Ok(_) => Ok(future_ok(Value::Null)),
                Err(e) => Ok(future_err(io_err_to_exception(&e, &path))),
            }
        }),
    );
    bc.methods.insert(
        "delete".to_string(),
        h::method(|_v, recv, _args, named| {
            let path = path_of(&recv);
            let recursive = named.get("recursive").and_then(h::as_bool).unwrap_or(false);
            crate::permission::require_fs_write(&path)?;
            let result = if Path::new(&path).is_dir() {
                if recursive {
                    fs::remove_dir_all(&path)
                } else {
                    fs::remove_dir(&path)
                }
            } else {
                fs::remove_file(&path)
            };
            match result {
                Ok(_) => Ok(future_ok(Value::Null)),
                Err(e)
                    if e.kind() == std::io::ErrorKind::NotFound && !Path::new(&path).exists() =>
                {
                    Ok(future_ok(Value::Null))
                }
                Err(e) => Ok(future_err(io_err_to_exception(&e, &path))),
            }
        }),
    );
    bc.methods.insert(
        "rename".to_string(),
        h::method(|_v, recv, args, _n| {
            let src = path_of(&recv);
            let dst = args.first().map(path_of).unwrap_or_default();
            crate::permission::require_fs_write(&src)?;
            crate::permission::require_fs_write(&dst)?;
            match fs::rename(&src, &dst) {
                Ok(_) => Ok(future_ok(wrap("File", Value::str(dst)))),
                Err(e) => Ok(future_err(io_err_to_exception(&e, &src))),
            }
        }),
    );
    bc.methods.insert(
        "copy".to_string(),
        h::method(|_v, recv, args, _n| {
            let src = path_of(&recv);
            let dst = args.first().map(path_of).unwrap_or_default();
            crate::permission::require_fs_write(&src)?;
            crate::permission::require_fs_write(&dst)?;
            match fs::copy(&src, &dst) {
                Ok(_) => Ok(future_ok(wrap("File", Value::str(dst)))),
                Err(e) => Ok(future_err(io_err_to_exception(&e, &src))),
            }
        }),
    );
    bc.methods.insert(
        "length".to_string(),
        h::method(|_v, recv, _args, _n| {
            let path = path_of(&recv);
            crate::permission::require_fs_read(&path)?;
            match fs::metadata(&path) {
                Ok(m) => Ok(future_ok(Value::Int(m.len() as i64))),
                Err(e) => Ok(future_err(io_err_to_exception(&e, &path))),
            }
        }),
    );
    bc.methods.insert(
        "lastModified".to_string(),
        h::method(|_v, recv, _args, _n| {
            let path = path_of(&recv);
            crate::permission::require_fs_read(&path)?;
            match fs::metadata(&path).and_then(|m| m.modified()) {
                Ok(t) => Ok(future_ok(us_to_datetime(system_time_us(Some(t))))),
                Err(e) => Ok(future_err(io_err_to_exception(&e, &path))),
            }
        }),
    );
    bc.methods.insert(
        "lastAccessed".to_string(),
        h::method(|_v, recv, _args, _n| {
            let path = path_of(&recv);
            crate::permission::require_fs_read(&path)?;
            match fs::metadata(&path).and_then(|m| m.accessed()) {
                Ok(t) => Ok(future_ok(us_to_datetime(system_time_us(Some(t))))),
                Err(e) => Ok(future_err(io_err_to_exception(&e, &path))),
            }
        }),
    );
    bc.methods.insert(
        "setLastModified".to_string(),
        h::method(|_v, recv, args, _n| {
            let path = path_of(&recv);
            let us = args.first().map(datetime_us).unwrap_or(0);
            match set_modified(&path, us) {
                Ok(_) => Ok(future_ok(Value::Null)),
                Err(e) => Ok(future_err(io_err_to_exception(&e, &path))),
            }
        }),
    );
    bc.methods.insert(
        "setLastAccessed".to_string(),
        h::method(|_v, recv, args, _n| {
            let path = path_of(&recv);
            let us = args.first().map(datetime_us).unwrap_or(0);
            match set_accessed(&path, us) {
                Ok(_) => Ok(future_ok(Value::Null)),
                Err(e) => Ok(future_err(io_err_to_exception(&e, &path))),
            }
        }),
    );
    bc.methods.insert(
        "stat".to_string(),
        h::method(|_v, recv, _args, _n| Ok(future_ok(make_stat(&path_of(&recv))?))),
    );
    bc.methods.insert(
        "resolveSymbolicLinksSync".to_string(),
        h::method(|_v, recv, _args, _n| {
            let path = path_of(&recv);
            resolve_links(&path)
                .map(Value::str)
                .map_err(|e| throw_fs(e.to_string(), &path))
        }),
    );
    bc.methods.insert(
        "resolveSymbolicLinks".to_string(),
        h::method(|_v, recv, _args, _n| {
            let path = path_of(&recv);
            match resolve_links(&path) {
                Ok(s) => Ok(future_ok(Value::str(s))),
                Err(e) => Ok(future_err(io_err_to_exception(&e, &path))),
            }
        }),
    );
    bc.methods.insert(
        "identicalSync".to_string(),
        h::method(|_v, recv, args, _n| {
            let other = args.first().map(path_of).unwrap_or_default();
            Ok(Value::Bool(fs_identical(&path_of(&recv), &other)?))
        }),
    );
    bc.methods.insert(
        "identical".to_string(),
        h::method(|_v, recv, args, _n| {
            let other = args.first().map(path_of).unwrap_or_default();
            Ok(future_ok(Value::Bool(fs_identical(
                &path_of(&recv),
                &other,
            )?)))
        }),
    );
    bc.methods.insert(
        "openRead".to_string(),
        h::method(|_v, recv, _args, _n| {
            let path = path_of(&recv);
            crate::permission::require_fs_read(&path)?;
            match fs::read(&path) {
                Ok(bytes) => Ok(crate::stdlib::stream_lib::stream_from_iterable(vec![
                    bytes_value(bytes),
                ])),
                Err(e) => Err(throw_fs(e.to_string(), &path)),
            }
        }),
    );
    bc.methods.insert(
        "openWrite".to_string(),
        h::method(|_v, recv, _args, named| {
            let path = path_of(&recv);
            let mode = named
                .get("mode")
                .map(mode_name)
                .unwrap_or_else(|| "w".to_string());
            if mode != "w" {
                write_with_mode(&path, &[], &mode).map_err(|e| throw_fs(e.to_string(), &path))?;
            }
            Ok(wrap("IOSink", file_sink_native(&path, &mode)))
        }),
    );
    bc.methods.insert(
        "createTempSync".to_string(),
        h::method(|_v, recv, args, named| {
            let base = Path::new(&path_of(&recv))
                .parent()
                .map(|p| p.to_string_lossy().to_string())
                .filter(|p| !p.is_empty())
                .unwrap_or_else(|| ".".to_string());
            let prefix = args
                .first()
                .map(|v| h::as_str(v).unwrap_or("").to_string())
                .or_else(|| {
                    named
                        .get("prefix")
                        .and_then(|v| h::as_str(v).map(|s| s.to_string()))
                })
                .unwrap_or_else(|| "dart".to_string());
            let suffix = named
                .get("suffix")
                .and_then(|v| h::as_str(v).map(|s| s.to_string()))
                .unwrap_or_default();
            match create_unique_file(&base, &prefix, &suffix) {
                Ok(path) => Ok(wrap("File", Value::str(path))),
                Err(e) => Err(throw_fs(e.to_string(), &base)),
            }
        }),
    );
    bc.methods.insert(
        "createTemp".to_string(),
        h::method(|_v, recv, args, named| {
            let base = Path::new(&path_of(&recv))
                .parent()
                .map(|p| p.to_string_lossy().to_string())
                .filter(|p| !p.is_empty())
                .unwrap_or_else(|| ".".to_string());
            let prefix = args
                .first()
                .map(|v| h::as_str(v).unwrap_or("").to_string())
                .or_else(|| {
                    named
                        .get("prefix")
                        .and_then(|v| h::as_str(v).map(|s| s.to_string()))
                })
                .unwrap_or_else(|| "dart".to_string());
            let suffix = named
                .get("suffix")
                .and_then(|v| h::as_str(v).map(|s| s.to_string()))
                .unwrap_or_default();
            match create_unique_file(&base, &prefix, &suffix) {
                Ok(path) => Ok(future_ok(wrap("File", Value::str(path)))),
                Err(e) => Ok(future_err(io_err_to_exception(&e, &base))),
            }
        }),
    );
    bc.methods.insert(
        "watch".to_string(),
        h::method(|visitor, recv, _args, named| watch_impl(visitor, &path_of(&recv), &named)),
    );
    bc.methods.insert(
        "toString".into(),
        h::method(|_v, t, _a, _n| Ok(Value::str(format!("File: '{}'", path_of(&t))))),
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

fn bytes_value(bytes: Vec<u8>) -> Value {
    Value::List(Rc::new(RefCell::new(
        bytes.into_iter().map(|b| Value::Int(b as i64)).collect(),
    )))
}

fn lines_value(content: &str) -> Value {
    let mut lines: Vec<Value> = content
        .split('\n')
        .map(|l| Value::str(l.strip_suffix('\n').unwrap_or(l)))
        .collect();
    if content.ends_with('\n') {
        lines.pop();
    }
    Value::List(Rc::new(RefCell::new(lines)))
}

fn datetime_us(v: &Value) -> i64 {
    h::as_list(v)
        .and_then(|l| l.borrow().first().cloned())
        .and_then(|x| h::as_int(&x))
        .unwrap_or(0)
}

fn time_from_us(us: i64) -> SystemTime {
    if us >= 0 {
        UNIX_EPOCH + std::time::Duration::from_micros(us as u64)
    } else {
        UNIX_EPOCH - std::time::Duration::from_micros(us.unsigned_abs())
    }
}

fn set_modified(path: &str, us: i64) -> std::io::Result<()> {
    crate::permission::require_fs_write(path).map_err(perm_io)?;
    fs::File::options()
        .write(true)
        .open(path)?
        .set_modified(time_from_us(us))
}

fn set_accessed(path: &str, us: i64) -> std::io::Result<()> {
    crate::permission::require_fs_write(path).map_err(perm_io)?;
    // std::fs::File has no set_accessed; use GNU `touch -a` with epoch time.
    let t = time_from_us(us);
    let secs = t.duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
    let micros = t
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .subsec_micros();
    let status = std::process::Command::new("touch")
        .arg("-a")
        .arg(format!("-d @{secs}.{micros:06}"))
        .arg(path)
        .status();
    match status {
        Ok(st) if st.success() => Ok(()),
        _ => Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "set_accessed unavailable",
        )),
    }
}

fn create_unique_file(base: &str, prefix: &str, suffix: &str) -> std::io::Result<String> {
    crate::permission::require_fs_write(base).map_err(perm_io)?;
    for i in 0..10000 {
        let name = format!("{prefix}{}_{}_{}{suffix}", std::process::id(), now_us(), i);
        let candidate = Path::new(base).join(&name);
        match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&candidate)
        {
            Ok(_) => return Ok(candidate.to_string_lossy().to_string()),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e),
        }
    }
    Err(std::io::Error::new(
        std::io::ErrorKind::AlreadyExists,
        "could not create a unique temp file",
    ))
}

// ---------- RandomAccessFile ----------

/// Native: `[Str(path), Int(position), Str(mode), Bool(closed)]`. Each
/// operation re-opens the file and seeks to the stored position, which
/// keeps the state inside `Value`s (no OS handle is retained).
fn raf_native(path: &str, position: i64, mode: &str) -> Value {
    Value::List(Rc::new(RefCell::new(vec![
        Value::str(path),
        Value::Int(position),
        Value::str(mode),
        Value::Bool(false),
    ])))
}

fn raf_closed(v: &Value) -> bool {
    as_list(v)
        .and_then(|l| l.borrow().get(3).cloned())
        .and_then(|c| h::as_bool(&c))
        .unwrap_or(false)
}

fn set_raf_closed(v: &Value, closed: bool) {
    if let Some(l) = as_list(v)
        && l.borrow().len() > 3
    {
        l.borrow_mut()[3] = Value::Bool(closed);
    }
}

fn raf_path(v: &Value) -> String {
    as_list(v)
        .and_then(|l| l.borrow().first().cloned())
        .and_then(|p| h::as_str(&p).map(|s| s.to_string()))
        .unwrap_or_default()
}

fn raf_position(v: &Value) -> i64 {
    as_list(v)
        .and_then(|l| l.borrow().get(1).cloned())
        .and_then(|p| h::as_int(&p))
        .unwrap_or(0)
}

fn raf_mode(v: &Value) -> String {
    as_list(v)
        .and_then(|l| l.borrow().get(2).cloned())
        .and_then(|p| h::as_str(&p).map(|s| s.to_string()))
        .unwrap_or_else(|| "r".to_string())
}

fn set_raf_position(v: &Value, position: i64) {
    if let Some(l) = as_list(v)
        && l.borrow().len() > 1
    {
        l.borrow_mut()[1] = Value::Int(position);
    }
}

fn as_list(v: &Value) -> Option<Rc<RefCell<Vec<Value>>>> {
    h::as_list(v)
}

/// In-process exclusive file locks for RandomAccessFile (no OS flock
/// dependency). Keyed by (context id, absolute path); `lockSync`/`lock`
/// acquire, `unlockSync`/`unlock`/`close`/`closeSync` release, and
/// [`cleanup_context`] releases everything a dropped `Context` held.
fn locked_files() -> &'static std::sync::Mutex<std::collections::HashSet<(i64, String)>> {
    static LOCKED_FILES: std::sync::OnceLock<
        std::sync::Mutex<std::collections::HashSet<(i64, String)>>,
    > = std::sync::OnceLock::new();
    LOCKED_FILES.get_or_init(|| std::sync::Mutex::new(std::collections::HashSet::new()))
}

fn try_acquire_file_lock(path: &str) -> Result<(), String> {
    let key = (crate::state::current().id, path.to_string());
    let mut locked = locked_files()
        .lock()
        .map_err(|_| "lock registry poisoned".to_string())?;
    if locked.contains(&key) {
        Err(format!("Resource temporarily unavailable: {path}"))
    } else {
        locked.insert(key);
        Ok(())
    }
}

fn release_file_lock(path: &str) {
    let key = (crate::state::current().id, path.to_string());
    if let Ok(mut locked) = locked_files().lock() {
        locked.remove(&key);
    }
}

/// Releases every file lock held by `ctx_id` (called when the owning
/// `Context` is dropped).
pub(crate) fn release_context_locks(ctx_id: i64) {
    if let Ok(mut locked) = locked_files().lock() {
        locked.retain(|(c, _)| *c != ctx_id);
    }
}

/// Reclaims every watcher/pipe/lock this `Context` created.
pub(crate) fn cleanup_context(ctx_id: i64) {
    cleanup_watchers(ctx_id);
    cleanup_pipes(ctx_id);
    release_context_locks(ctx_id);
}

/// Opens `path` in `mode` (`r/w/a/w+/a+`), honoring append start-position
/// and truncation, and returns a RandomAccessFile value.
fn open_raf(path: &str, mode: &str) -> Result<Value, std::io::Error> {
    if mode == "w" {
        crate::permission::require_fs_write(path).map_err(perm_io)?;
        fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(path)?;
    }
    let pos = if mode.starts_with('a') {
        fs::metadata(path).map(|m| m.len() as i64).unwrap_or(0)
    } else {
        0
    };
    if mode != "w" {
        crate::permission::require_fs_read(path).map_err(perm_io)?;
    }
    Ok(wrap("RandomAccessFile", raf_native(path, pos, mode)))
}

/// Reads up to `count` bytes at the stored position, advancing it.
fn raf_read(v: &Value, count: usize) -> std::io::Result<Vec<u8>> {
    use std::io::{Read, Seek, SeekFrom};
    if raf_closed(v) {
        return Err(std::io::Error::other("Cannot read from a closed file"));
    }
    let path = raf_path(v);
    crate::permission::require_fs_read(&path).map_err(perm_io)?;
    let start = raf_position(v) as u64;
    let mut f = fs::File::open(&path)?;
    f.seek(SeekFrom::Start(start))?;
    let mut buf = vec![0u8; count];
    let mut got = 0usize;
    while got < count {
        let n = f.read(&mut buf[got..])?;
        if n == 0 {
            break;
        }
        got += n;
    }
    buf.truncate(got);
    set_raf_position(v, start as i64 + got as i64);
    Ok(buf)
}

/// Writes `bytes` at the stored position, advancing it.
fn raf_write(v: &Value, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::{Seek, SeekFrom, Write};
    if raf_closed(v) {
        return Err(std::io::Error::other("Cannot write to a closed file"));
    }
    let path = raf_path(v);
    crate::permission::require_fs_write(&path).map_err(perm_io)?;
    let mode = raf_mode(v);
    let start = raf_position(v) as u64;
    let mut f = fs::OpenOptions::new()
        .create(mode == "w" || mode.starts_with('a') || mode.starts_with('w'))
        .write(true)
        .open(&path)?;
    f.seek(SeekFrom::Start(start))?;
    f.write_all(bytes)?;
    set_raf_position(v, start as i64 + bytes.len() as i64);
    Ok(())
}

fn list_bytes(v: &Value) -> Vec<u8> {
    h::as_list(v)
        .map(|l| {
            l.borrow()
                .iter()
                .filter_map(|x| h::as_int(x).map(|i| i as u8))
                .collect()
        })
        .unwrap_or_default()
}

type SyncMethod = Box<
    dyn Fn(
            &mut InterpreterVisitor,
            Value,
            Vec<Value>,
            std::collections::HashMap<String, Value>,
        ) -> Result<Value, InterpError>
        + Send
        + Sync
        + 'static,
>;
type AsyncImpl = Box<
    dyn Fn(
            &Value,
            &[Value],
            &std::collections::HashMap<String, Value>,
        ) -> Result<Value, InterpError>
        + Send
        + Sync
        + 'static,
>;

fn random_access_file_definition() -> BridgedClassDef {
    let mut bc = BridgedClassDef::new("RandomAccessFile");
    bc.native_type = Some("d4rt::io::RandomAccessFile".to_string());

    // ---- sync methods ----
    let mut sync = |name: &str, f: SyncMethod| {
        bc.methods.insert(name.to_string(), h::method(f));
    };
    sync(
        "closeSync",
        Box::new(|_v, recv, _args, _n| {
            let path = raf_path(&recv);
            set_raf_closed(&recv, true);
            release_file_lock(&path);
            Ok(Value::Null)
        }),
    );
    sync(
        "readByteSync",
        Box::new(|_v, recv, _args, _n| {
            let recv = &recv;
            let path = raf_path(recv);
            match raf_read(recv, 1) {
                Ok(bytes) => Ok(bytes
                    .first()
                    .map(|b| Value::Int(*b as i64))
                    .unwrap_or(Value::Null)),
                Err(e) => Err(throw_fs(e.to_string(), &path)),
            }
        }),
    );
    sync(
        "readSync",
        Box::new(|_v, recv, args, _n| {
            let recv = &recv;
            let count = args.first().and_then(h::as_int).unwrap_or(0).max(0) as usize;
            let path = raf_path(recv);
            match raf_read(recv, count) {
                Ok(bytes) => {
                    if bytes.is_empty() {
                        Ok(Value::Null)
                    } else {
                        Ok(bytes_value(bytes))
                    }
                }
                Err(e) => Err(throw_fs(e.to_string(), &path)),
            }
        }),
    );
    sync(
        "readIntoSync",
        Box::new(|_v, recv, args, _n| {
            let recv = &recv;
            let buffer = args.first().cloned().unwrap_or(Value::Null);
            let Some(buffer_list) = h::as_list(&buffer) else {
                return Err(throw_fs(
                    "readIntoSync requires a List<int> buffer",
                    &raf_path(recv),
                ));
            };
            let len = buffer_list.borrow().len();
            let start = args
                .get(1)
                .and_then(h::as_int)
                .unwrap_or(0)
                .clamp(0, len as i64) as usize;
            let end = args
                .get(2)
                .and_then(h::as_int)
                .unwrap_or(len as i64)
                .clamp(0, len as i64) as usize;
            let count = end.saturating_sub(start);
            let path = raf_path(recv);
            match raf_read(recv, count) {
                Ok(bytes) => {
                    let mut l = buffer_list.borrow_mut();
                    for (i, b) in bytes.iter().enumerate() {
                        l[start + i] = Value::Int(*b as i64);
                    }
                    Ok(Value::Int(bytes.len() as i64))
                }
                Err(e) => Err(throw_fs(e.to_string(), &path)),
            }
        }),
    );
    sync(
        "writeByteSync",
        Box::new(|_v, recv, args, _n| {
            let recv = &recv;
            let byte = args.first().and_then(h::as_int).unwrap_or(0) as u8;
            let path = raf_path(recv);
            raf_write(recv, &[byte]).map_err(|e| throw_fs(e.to_string(), &path))?;
            Ok(Value::Null)
        }),
    );
    sync(
        "writeFromSync",
        Box::new(|_v, recv, args, _n| {
            let recv = &recv;
            let buffer = args.first().cloned().unwrap_or(Value::Null);
            let bytes = list_bytes(&buffer);
            let start = args
                .get(1)
                .and_then(h::as_int)
                .unwrap_or(0)
                .clamp(0, bytes.len() as i64) as usize;
            let end = args
                .get(2)
                .and_then(h::as_int)
                .unwrap_or(bytes.len() as i64)
                .clamp(0, bytes.len() as i64) as usize;
            let path = raf_path(recv);
            raf_write(recv, &bytes[start..end]).map_err(|e| throw_fs(e.to_string(), &path))?;
            Ok(Value::Null)
        }),
    );
    sync(
        "writeStringSync",
        Box::new(|_v, recv, args, _n| {
            let recv = &recv;
            let s = args
                .first()
                .map(|v| h::as_str(v).unwrap_or("").to_string())
                .unwrap_or_default();
            let path = raf_path(recv);
            raf_write(recv, s.as_bytes()).map_err(|e| throw_fs(e.to_string(), &path))?;
            Ok(Value::Null)
        }),
    );
    sync(
        "positionSync",
        Box::new(|_v, recv, _args, _n| Ok(Value::Int(raf_position(&recv)))),
    );
    sync(
        "setPositionSync",
        Box::new(|_v, recv, args, _n| {
            let pos = args.first().and_then(h::as_int).unwrap_or(0);
            set_raf_position(&recv, pos);
            Ok(Value::Null)
        }),
    );
    sync(
        "truncateSync",
        Box::new(|_v, recv, args, _n| {
            let recv = &recv;
            let len = args.first().and_then(h::as_int).unwrap_or(0).max(0) as u64;
            let path = raf_path(recv);
            crate::permission::require_fs_write(&path)?;
            fs::OpenOptions::new()
                .write(true)
                .open(&path)
                .and_then(|f| f.set_len(len))
                .map_err(|e| throw_fs(e.to_string(), &path))?;
            Ok(Value::Null)
        }),
    );
    sync(
        "lengthSync",
        Box::new(|_v, recv, _args, _n| {
            let recv = &recv;
            let path = raf_path(recv);
            crate::permission::require_fs_read(&path)?;
            match fs::metadata(&path) {
                Ok(m) => Ok(Value::Int(m.len() as i64)),
                Err(e) => Err(throw_fs(e.to_string(), &path)),
            }
        }),
    );
    sync(
        "flushSync",
        Box::new(|_v, recv, _args, _n| {
            let path = raf_path(&recv);
            crate::permission::require_fs_read(&path)?;
            fs::File::open(&path)
                .and_then(|f| f.sync_all())
                .map_err(|e| throw_fs(e.to_string(), &path))?;
            Ok(Value::Null)
        }),
    );
    sync(
        "lockSync",
        Box::new(|_v, recv, _args, _n| {
            let path = raf_path(&recv);
            try_acquire_file_lock(&path).map_err(|e| throw_fs(e, &path))?;
            Ok(Value::Null)
        }),
    );
    sync(
        "unlockSync",
        Box::new(|_v, recv, _args, _n| {
            let path = raf_path(&recv);
            release_file_lock(&path);
            Ok(Value::Null)
        }),
    );

    // ---- async wrappers ----
    let mut r#async = |name: &str, f: AsyncImpl| {
        bc.methods.insert(
            name.to_string(),
            h::method(move |_v, recv, args, named| match f(&recv, &args, &named) {
                Ok(v) => Ok(future_ok(v)),
                Err(InterpError::Throw(t)) => Ok(future_err(t)),
                Err(e) => Ok(future_err(Value::str(e.to_string()))),
            }),
        );
    };
    r#async(
        "close",
        Box::new(|recv, _args, _n| {
            let path = raf_path(recv);
            set_raf_closed(recv, true);
            release_file_lock(&path);
            Ok(Value::Null)
        }),
    );
    r#async(
        "readByte",
        Box::new(|recv, _args, _n| {
            let path = raf_path(recv);
            match raf_read(recv, 1) {
                Ok(bytes) => Ok(bytes
                    .first()
                    .map(|b| Value::Int(*b as i64))
                    .unwrap_or(Value::Null)),
                Err(e) => Err(throw_fs(e.to_string(), &path)),
            }
        }),
    );
    r#async(
        "read",
        Box::new(|recv, args, _n| {
            let count = args.first().and_then(h::as_int).unwrap_or(0).max(0) as usize;
            let path = raf_path(recv);
            match raf_read(recv, count) {
                Ok(bytes) => Ok(bytes_value(bytes)),
                Err(e) => Err(throw_fs(e.to_string(), &path)),
            }
        }),
    );
    r#async(
        "readInto",
        Box::new(|recv, args, _n| {
            let buffer = args.first().cloned().unwrap_or(Value::Null);
            let Some(buffer_list) = h::as_list(&buffer) else {
                return Err(throw_fs(
                    "readInto requires a List<int> buffer",
                    &raf_path(recv),
                ));
            };
            let len = buffer_list.borrow().len();
            let start = args
                .get(1)
                .and_then(h::as_int)
                .unwrap_or(0)
                .clamp(0, len as i64) as usize;
            let end = args
                .get(2)
                .and_then(h::as_int)
                .unwrap_or(len as i64)
                .clamp(0, len as i64) as usize;
            let count = end.saturating_sub(start);
            let path = raf_path(recv);
            match raf_read(recv, count) {
                Ok(bytes) => {
                    let mut l = buffer_list.borrow_mut();
                    for (i, b) in bytes.iter().enumerate() {
                        l[start + i] = Value::Int(*b as i64);
                    }
                    Ok(Value::Int(bytes.len() as i64))
                }
                Err(e) => Err(throw_fs(e.to_string(), &path)),
            }
        }),
    );
    r#async(
        "writeByte",
        Box::new(|recv, args, _n| {
            let byte = args.first().and_then(h::as_int).unwrap_or(0) as u8;
            let path = raf_path(recv);
            raf_write(recv, &[byte]).map_err(|e| throw_fs(e.to_string(), &path))?;
            Ok(Value::Null)
        }),
    );
    r#async(
        "writeFrom",
        Box::new(|recv, args, _n| {
            let buffer = args.first().cloned().unwrap_or(Value::Null);
            let bytes = list_bytes(&buffer);
            let start = args
                .get(1)
                .and_then(h::as_int)
                .unwrap_or(0)
                .clamp(0, bytes.len() as i64) as usize;
            let end = args
                .get(2)
                .and_then(h::as_int)
                .unwrap_or(bytes.len() as i64)
                .clamp(0, bytes.len() as i64) as usize;
            let path = raf_path(recv);
            raf_write(recv, &bytes[start..end]).map_err(|e| throw_fs(e.to_string(), &path))?;
            Ok(Value::Null)
        }),
    );
    r#async(
        "writeString",
        Box::new(|recv, args, _n| {
            let s = args
                .first()
                .map(|v| h::as_str(v).unwrap_or("").to_string())
                .unwrap_or_default();
            let path = raf_path(recv);
            raf_write(recv, s.as_bytes()).map_err(|e| throw_fs(e.to_string(), &path))?;
            Ok(Value::Null)
        }),
    );
    r#async(
        "position",
        Box::new(|recv, _args, _n| Ok(Value::Int(raf_position(recv)))),
    );
    r#async(
        "setPosition",
        Box::new(|recv, args, _n| {
            let pos = args.first().and_then(h::as_int).unwrap_or(0);
            set_raf_position(recv, pos);
            Ok(Value::Null)
        }),
    );
    r#async(
        "truncate",
        Box::new(|recv, args, _n| {
            let len = args.first().and_then(h::as_int).unwrap_or(0).max(0) as u64;
            let path = raf_path(recv);
            crate::permission::require_fs_write(&path)?;
            fs::OpenOptions::new()
                .write(true)
                .open(&path)
                .and_then(|f| f.set_len(len))
                .map_err(|e| throw_fs(e.to_string(), &path))?;
            Ok(Value::Null)
        }),
    );
    r#async(
        "length",
        Box::new(|recv, _args, _n| {
            let path = raf_path(recv);
            crate::permission::require_fs_read(&path)?;
            match fs::metadata(&path) {
                Ok(m) => Ok(Value::Int(m.len() as i64)),
                Err(e) => Err(throw_fs(e.to_string(), &path)),
            }
        }),
    );
    r#async(
        "flush",
        Box::new(|recv, _args, _n| {
            let path = raf_path(recv);
            crate::permission::require_fs_read(&path)?;
            fs::File::open(&path)
                .and_then(|f| f.sync_all())
                .map_err(|e| throw_fs(e.to_string(), &path))?;
            Ok(Value::Null)
        }),
    );
    r#async(
        "lock",
        Box::new(|recv, _args, _n| {
            let path = raf_path(recv);
            try_acquire_file_lock(&path).map_err(|e| throw_fs(e, &path))?;
            Ok(wrap("FileLock", Value::str("exclusive")))
        }),
    );
    r#async(
        "unlock",
        Box::new(|recv, _args, _n| {
            let path = raf_path(recv);
            release_file_lock(&path);
            Ok(Value::Null)
        }),
    );

    bc.getters.insert(
        "path".to_string(),
        h::getter(|_v, recv| Ok(Value::str(raf_path(&recv)))),
    );
    bc
}

// ---------- Directory ----------

fn directory_definition() -> BridgedClassDef {
    let mut bc = BridgedClassDef::new("Directory");
    bc.static_methods.insert(
        "createTempSync".to_string(),
        h::static_method(|_v, args, _n| {
            let base = std::env::temp_dir().to_string_lossy().to_string();
            let prefix = args
                .first()
                .map(|v| h::as_str(v).unwrap_or("").to_string())
                .unwrap_or_default();
            match create_unique_dir(&base, &prefix) {
                Ok(dir) => Ok(wrap("Directory", Value::str(dir))),
                Err(e) => Err(throw_fs(e.to_string(), &base)),
            }
        }),
    );
    bc.static_methods.insert(
        "createTemp".to_string(),
        h::static_method(|_v, args, _n| {
            let base = std::env::temp_dir().to_string_lossy().to_string();
            let prefix = args
                .first()
                .map(|v| h::as_str(v).unwrap_or("").to_string())
                .unwrap_or_default();
            match create_unique_dir(&base, &prefix) {
                Ok(dir) => Ok(future_ok(wrap("Directory", Value::str(dir)))),
                Err(e) => Ok(future_err(io_err_to_exception(&e, &base))),
            }
        }),
    );
    bc.constructors.insert(
        "".to_string(),
        h::ctor(|_v, args, _n| {
            let path = args.first().map(path_of).unwrap_or_default();
            Ok(wrap("Directory", Value::str(path)))
        }),
    );
    bc.constructors.insert(
        "fromUri".to_string(),
        h::ctor(|_v, args, _n| {
            let path = args.first().map(path_from_uri).unwrap_or_default();
            Ok(wrap("Directory", Value::str(path)))
        }),
    );
    bc.constructors.insert(
        "fromRawPath".to_string(),
        h::ctor(|_v, args, _n| {
            let path = args.first().map(path_from_raw_bytes).unwrap_or_default();
            Ok(wrap("Directory", Value::str(path)))
        }),
    );
    bc.getters.insert(
        "path".to_string(),
        h::getter(|_v, recv| Ok(Value::str(path_of(&recv)))),
    );
    bc.getters.insert(
        "absolute".to_string(),
        h::getter(|_v, recv| {
            Ok(wrap(
                "Directory",
                Value::str(absolute_path(&path_of(&recv))),
            ))
        }),
    );
    bc.getters.insert(
        "isAbsolute".to_string(),
        h::getter(|_v, recv| Ok(Value::Bool(Path::new(&path_of(&recv)).is_absolute()))),
    );
    bc.getters.insert(
        "uri".to_string(),
        h::getter(|_v, recv| Ok(uri_of(&path_of(&recv)))),
    );
    bc.getters.insert(
        "parent".to_string(),
        h::getter(|_v, recv| {
            let path = path_of(&recv);
            let parent = Path::new(&path)
                .parent()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_default();
            Ok(wrap("Directory", Value::str(parent)))
        }),
    );
    bc.static_getters.insert(
        "current".to_string(),
        h::static_getter(|_v| {
            let path = std::env::current_dir()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_default();
            Ok(wrap("Directory", Value::str(path)))
        }),
    );
    bc.static_getters.insert(
        "systemTemp".to_string(),
        h::static_getter(|_v| {
            let path = std::env::temp_dir().to_string_lossy().to_string();
            Ok(wrap("Directory", Value::str(path)))
        }),
    );
    bc.methods.insert(
        "existsSync".to_string(),
        h::method(|_v, recv, _args, _n| {
            let path = path_of(&recv);
            crate::permission::require_fs_read(&path)?;
            Ok(Value::Bool(Path::new(&path).is_dir()))
        }),
    );
    bc.methods.insert(
        "exists".to_string(),
        h::method(|_v, recv, _args, _n| {
            let path = path_of(&recv);
            crate::permission::require_fs_read(&path)?;
            Ok(future_ok(Value::Bool(Path::new(&path).is_dir())))
        }),
    );
    bc.methods.insert(
        "createSync".to_string(),
        h::method(|_v, recv, _args, named| {
            let path = path_of(&recv);
            let recursive = named.get("recursive").and_then(h::as_bool).unwrap_or(false);
            crate::permission::require_fs_write(&path)?;
            let result = if recursive {
                fs::create_dir_all(&path)
            } else {
                fs::create_dir(&path)
            };
            result.map_err(|e| throw_fs(e.to_string(), &path))?;
            Ok(Value::Null)
        }),
    );
    bc.methods.insert(
        "create".to_string(),
        h::method(|_v, recv, _args, named| {
            let path = path_of(&recv);
            let recursive = named.get("recursive").and_then(h::as_bool).unwrap_or(false);
            crate::permission::require_fs_write(&path)?;
            let result = if recursive {
                fs::create_dir_all(&path)
            } else {
                fs::create_dir(&path)
            };
            match result {
                Ok(_) => Ok(future_ok(Value::Null)),
                Err(e) => Ok(future_err(io_err_to_exception(&e, &path))),
            }
        }),
    );
    bc.methods.insert(
        "deleteSync".to_string(),
        h::method(|_v, recv, _args, named| {
            let path = path_of(&recv);
            let recursive = named.get("recursive").and_then(h::as_bool).unwrap_or(false);
            crate::permission::require_fs_write(&path)?;
            let result = if recursive {
                fs::remove_dir_all(&path)
            } else {
                fs::remove_dir(&path)
            };
            result.map_err(|e| throw_fs(e.to_string(), &path))?;
            Ok(Value::Null)
        }),
    );
    bc.methods.insert(
        "delete".to_string(),
        h::method(|_v, recv, _args, named| {
            let path = path_of(&recv);
            let recursive = named.get("recursive").and_then(h::as_bool).unwrap_or(false);
            crate::permission::require_fs_write(&path)?;
            let result = if recursive {
                fs::remove_dir_all(&path)
            } else {
                fs::remove_dir(&path)
            };
            match result {
                Ok(_) => Ok(future_ok(Value::Null)),
                Err(e) => Ok(future_err(io_err_to_exception(&e, &path))),
            }
        }),
    );
    bc.methods.insert(
        "renameSync".to_string(),
        h::method(|_v, recv, args, _n| {
            let src = path_of(&recv);
            let dst = args.first().map(path_of).unwrap_or_default();
            crate::permission::require_fs_write(&src)?;
            crate::permission::require_fs_write(&dst)?;
            fs::rename(&src, &dst).map_err(|e| throw_fs(e.to_string(), &src))?;
            Ok(wrap("Directory", Value::str(dst)))
        }),
    );
    bc.methods.insert(
        "rename".to_string(),
        h::method(|_v, recv, args, _n| {
            let src = path_of(&recv);
            let dst = args.first().map(path_of).unwrap_or_default();
            crate::permission::require_fs_write(&src)?;
            crate::permission::require_fs_write(&dst)?;
            match fs::rename(&src, &dst) {
                Ok(_) => Ok(future_ok(wrap("Directory", Value::str(dst)))),
                Err(e) => Ok(future_err(io_err_to_exception(&e, &src))),
            }
        }),
    );
    bc.methods.insert(
        "resolveSymbolicLinksSync".to_string(),
        h::method(|_v, recv, _args, _n| {
            let path = path_of(&recv);
            resolve_links(&path)
                .map(Value::str)
                .map_err(|e| throw_fs(e.to_string(), &path))
        }),
    );
    bc.methods.insert(
        "resolveSymbolicLinks".to_string(),
        h::method(|_v, recv, _args, _n| {
            let path = path_of(&recv);
            match resolve_links(&path) {
                Ok(s) => Ok(future_ok(Value::str(s))),
                Err(e) => Ok(future_err(io_err_to_exception(&e, &path))),
            }
        }),
    );
    bc.methods.insert(
        "createTempSync".to_string(),
        h::method(|_v, recv, args, named| {
            let base = path_of(&recv);
            let prefix = args
                .first()
                .map(|v| h::as_str(v).unwrap_or("").to_string())
                .or_else(|| {
                    named
                        .get("prefix")
                        .and_then(|v| h::as_str(v).map(|s| s.to_string()))
                })
                .unwrap_or_else(|| "dart".to_string());
            match create_unique_dir(&base, &prefix) {
                Ok(dir) => Ok(wrap("Directory", Value::str(dir))),
                Err(e) => Err(throw_fs(e.to_string(), &base)),
            }
        }),
    );
    bc.methods.insert(
        "createTemp".to_string(),
        h::method(|_v, recv, args, named| {
            let base = path_of(&recv);
            let prefix = args
                .first()
                .map(|v| h::as_str(v).unwrap_or("").to_string())
                .or_else(|| {
                    named
                        .get("prefix")
                        .and_then(|v| h::as_str(v).map(|s| s.to_string()))
                })
                .unwrap_or_else(|| "dart".to_string());
            match create_unique_dir(&base, &prefix) {
                Ok(dir) => Ok(future_ok(wrap("Directory", Value::str(dir)))),
                Err(e) => Ok(future_err(io_err_to_exception(&e, &base))),
            }
        }),
    );
    bc.methods.insert(
        "watch".to_string(),
        h::method(|visitor, recv, _args, named| watch_impl(visitor, &path_of(&recv), &named)),
    );
    bc.methods.insert(
        "listSync".to_string(),
        h::method(|_v, recv, args, named| {
            let path = path_of(&recv);
            let recursive = args
                .first()
                .and_then(h::as_bool)
                .or_else(|| named.get("recursive").and_then(h::as_bool))
                .unwrap_or(false);
            list_entities(&path, recursive)
                .map(|entities| Value::List(Rc::new(RefCell::new(entities))))
                .map_err(|e| throw_fs(e.to_string(), &path))
        }),
    );
    bc.methods.insert(
        "list".to_string(),
        h::method(|_v, recv, args, named| {
            let path = path_of(&recv);
            let recursive = args
                .first()
                .and_then(h::as_bool)
                .or_else(|| named.get("recursive").and_then(h::as_bool))
                .unwrap_or(false);
            match list_entities(&path, recursive) {
                Ok(entities) => Ok(crate::stdlib::stream_lib::stream_from_iterable(entities)),
                Err(e) => Ok(crate::stdlib::stream_lib::stream_from_iterable(vec![
                    Value::List(Rc::new(RefCell::new(vec![
                        Value::Bool(true),
                        io_err_to_exception(&e, &path),
                    ]))),
                ])),
            }
        }),
    );
    bc.methods.insert(
        "statSync".to_string(),
        h::method(|_v, recv, _args, _n| make_stat(&path_of(&recv))),
    );
    bc.methods.insert(
        "stat".to_string(),
        h::method(|_v, recv, _args, _n| Ok(future_ok(make_stat(&path_of(&recv))?))),
    );
    bc.methods.insert(
        "toString".into(),
        h::method(|_v, t, _a, _n| Ok(Value::str(format!("Directory: '{}'", path_of(&t))))),
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

fn create_unique_dir(base: &str, prefix: &str) -> std::io::Result<String> {
    crate::permission::require_fs_write(base).map_err(perm_io)?;
    for i in 0..10000 {
        let name = format!("{}{}_{}_{}", prefix, std::process::id(), now_us(), i);
        let candidate = Path::new(base).join(&name);
        match fs::create_dir(&candidate) {
            Ok(_) => return Ok(candidate.to_string_lossy().to_string()),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e),
        }
    }
    Err(std::io::Error::new(
        std::io::ErrorKind::AlreadyExists,
        "could not create a unique temp directory",
    ))
}

fn list_entities(path: &str, recursive: bool) -> Result<Vec<Value>, std::io::Error> {
    crate::permission::require_fs_read(path).map_err(perm_io)?;
    let mut result: Vec<Value> = Vec::new();
    fn walk(dir: &Path, recursive: bool, out: &mut Vec<Value>) -> std::io::Result<()> {
        let mut names: Vec<String> = fs::read_dir(dir)?
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().to_string())
            .collect();
        names.sort();
        for name in names {
            let full = dir.join(&name);
            let full_s = full.to_string_lossy().to_string();
            let is_dir = full.is_dir();
            out.push(wrap(
                if is_dir { "Directory" } else { "File" },
                Value::str(full_s),
            ));
            if recursive && is_dir {
                walk(&full, true, out)?;
            }
        }
        Ok(())
    }
    walk(Path::new(path), recursive, &mut result)?;
    Ok(result)
}

// ---------- Platform ----------

fn platform_definition() -> BridgedClassDef {
    let mut bc = BridgedClassDef::new("Platform");
    bc.static_getters.insert(
        "numberOfProcessors".into(),
        h::static_getter(|_v| {
            Ok(Value::Int(
                std::thread::available_parallelism()
                    .map(|n| n.get() as i64)
                    .unwrap_or(1),
            ))
        }),
    );
    bc.static_getters.insert(
        "pathSeparator".into(),
        h::static_getter(|_v| Ok(Value::str(std::path::MAIN_SEPARATOR.to_string()))),
    );
    bc.static_getters.insert(
        "operatingSystem".into(),
        h::static_getter(|_v| Ok(Value::str(operating_system()))),
    );
    bc.static_getters.insert(
        "operatingSystemVersion".into(),
        h::static_getter(|_v| Ok(Value::str(operating_system_version()))),
    );
    bc.static_getters.insert(
        "localHostname".into(),
        h::static_getter(|_v| {
            let host = std::env::var("HOSTNAME")
                .or_else(|_| std::env::var("COMPUTERNAME"))
                .unwrap_or_else(|_| "localhost".to_string());
            Ok(Value::str(host))
        }),
    );
    bc.static_getters.insert(
        "environment".into(),
        h::static_getter(|_v| {
            crate::permission::require_env()?;
            let mut map = DartMap::new();
            for (k, v) in std::env::vars() {
                map.entries.push((Value::str(k), Value::str(v)));
            }
            Ok(Value::Map(Rc::new(RefCell::new(map))))
        }),
    );
    bc.static_getters.insert(
        "packageConfig".into(),
        h::static_getter(|_v| {
            let start = crate::stdlib::main_script_path()
                .map(PathBuf::from)
                .and_then(|p| p.parent().map(|d| d.to_path_buf()))
                .unwrap_or_else(|| std::env::current_dir().unwrap_or_default());
            let mut dir = Some(start);
            while let Some(d) = dir {
                let candidate = d.join("package_config.json");
                if candidate.is_file() {
                    return Ok(uri_of(&candidate.to_string_lossy()));
                }
                dir = d.parent().map(|p| p.to_path_buf());
            }
            Ok(Value::Null)
        }),
    );
    bc.static_getters.insert(
        "executable".into(),
        h::static_getter(|_v| {
            Ok(Value::str(
                std::env::current_exe()
                    .map(|p| p.to_string_lossy().to_string())
                    .unwrap_or_default(),
            ))
        }),
    );
    bc.static_getters.insert(
        "resolvedExecutable".into(),
        h::static_getter(|_v| {
            Ok(Value::str(
                std::env::current_exe()
                    .ok()
                    .and_then(|p| fs::canonicalize(p).ok())
                    .map(|p| p.to_string_lossy().to_string())
                    .unwrap_or_default(),
            ))
        }),
    );
    bc.static_getters.insert(
        "script".into(),
        h::static_getter(|_v| {
            let path = crate::stdlib::main_script_path().unwrap_or_else(|| {
                std::env::current_exe()
                    .map(|p| p.to_string_lossy().to_string())
                    .unwrap_or_default()
            });
            let uri = if path.starts_with('/') {
                format!("file://{path}")
            } else {
                format!("file:///{path}")
            };
            Ok(h::wrap(&h::class("Uri"), Value::str(uri)))
        }),
    );
    bc.static_getters.insert(
        "executableArguments".into(),
        h::static_getter(|_v| {
            let args: Vec<Value> = std::env::args().skip(1).map(Value::str).collect();
            Ok(Value::List(Rc::new(RefCell::new(args))))
        }),
    );
    bc.static_getters.insert(
        "version".into(),
        h::static_getter(|_v| Ok(Value::str("3.5.0 (d4rt_rs)"))),
    );
    bc.static_getters.insert(
        "localeName".into(),
        h::static_getter(|_v| {
            let locale = std::env::var("LC_ALL")
                .or_else(|_| std::env::var("LANG"))
                .unwrap_or_else(|_| "en_US".to_string());
            Ok(Value::str(locale))
        }),
    );
    bc.static_getters.insert(
        "isLinux".into(),
        h::static_getter(|_v| Ok(Value::Bool(cfg!(target_os = "linux")))),
    );
    bc.static_getters.insert(
        "isMacOS".into(),
        h::static_getter(|_v| Ok(Value::Bool(cfg!(target_os = "macos")))),
    );
    bc.static_getters.insert(
        "isWindows".into(),
        h::static_getter(|_v| Ok(Value::Bool(cfg!(target_os = "windows")))),
    );
    bc.static_getters.insert(
        "isAndroid".into(),
        h::static_getter(|_v| Ok(Value::Bool(cfg!(target_os = "android")))),
    );
    bc.static_getters.insert(
        "isIOS".into(),
        h::static_getter(|_v| Ok(Value::Bool(cfg!(target_os = "ios")))),
    );
    bc.static_getters.insert(
        "isFuchsia".into(),
        h::static_getter(|_v| Ok(Value::Bool(cfg!(target_os = "fuchsia")))),
    );
    bc
}

fn operating_system() -> &'static str {
    if cfg!(target_os = "linux") {
        "linux"
    } else if cfg!(target_os = "macos") {
        "macos"
    } else if cfg!(target_os = "windows") {
        "windows"
    } else if cfg!(target_os = "android") {
        "android"
    } else if cfg!(target_os = "ios") {
        "ios"
    } else if cfg!(target_os = "fuchsia") {
        "fuchsia"
    } else {
        "unknown"
    }
}

fn operating_system_version() -> String {
    if cfg!(target_os = "linux") {
        fs::read_to_string("/proc/sys/kernel/osrelease")
            .map(|s| s.trim().to_string())
            .unwrap_or_else(|_| "linux".to_string())
    } else {
        operating_system().to_string()
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

// ---------- stdio ----------

fn stdio_tag(v: &Value) -> &'static str {
    match h::as_list(v).and_then(|l| l.borrow().first().cloned()) {
        Some(Value::Str(s)) if s.as_ref() == "stderr" => "stderr",
        _ => "stdout",
    }
}

fn stdio_done_state(v: &Value) -> Option<Rc<RefCell<async_state::FutureState>>> {
    h::as_list(v)
        .and_then(|l| l.borrow().get(1).cloned())
        .and_then(|f| match f {
            Value::Future(st) => Some(st),
            _ => None,
        })
}

fn stdio_write(v: &Value, bytes: &[u8]) {
    if stdio_tag(v) == "stderr" {
        let _ = std::io::stderr().lock().write_all(bytes);
    } else {
        let _ = std::io::stdout().lock().write_all(bytes);
    }
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

fn stdin_definition() -> BridgedClassDef {
    let mut bc = BridgedClassDef::new("Stdin");
    bc.native_type = Some("d4rt::io::Stdin".to_string());
    bc.methods.insert(
        "readLineSync".to_string(),
        h::method(|_v, _r, _a, named| {
            let retain = named
                .get("retainNewlines")
                .and_then(h::as_bool)
                .unwrap_or(false);
            let mut s = String::new();
            match std::io::stdin().read_line(&mut s) {
                Ok(0) | Err(_) => Ok(Value::Null),
                Ok(_) => {
                    if !retain {
                        while s.ends_with('\n') || s.ends_with('\r') {
                            s.pop();
                        }
                    }
                    Ok(Value::str(s))
                }
            }
        }),
    );
    bc.methods.insert(
        "readByteSync".to_string(),
        h::method(|_v, _r, _a, _n| {
            let mut b = [0u8; 1];
            match std::io::stdin().read(&mut b) {
                Ok(0) | Err(_) => Ok(Value::Null),
                Ok(_) => Ok(Value::Int(b[0] as i64)),
            }
        }),
    );
    bc.methods.insert(
        "listen".to_string(),
        h::method(|visitor, _recv, args, named| {
            let on_data = args.first().cloned().unwrap_or(Value::Null);
            let on_done = named.get("onDone").cloned().unwrap_or(Value::Null);
            let mut buf = [0u8; 8192];
            loop {
                match std::io::stdin().read(&mut buf) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        if !matches!(on_data, Value::Null) {
                            visitor.call_value(
                                &on_data,
                                vec![bytes_value(buf[..n].to_vec())],
                                HashMap::new(),
                            )?;
                        }
                    }
                }
            }
            if !matches!(on_done, Value::Null) {
                visitor.call_value(&on_done, Vec::new(), HashMap::new())?;
            }
            Ok(Value::Null)
        }),
    );
    bc.getters.insert(
        "hasTerminal".to_string(),
        h::getter(|_v, _r| Ok(Value::Bool(std::io::stdin().is_terminal()))),
    );
    bc.getters.insert(
        "echoMode".to_string(),
        h::getter(|_v, _r| Ok(Value::Bool(true))),
    );
    bc.getters.insert(
        "lineMode".to_string(),
        h::getter(|_v, _r| Ok(Value::Bool(true))),
    );
    bc.getters.insert(
        "echoNewlineMode".to_string(),
        h::getter(|_v, _r| Ok(Value::Bool(false))),
    );
    bc.setters
        .insert("echoMode".to_string(), h::setter(|_v, _r, _val| Ok(())));
    bc.setters
        .insert("lineMode".to_string(), h::setter(|_v, _r, _val| Ok(())));
    bc.setters.insert(
        "echoNewlineMode".to_string(),
        h::setter(|_v, _r, _val| Ok(())),
    );
    bc
}

fn stdout_definition() -> BridgedClassDef {
    let mut bc = BridgedClassDef::new("Stdout");
    bc.native_type = Some("d4rt::io::Stdout".to_string());
    bc.methods.insert(
        "write".to_string(),
        h::method(|_v, recv, args, _n| {
            let s = args.first().map(h::display).unwrap_or_default();
            stdio_write(&recv, s.as_bytes());
            Ok(Value::Null)
        }),
    );
    bc.methods.insert(
        "writeln".to_string(),
        h::method(|_v, recv, args, _n| {
            let s = args.first().map(h::display).unwrap_or_default();
            stdio_write(&recv, format!("{s}\n").as_bytes());
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
            stdio_write(&recv, items.join(&sep).as_bytes());
            Ok(Value::Null)
        }),
    );
    bc.methods.insert(
        "writeCharCode".to_string(),
        h::method(|_v, recv, args, _n| {
            let c = args.first().and_then(h::as_int).unwrap_or(0) as u32;
            if let Some(ch) = char::from_u32(c) {
                stdio_write(&recv, ch.to_string().as_bytes());
            }
            Ok(Value::Null)
        }),
    );
    bc.methods.insert(
        "add".to_string(),
        h::method(|_v, recv, args, _n| {
            if let Some(l) = args.first() {
                stdio_write(&recv, &ints_bytes(l));
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
            let on_data = Value::NativeFn(Rc::new(NativeFunction {
                name: Some("<Stdout.addStream data>".to_string()),
                min_arity: 1,
                max_arity: Some(1),
                is_async: false,
                is_generator: false,
                closure: Box::new(move |_visitor, args, _n| {
                    if let Some(l) = args.first() {
                        stdio_write(&recv_t, &ints_bytes(l));
                    }
                    Ok(Value::Null)
                }),
                callable_runtime_type: FunctionRuntimeType::untyped(),
            }));
            let on_done = Value::NativeFn(Rc::new(NativeFunction {
                name: Some("<Stdout.addStream done>".to_string()),
                min_arity: 0,
                max_arity: Some(0),
                is_async: false,
                is_generator: false,
                closure: Box::new(move |_visitor, _args, _n| {
                    async_state::complete(&out_done, Ok(Value::Null));
                    Ok(Value::Null)
                }),
                callable_runtime_type: FunctionRuntimeType::untyped(),
            }));
            crate::stdlib::stream_lib::stream_subscribe_value(
                visitor,
                &stream,
                on_data,
                Value::Null,
                on_done,
            );
            Ok(wrap_future(out))
        }),
    );
    bc.methods.insert(
        "flush".to_string(),
        h::method(|_v, _r, _a, _n| Ok(future_ok(Value::Null))),
    );
    bc.methods.insert(
        "close".to_string(),
        h::method(|_v, recv, _a, _n| match stdio_done_state(&recv) {
            Some(st) => {
                async_state::complete(&st, Ok(Value::Null));
                Ok(wrap("Future", Value::Future(st)))
            }
            None => Ok(future_ok(Value::Null)),
        }),
    );
    bc.methods.insert(
        "addError".to_string(),
        h::method(|_v, _r, _a, _n| Ok(Value::Null)),
    );
    bc.methods.insert(
        "toString".to_string(),
        h::method(|_v, recv, _a, _n| {
            Ok(Value::str(if stdio_tag(&recv) == "stderr" {
                "Stderr"
            } else {
                "Stdout"
            }))
        }),
    );
    bc.getters.insert(
        "encoding".to_string(),
        h::getter(|_v, _r| Ok(wrap("Utf8Codec", Value::Null))),
    );
    bc.getters.insert(
        "done".to_string(),
        h::getter(|_v, recv| {
            Ok(stdio_done_state(&recv)
                .map(|st| wrap("Future", Value::Future(st)))
                .unwrap_or_else(|| future_ok(Value::Null)))
        }),
    );
    bc.getters.insert(
        "supportsAnsiEscapes".to_string(),
        h::getter(|_v, _r| {
            Ok(Value::Bool(
                std::io::stdout().is_terminal()
                    && std::env::var("TERM").map(|t| t != "dumb").unwrap_or(false),
            ))
        }),
    );
    bc.getters.insert(
        "terminalLines".to_string(),
        h::getter(|_v, _r| {
            Ok(Value::Int(
                std::env::var("LINES")
                    .ok()
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(24),
            ))
        }),
    );
    bc.getters.insert(
        "terminalColumns".to_string(),
        h::getter(|_v, _r| {
            Ok(Value::Int(
                std::env::var("COLUMNS")
                    .ok()
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(80),
            ))
        }),
    );
    bc.getters.insert(
        "hasTerminal".to_string(),
        h::getter(|_v, _r| Ok(Value::Bool(std::io::stdout().is_terminal()))),
    );
    bc.setters
        .insert("encoding".to_string(), h::setter(|_v, _r, _val| Ok(())));
    bc
}

fn stdio_type_definition() -> BridgedClassDef {
    let mut bc = BridgedClassDef::new("StdioType");
    bc.native_type = Some("d4rt::io::StdioType".to_string());
    for (name, value) in [
        ("terminal", "terminal"),
        ("pipe", "pipe"),
        ("file", "file"),
        ("other", "other"),
    ] {
        bc.static_getters.insert(
            name.to_string(),
            h::static_getter(move |_v| Ok(wrap("StdioType", Value::str(value)))),
        );
    }
    bc.methods.insert(
        "toString".to_string(),
        h::method(|_v, recv, _a, _n| {
            let name = h::as_str(&recv).unwrap_or("other");
            Ok(Value::str(format!("StdioType.{name}")))
        }),
    );
    bc
}

// ---------- shared helpers (File/Directory/FileSystemEntity) ----------

fn uri_of(path: &str) -> Value {
    let abs = absolute_path(path);
    let uri = if abs.starts_with('/') {
        format!("file://{abs}")
    } else {
        format!("file:///{abs}")
    };
    h::wrap(&h::class("Uri"), Value::str(uri))
}

fn path_from_uri(v: &Value) -> String {
    let raw = h::as_str(v).unwrap_or("").to_string();
    let s = raw
        .strip_prefix("file://localhost")
        .or_else(|| raw.strip_prefix("file://"))
        .unwrap_or(&raw);
    if s.starts_with('/') {
        s.to_string()
    } else {
        format!("/{s}")
    }
}

fn path_from_raw_bytes(v: &Value) -> String {
    h::as_list(v)
        .map(|l| {
            l.borrow()
                .iter()
                .filter_map(|b| h::as_int(b).map(|i| i as u8))
                .collect::<Vec<u8>>()
        })
        .map(|bytes| String::from_utf8_lossy(&bytes).to_string())
        .unwrap_or_default()
}

fn fs_identical(a: &str, b: &str) -> Result<bool, crate::error::InterpError> {
    crate::permission::require_fs_read(a)?;
    crate::permission::require_fs_read(b)?;
    Ok(match (fs::canonicalize(a), fs::canonicalize(b)) {
        (Ok(pa), Ok(pb)) => pa == pb,
        _ => a == b,
    })
}

fn resolve_links(path: &str) -> Result<String, std::io::Error> {
    crate::permission::require_fs_read(path).map_err(perm_io)?;
    fs::canonicalize(path).map(|p| p.to_string_lossy().to_string())
}

fn metadata_mtime_us(meta: &fs::Metadata) -> i64 {
    meta.modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_micros() as i64)
        .unwrap_or(0)
}

fn mode_string_of(mode: i64) -> String {
    #[cfg(unix)]
    {
        let mut out = String::with_capacity(10);
        out.push(if mode & 0o170000 == 0o040000 {
            'd'
        } else if mode & 0o170000 == 0o120000 {
            'l'
        } else {
            '-'
        });
        for shift in [6, 3, 0] {
            let bits = (mode >> shift) & 0o7;
            out.push(if bits & 0o4 != 0 { 'r' } else { '-' });
            out.push(if bits & 0o2 != 0 { 'w' } else { '-' });
            out.push(if bits & 0o1 != 0 { 'x' } else { '-' });
        }
        out
    }
    #[cfg(not(unix))]
    {
        let _ = mode;
        "----------".to_string()
    }
}

// ---------- FileSystemEvent ----------

pub const FS_CREATE: i32 = 1;
pub const FS_MODIFY: i32 = 2;
pub const FS_DELETE: i32 = 4;
pub const FS_MOVE: i32 = 8;
pub const FS_ALL: i32 = i32::MAX;

fn file_system_event_definition() -> BridgedClassDef {
    let mut bc = BridgedClassDef::new("FileSystemEvent");
    for (name, value) in [
        ("create", FS_CREATE),
        ("modify", FS_MODIFY),
        ("delete", FS_DELETE),
        ("move", FS_MOVE),
        ("all", FS_ALL),
    ] {
        bc.static_getters.insert(
            name.to_string(),
            h::static_getter(move |_v| Ok(Value::Int(value as i64))),
        );
    }
    bc.getters.insert(
        "type".into(),
        h::getter(|_v, t| Ok(h::item_at(&t, 0).unwrap_or(Value::Int(0)))),
    );
    bc.getters.insert(
        "path".into(),
        h::getter(|_v, t| Ok(h::item_at(&t, 1).unwrap_or(Value::Null))),
    );
    bc.getters.insert(
        "isDirectory".into(),
        h::getter(|_v, t| Ok(h::item_at(&t, 2).unwrap_or(Value::Bool(false)))),
    );
    for (name, bit) in [
        ("isCreate", FS_CREATE),
        ("isModify", FS_MODIFY),
        ("isDelete", FS_DELETE),
        ("isMove", FS_MOVE),
    ] {
        let bit = bit as i64;
        bc.getters.insert(
            name.to_string(),
            h::getter(move |_v, t| {
                let ty = h::item_at(&t, 0).and_then(|x| h::as_int(&x)).unwrap_or(0);
                Ok(Value::Bool(ty == bit))
            }),
        );
    }
    bc.methods.insert(
        "toString".into(),
        h::method(|_v, t, _a, _n| {
            let path = h::display(&h::item_at(&t, 1).unwrap_or(Value::Null));
            Ok(Value::str(format!("FileSystemEvent: {path}")))
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

fn fs_event_value(path: &str, kind: i32, is_dir: bool) -> Value {
    wrap(
        "FileSystemEvent",
        Value::List(Rc::new(RefCell::new(vec![
            Value::Int(kind as i64),
            Value::str(path),
            Value::Bool(is_dir),
        ]))),
    )
}

// ---------- FileSystemEntity.watch (polling watcher) ----------

/// Event delivered by the watcher thread.
pub struct WatchEvent {
    pub path: String,
    pub kind: i32,
    pub is_dir: bool,
}

static WATCHERS: LazyLock<Mutex<HashMap<i64, mpsc::Sender<WatchEvent>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));
static NEXT_WATCH_ID: AtomicI64 = AtomicI64::new(1);
/// watcher id -> context id that created it.
static OWNED_WATCHERS: LazyLock<Mutex<HashMap<i64, i64>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// Stops every file watcher owned by `ctx_id` (the watcher threads exit as
/// soon as their entry disappears).
pub(crate) fn cleanup_watchers(ctx_id: i64) {
    let owned: Vec<i64> = OWNED_WATCHERS
        .lock()
        .unwrap()
        .iter()
        .filter(|(_, c)| **c == ctx_id)
        .map(|(id, _)| *id)
        .collect();
    for id in owned {
        OWNED_WATCHERS.lock().unwrap().remove(&id);
        WATCHERS.lock().unwrap().remove(&id);
    }
}

fn snapshot(path: &str, recursive: bool) -> HashMap<String, (i64, i64, bool)> {
    fn walk(dir: &Path, recursive: bool, out: &mut HashMap<String, (i64, i64, bool)>) {
        let Ok(entries) = fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let p = entry.path();
            let s = p.to_string_lossy().to_string();
            if let Ok(meta) = entry.metadata() {
                out.insert(
                    s,
                    (metadata_mtime_us(&meta), meta.len() as i64, meta.is_dir()),
                );
                if recursive && meta.is_dir() {
                    walk(&p, true, out);
                }
            }
        }
    }
    let mut m = HashMap::new();
    let p = Path::new(path);
    if let Ok(meta) = fs::metadata(p) {
        if meta.is_dir() {
            walk(p, recursive, &mut m);
        } else {
            m.insert(
                path.to_string(),
                (metadata_mtime_us(&meta), meta.len() as i64, false),
            );
        }
    }
    m
}

/// Starts a polling watcher on `path`. Returns `(id, receiver)`; the
/// poll task drains the receiver into the returned stream controller.
fn watch_start(path: &str, recursive: bool, mask: i32) -> (i64, mpsc::Receiver<WatchEvent>) {
    let id = NEXT_WATCH_ID.fetch_add(1, Ordering::Relaxed);
    let (tx, rx) = mpsc::channel();
    WATCHERS.lock().unwrap().insert(id, tx);
    OWNED_WATCHERS
        .lock()
        .unwrap()
        .insert(id, crate::state::current().id);
    let path = path.to_string();
    std::thread::spawn(move || {
        let mut snap = snapshot(&path, recursive);
        loop {
            std::thread::sleep(std::time::Duration::from_millis(100));
            if !WATCHERS.lock().unwrap().contains_key(&id) {
                return;
            }
            let snap2 = snapshot(&path, recursive);
            let mut events: Vec<WatchEvent> = Vec::new();
            for (p, (_, _, is_dir)) in &snap {
                if !snap2.contains_key(p) {
                    events.push(WatchEvent {
                        path: p.clone(),
                        kind: FS_DELETE,
                        is_dir: *is_dir,
                    });
                }
            }
            for (p, (m, l, is_dir)) in &snap2 {
                match snap.get(p) {
                    None => events.push(WatchEvent {
                        path: p.clone(),
                        kind: FS_CREATE,
                        is_dir: *is_dir,
                    }),
                    Some((om, ol, _)) => {
                        if m != om || l != ol {
                            events.push(WatchEvent {
                                path: p.clone(),
                                kind: FS_MODIFY,
                                is_dir: *is_dir,
                            });
                        }
                    }
                }
            }
            snap = snap2;
            for ev in events {
                if (ev.kind & mask) == 0 {
                    continue;
                }
                if WATCHERS.lock().unwrap().get(&id).is_none() {
                    return;
                }
                let tx = WATCHERS.lock().unwrap().get(&id).cloned();
                let Some(tx) = tx else { return };
                if tx.send(ev).is_err() {
                    WATCHERS.lock().unwrap().remove(&id);
                    return;
                }
            }
        }
    });
    (id, rx)
}

/// Poll arm for `Task::WatchPoll`; returns whether the task should stay alive.
pub fn poll_watch(
    visitor: &mut InterpreterVisitor,
    id: i64,
    rx: &mpsc::Receiver<WatchEvent>,
    ctrl: &Value,
) -> bool {
    loop {
        match rx.try_recv() {
            Ok(ev) => {
                crate::stdlib::stream_lib::controller_add_value(
                    visitor,
                    ctrl,
                    fs_event_value(&ev.path, ev.kind, ev.is_dir),
                );
            }
            Err(mpsc::TryRecvError::Empty) => break,
            Err(mpsc::TryRecvError::Disconnected) => {
                WATCHERS.lock().unwrap().remove(&id);
                return false;
            }
        }
    }
    true
}

/// Shared implementation of `watch(events, {recursive})` for File,
/// Directory and FileSystemEntity. Returns a stream of FileSystemEvent.
fn watch_impl(
    _visitor: &mut InterpreterVisitor,
    path: &str,
    named: &HashMap<String, Value>,
) -> Result<Value, InterpError> {
    crate::permission::require_fs_read(path)?;
    let recursive = named.get("recursive").and_then(h::as_bool).unwrap_or(false);
    let mask = named
        .get("events")
        .and_then(h::as_int)
        .unwrap_or(FS_ALL as i64) as i32;
    let (id, rx) = watch_start(path, recursive, mask);
    let ctrl = crate::stdlib::stream_lib::make_controller_value(false, false);
    let stream = crate::stdlib::stream_lib::controller_stream_value(&ctrl);
    let ctrl_c = ctrl.clone();
    let on_cancel = Value::NativeFn(Rc::new(NativeFunction {
        name: Some("<watch cancel>".to_string()),
        min_arity: 0,
        max_arity: Some(0),
        is_async: false,
        is_generator: false,
        closure: Box::new(move |_visitor, _args, _n| {
            WATCHERS.lock().unwrap().remove(&id);
            let _ = &ctrl_c;
            Ok(Value::Null)
        }),
        callable_runtime_type: crate::rt::FunctionRuntimeType::untyped(),
    }));
    crate::stdlib::stream_lib::controller_set_on_cancel(&ctrl, on_cancel);
    async_state::enqueue_task(async_state::Task::WatchPoll { id, rx, ctrl });
    Ok(stream)
}

// ---------- FileLock ----------

fn file_lock_definition() -> BridgedClassDef {
    let mut bc = BridgedClassDef::new("FileLock");
    for name in ["shared", "exclusive", "blockingShared", "blockingExclusive"] {
        let name_s = name.to_string();
        bc.static_getters.insert(
            name.to_string(),
            h::static_getter(move |_v| Ok(wrap("FileLock", Value::str(&name_s)))),
        );
    }
    bc.getters.insert(
        "name".into(),
        h::getter(|_v, recv| Ok(Value::str(h::as_str(&recv).unwrap_or_default()))),
    );
    bc.methods.insert(
        "toString".into(),
        h::method(|_v, recv, _a, _n| {
            Ok(Value::str(format!(
                "FileLock.{}",
                h::as_str(&recv).unwrap_or_default()
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

// ---------- Path exception subclasses ----------

fn path_exception_definition(name: &str, default_message: &str) -> BridgedClassDef {
    let name = name.to_string();
    let default_message = default_message.to_string();
    let name_c = name.clone();
    let mut bc = BridgedClassDef::new(&name);
    bc.constructors.insert(
        "".to_string(),
        h::ctor(move |_v, args, _n| {
            let path = args
                .first()
                .map(|v| h::as_str(v).unwrap_or_default().to_string())
                .unwrap_or_default();
            let message = args
                .get(1)
                .map(|v| h::as_str(v).unwrap_or_default().to_string())
                .unwrap_or_else(|| default_message.clone());
            Ok(wrap(
                &name_c,
                Value::List(Rc::new(RefCell::new(vec![
                    Value::str(message),
                    Value::str(path),
                    Value::Null,
                ]))),
            ))
        }),
    );
    bc.getters.insert(
        "message".into(),
        h::getter(|_v, t| Ok(h::item_at(&t, 0).unwrap_or(Value::Null))),
    );
    bc.getters.insert(
        "path".into(),
        h::getter(|_v, t| Ok(h::item_at(&t, 1).unwrap_or(Value::Null))),
    );
    bc.getters.insert(
        "osError".into(),
        h::getter(|_v, t| Ok(h::item_at(&t, 2).unwrap_or(Value::Null))),
    );
    let name_d = name.clone();
    bc.methods.insert(
        "toString".into(),
        h::method(move |_v, t, _a, _n| {
            let message = h::display(&h::item_at(&t, 0).unwrap_or(Value::Null));
            let path = h::display(&h::item_at(&t, 1).unwrap_or(Value::Null));
            Ok(Value::str(format!("{name_d}: {message}, path = '{path}'")))
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

// ---------- file-backed IOSink (File.openWrite) ----------

/// Native: `[Str(path), Str(mode), List(chunks), Future(done), Bool(written)]`.
fn file_sink_native(path: &str, mode: &str) -> Value {
    Value::List(Rc::new(RefCell::new(vec![
        Value::str(path),
        Value::str(mode),
        Value::List(Rc::new(RefCell::new(Vec::new()))),
        wrap_future(Rc::new(RefCell::new(async_state::FutureState::new()))),
        Value::Bool(false),
    ])))
}

pub fn file_sink_write(recv: &Value, bytes: &[u8]) {
    if let Some(l) = h::as_list(recv)
        && l.borrow().len() > 2
        && let Some(chunks) = h::as_list(&l.borrow()[2].clone())
    {
        chunks.borrow_mut().push(Value::List(Rc::new(RefCell::new(
            bytes.iter().map(|b| Value::Int(*b as i64)).collect(),
        ))));
    }
}

fn file_sink_path(recv: &Value) -> String {
    h::as_list(recv)
        .and_then(|l| l.borrow().first().cloned())
        .and_then(|p| h::as_str(&p).map(|s| s.to_string()))
        .unwrap_or_default()
}

fn file_sink_mode(recv: &Value) -> String {
    h::as_list(recv)
        .and_then(|l| l.borrow().get(1).cloned())
        .and_then(|m| h::as_str(&m).map(|s| s.to_string()))
        .unwrap_or_else(|| "w".to_string())
}

pub fn file_sink_flush(recv: &Value) -> Result<(), std::io::Error> {
    let path = file_sink_path(recv);
    crate::permission::require_fs_write(&path).map_err(perm_io)?;
    let mode = file_sink_mode(recv);
    let mut all: Vec<u8> = Vec::new();
    if let Some(l) = h::as_list(recv)
        && l.borrow().len() > 2
        && let Some(chunks) = h::as_list(&l.borrow()[2].clone())
    {
        for c in chunks.borrow().iter() {
            if let Some(cl) = h::as_list(c) {
                for b in cl.borrow().iter() {
                    if let Some(i) = h::as_int(b) {
                        all.push(i as u8);
                    }
                }
            }
        }
    }
    let has_written = h::as_list(recv)
        .map(|l| l.borrow().len() > 4 && h::as_bool(&l.borrow()[4].clone()).unwrap_or(false))
        .unwrap_or(false);
    if !all.is_empty() {
        if has_written || mode.starts_with('a') {
            use std::io::Write;
            let mut f = fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&path)?;
            f.write_all(&all)?;
        } else {
            write_with_mode(&path, &all, &mode)?;
        }
    }
    if let Some(l) = h::as_list(recv) {
        let mut b = l.borrow_mut();
        if b.len() > 4 {
            b[4] = Value::Bool(true);
        }
    }
    if let Some(l) = h::as_list(recv)
        && l.borrow().len() > 2
        && let Some(chunks) = h::as_list(&l.borrow()[2].clone())
    {
        chunks.borrow_mut().clear();
    }
    Ok(())
}

pub fn file_sink_done_state(recv: &Value) -> Option<Rc<RefCell<async_state::FutureState>>> {
    h::as_list(recv)
        .and_then(|l| l.borrow().get(3).cloned())
        .and_then(|f| match f {
            Value::Future(st) => Some(st),
            _ => None,
        })
}

/// True when `recv` is a file-backed IOSink (vs. the process-backed
/// `Value::Int(pid)` form or a pipe-backed sink).
pub fn is_file_sink(recv: &Value) -> bool {
    matches!(recv, Value::List(_)) && !is_pipe_sink(recv)
}

/// True when `recv` is a pipe-backed IOSink (Pipe.write).
pub fn is_pipe_sink(recv: &Value) -> bool {
    h::as_list(recv)
        .and_then(|l| l.borrow().first().cloned())
        .and_then(|v| h::as_str(&v).map(|s| s == PIPE_SINK_MARKER))
        .unwrap_or(false)
}

/// Marker used to identify pipe-backed IOSink values.
pub const PIPE_SINK_MARKER: &str = "__d4rt_pipe__";

// ---------- Pipe ----------

/// Pipe write handles: id -> (bytes channel, close channel).
static PIPE_WRITES: LazyLock<Mutex<HashMap<i64, PipeWrites>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));
/// pipe id -> context id that created it.
static OWNED_PIPES: LazyLock<Mutex<HashMap<i64, i64>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// Closes every pipe owned by `ctx_id` (dropping the senders makes the
/// writer threads shut the OS pipe down and exit).
pub(crate) fn cleanup_pipes(ctx_id: i64) {
    let owned: Vec<i64> = OWNED_PIPES
        .lock()
        .unwrap()
        .iter()
        .filter(|(_, c)| **c == ctx_id)
        .map(|(id, _)| *id)
        .collect();
    for id in owned {
        OWNED_PIPES.lock().unwrap().remove(&id);
        PIPE_WRITES.lock().unwrap().remove(&id);
    }
}

/// Writes into a pipe-backed IOSink.
pub fn is_pipe_id(id: i64) -> bool {
    PIPE_WRITES.lock().unwrap().contains_key(&id)
}

pub fn pipe_sink_write(recv: &Value, bytes: &[u8]) {
    if let Some(id) = h::as_list(recv)
        .and_then(|l| l.borrow().get(1).cloned())
        .and_then(|i| h::as_int(&i))
        && let Some((tx, _)) = PIPE_WRITES.lock().unwrap().get(&id)
    {
        let _ = tx.send(bytes.to_vec());
    }
}

/// The done future of a pipe-backed IOSink (completes when the pipe closes).
pub fn pipe_sink_done_state(recv: &Value) -> Option<Rc<RefCell<async_state::FutureState>>> {
    h::as_list(recv)
        .and_then(|l| l.borrow().get(2).cloned())
        .and_then(|f| match f {
            Value::Future(st) => Some(st),
            _ => None,
        })
}

/// Closes the write end of a pipe-backed IOSink.
pub fn pipe_sink_close(recv: &Value) {
    if let Some(id) = h::as_list(recv)
        .and_then(|l| l.borrow().get(1).cloned())
        .and_then(|i| h::as_int(&i))
        && let Some((_, ctrl)) = PIPE_WRITES.lock().unwrap().get(&id)
    {
        let _ = ctrl.send(());
    }
}

/// The shared Dart value of a pipe sink given its id.
fn pipe_sink_value(id: i64, done: &Rc<RefCell<async_state::FutureState>>) -> Value {
    wrap(
        "IOSink",
        Value::List(Rc::new(RefCell::new(vec![
            Value::str(PIPE_SINK_MARKER),
            Value::Int(id),
            wrap_future(done.clone()),
        ]))),
    )
}

fn new_pipe_id() -> i64 {
    static NEXT_PIPE_ID: AtomicI64 = AtomicI64::new(1);
    NEXT_PIPE_ID.fetch_add(1, Ordering::SeqCst)
}

fn pipe_value() -> Value {
    let (read_end, write_end) = std::os::unix::net::UnixStream::pair().expect("UnixStream::pair");
    let id = new_pipe_id();
    let (data_tx, data_rx) = mpsc::channel();
    let ctrl = crate::stdlib::stream_lib::make_controller_value(false, false);
    let stream = crate::stdlib::stream_lib::controller_stream_value(&ctrl);
    let done = Rc::new(RefCell::new(async_state::FutureState::new()));
    let (wtx, wrx) = mpsc::channel::<Vec<u8>>();
    let (ctx, crx) = mpsc::channel::<()>();
    PIPE_WRITES.lock().unwrap().insert(id, (wtx, ctx));
    OWNED_PIPES
        .lock()
        .unwrap()
        .insert(id, crate::state::current().id);
    let mut w = write_end;
    std::thread::spawn(move || {
        loop {
            match wrx.try_recv() {
                Ok(bytes) => {
                    let _ = w.write_all(&bytes);
                    continue;
                }
                Err(mpsc::TryRecvError::Empty) => {}
                Err(mpsc::TryRecvError::Disconnected) => {
                    let _ = w.shutdown(std::net::Shutdown::Both);
                    break;
                }
            }
            if crx.try_recv().is_ok() {
                while let Ok(bytes) = wrx.try_recv() {
                    let _ = w.write_all(&bytes);
                }
                let _ = w.shutdown(std::net::Shutdown::Both);
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
    });
    let mut r = read_end;
    let data_tx_w = data_tx.clone();
    std::thread::spawn(move || {
        let mut buf = [0u8; 8192];
        loop {
            match r.read(&mut buf) {
                Ok(0) => {
                    let _ = data_tx_w.send(crate::stdlib::http::SocketData::Eof);
                    break;
                }
                Ok(n) => {
                    if data_tx_w
                        .send(crate::stdlib::http::SocketData::Read(buf[..n].to_vec()))
                        .is_err()
                    {
                        break;
                    }
                }
                Err(_) => {
                    let _ = data_tx_w.send(crate::stdlib::http::SocketData::Eof);
                    break;
                }
            }
        }
    });
    async_state::enqueue_task(async_state::Task::SocketPoll {
        id,
        rx: data_rx,
        ctrl: ctrl.clone(),
        done: done.clone(),
    });
    wrap(
        "Pipe",
        Value::List(Rc::new(RefCell::new(vec![
            stream,
            pipe_sink_value(id, &done),
            wrap_future(done.clone()),
        ]))),
    )
}

fn pipe_definition() -> BridgedClassDef {
    let mut bc = BridgedClassDef::new("Pipe");
    bc.static_methods.insert(
        "createSync".to_string(),
        h::static_method(|_v, _args, _named| Ok(pipe_value())),
    );
    bc.static_methods.insert(
        "create".to_string(),
        h::static_method(|_v, _args, _named| Ok(wrap_future_completed(pipe_value()))),
    );
    bc.getters.insert(
        "read".to_string(),
        h::getter(|_v, recv| Ok(list_get(&recv, 0).unwrap_or(Value::Null))),
    );
    bc.getters.insert(
        "write".to_string(),
        h::getter(|_v, recv| Ok(list_get(&recv, 1).unwrap_or(Value::Null))),
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

fn wrap_future_completed(v: Value) -> Value {
    let state = Rc::new(RefCell::new(async_state::FutureState::new()));
    async_state::complete(&state, Ok(v));
    wrap_future(state)
}

fn list_get(v: &Value, index: usize) -> Option<Value> {
    h::as_list(v).and_then(|l| l.borrow().get(index).cloned())
}
