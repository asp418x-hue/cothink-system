pub mod core;
pub mod helpers;

use crate::class::BridgedClassDef;
use crate::env::Environment;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::{Arc, LazyLock, Mutex, OnceLock};

/// Externally registered package libraries (`package:...` URIs) mapped to
/// the bridged classes they expose, mirroring
/// `D4rt.registerBridgedClass(bridge, libraryUri)` from `~/d4rt`.
///
/// This is host-level configuration (class *definitions* only): the
/// environments built from them are per-`Context` via
/// [`package_library_environment`]. Process-global (not per-thread) so the
/// host can register from any thread and every `Context` sees the classes.
static PACKAGE_LIBRARIES: LazyLock<Mutex<HashMap<String, Vec<Arc<BridgedClassDef>>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// Absolute path of the main script, if the host set it (used by
/// `Platform.script`).
static MAIN_SCRIPT_PATH: OnceLock<String> = OnceLock::new();

/// Sets the absolute path of the main script being executed. Consumed by
/// `Platform.script` in `dart:io`.
pub fn set_main_script_path(path: &str) {
    let _ = MAIN_SCRIPT_PATH.set(path.to_string());
}

/// Returns the main script path registered via [`set_main_script_path`].
pub fn main_script_path() -> Option<String> {
    MAIN_SCRIPT_PATH.get().cloned()
}

/// Registers a package library URI so `import 'package:...'` resolves to
/// the given bridged classes.
pub fn register_package_library(uri: &str, classes: Vec<Arc<BridgedClassDef>>) {
    let mut reg = PACKAGE_LIBRARIES.lock().unwrap();
    let entry = reg.entry(uri.to_string()).or_default();
    for c in classes {
        if !entry.iter().any(|e| e.name == c.name) {
            entry.push(c);
        }
    }
}

/// Registers a module library under an arbitrary URI — including bare
/// names like `cheerio` — so `import 'cheerio';` resolves to the given
/// bridged classes implemented in Rust. Process-global and keyed by exact
/// URI, like [`register_package_library`]. `dart:` URIs and Rust module
/// libraries win over virtual Dart-source modules
/// ([`Context::register_dart_module`](crate::Context::register_dart_module)).
///
/// For a registration that only affects one sandbox, use
/// [`Context::register_module_library`](crate::Context::register_module_library)
/// instead — per-`Context` registrations shadow this global one.
pub fn register_module_library(uri: &str, classes: Vec<Arc<BridgedClassDef>>) {
    register_package_library(uri, classes);
}

/// Returns the environment for a registered module library URI (bare name
/// or `package:`), if any. Per-`Context` registrations made via
/// [`Context::register_module_library`](crate::Context::register_module_library)
/// take precedence over the process-global registry
/// ([`register_module_library`]).
pub fn module_library_environment(uri: &str) -> Option<Rc<RefCell<Environment>>> {
    let classes = crate::state::current()
        .module_libraries
        .borrow()
        .get(uri)
        .cloned()
        .or_else(|| PACKAGE_LIBRARIES.lock().unwrap().get(uri).cloned())?;
    let env = Environment::new();
    for bc in classes {
        crate::log::debug(&format!(
            "Registering module class '{}' from '{uri}'",
            bc.name
        ));
        Environment::define_bridge(&env, bc.clone());
    }
    Some(env)
}

/// Process-global registry of Dart-source modules: URI (as written in
/// `import`, e.g. `./mylib.dart`) → source, like the per-`Context`
/// [`Context::register_dart_module`](crate::Context::register_dart_module)
/// but visible to every `Context` in the process.
static GLOBAL_DART_MODULES: LazyLock<Mutex<HashMap<String, String>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// Registers a Dart source file under `name` for every `Context` in the
/// process, making it importable with `import 'name';` — e.g.
/// `./mylib.dart` — without any filesystem access. Mirrors
/// [`Context::register_dart_module`](crate::Context::register_dart_module),
/// which registers for a single sandbox and shadows this global entry
/// for the same `name`. Registering the same `name` again replaces the
/// previous source.
pub fn register_dart_module(name: &str, source: &str) {
    GLOBAL_DART_MODULES
        .lock()
        .unwrap()
        .insert(name.to_string(), source.to_string());
}

/// Returns the source of a process-globally registered Dart module, if any.
pub fn dart_module_source(name: &str) -> Option<String> {
    GLOBAL_DART_MODULES.lock().unwrap().get(name).cloned()
}

/// Returns the environment for a registered package library URI, if any.
pub fn package_library_environment(uri: &str) -> Option<Rc<RefCell<Environment>>> {
    module_library_environment(uri)
}

/// Loads (or returns the cached) environment for a `dart:*` library URI,
/// scoped to the current `Context` (see [`crate::state`]).
pub fn load_library(uri: &str) -> Option<Rc<RefCell<Environment>>> {
    let state = crate::state::current();
    if let Some(env) = state.libraries.borrow().get(uri).cloned() {
        return Some(env);
    }
    let env = match uri {
        "dart:core" => {
            let env = Environment::new();
            core::load_core(&env);
            env
        }
        "dart:math" => {
            #[cfg(feature = "math")]
            {
                let env = Environment::new();
                math::load(&env);
                env
            }
            #[cfg(not(feature = "math"))]
            {
                return None;
            }
        }
        "dart:convert" => {
            #[cfg(feature = "convert")]
            {
                let env = Environment::new();
                convert::load(&env);
                env
            }
            #[cfg(not(feature = "convert"))]
            {
                return None;
            }
        }
        "dart:collection" => {
            #[cfg(feature = "collection")]
            {
                let env = Environment::new();
                collection::load(&env);
                env
            }
            #[cfg(not(feature = "collection"))]
            {
                return None;
            }
        }
        "dart:typed_data" => {
            #[cfg(feature = "typed_data")]
            {
                let env = Environment::new();
                typed_data::load(&env);
                env
            }
            #[cfg(not(feature = "typed_data"))]
            {
                return None;
            }
        }
        "dart:async" => {
            #[cfg(feature = "async")]
            {
                let env = Environment::new();
                async_lib::load(&env);
                stream_lib::load(&env);
                env
            }
            #[cfg(not(feature = "async"))]
            {
                return None;
            }
        }
        "dart:io" => {
            #[cfg(feature = "io")]
            {
                let env = Environment::new();
                io::load(&env);
                process::load(&env);
                http::load(&env);
                // dart:io re-exports dart:convert top-levels (utf8, ascii, ...).
                convert::load(&env);
                env
            }
            #[cfg(not(feature = "io"))]
            {
                return None;
            }
        }
        "dart:isolate" => {
            #[cfg(feature = "isolate")]
            {
                let env = Environment::new();
                isolate::load(&env);
                env
            }
            #[cfg(not(feature = "isolate"))]
            {
                return None;
            }
        }
        _ => return None,
    };
    state
        .libraries
        .borrow_mut()
        .insert(uri.to_string(), env.clone());
    Some(env)
}

pub mod async_lib;
#[cfg(feature = "collection")]
pub mod collection;
#[cfg(feature = "convert")]
pub mod convert;
#[cfg(feature = "io")]
pub mod http;
#[cfg(feature = "io")]
pub mod io;
#[cfg(feature = "isolate")]
pub mod isolate;
#[cfg(feature = "math")]
pub mod math;
#[cfg(feature = "io")]
pub mod process;
pub mod stream_lib;
#[cfg(feature = "typed_data")]
pub mod typed_data;
