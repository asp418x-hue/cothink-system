//! Permission-based sandboxing, modeled after `d4rt`'s permission system
//! and Deno's `--allow-*` flags.
//!
//! By default a [`Context`](crate::Context) denies every dangerous
//! operation (filesystem, network, process, isolate, environment). The
//! host grants specific capabilities through [`crate::Context::grant`],
//! checks them with [`crate::Context::has_permission`] and withdraws them
//! with [`crate::Context::revoke`]. Interpreted code can `catch` the
//! resulting `InterpError` like any other Dart error.
//!
//! Matching rules:
//! - `FilesystemPermission::Any` allows everything; `Read` allows any
//!   read (and metadata) access; `Write` any write access;
//!   `ReadPath(p)`/`WritePath(p)`/`Path(p)` allow access only under the
//!   given directory prefix.
//! - `NetworkPermission::Any` allows all network operations;
//!   `Connect(host)` allows connections to that host (with an optional
//!   `:port` suffix when the port must also match).
//! - `ProcessPermission::Any` allows running anything;
//!   `Command(cmd)` allows only that executable (exact path or bare
//!   name).
//! - `Permission::Isolate` allows isolate creation and port
//!   communication; `Permission::Env` allows reading `Platform.environment`.

use crate::error::InterpError;
use std::cell::RefCell;

/// Filesystem capabilities (d4rt's `FilesystemPermission`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FilesystemPermission {
    /// All filesystem operations.
    Any,
    /// Read-only operations (including stat/list/exists/watch).
    Read,
    /// Write operations.
    Write,
    /// Read operations under a specific directory prefix
    /// (d4rt's `readPath`).
    ReadPath(String),
    /// Write operations under a specific directory prefix.
    WritePath(String),
    /// Read and write operations under a specific directory prefix
    /// (d4rt's `path`).
    Path(String),
}

/// Network capabilities (d4rt's `NetworkPermission`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NetworkPermission {
    /// All network operations.
    Any,
    /// Connect/bind to a specific host, optionally `host:port`.
    Connect(String),
}

/// Process execution capabilities (d4rt's `ProcessRunPermission`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProcessPermission {
    /// Run any command.
    Any,
    /// Run only this executable (path or bare name).
    Command(String),
}

/// One grantable capability.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Permission {
    Filesystem(FilesystemPermission),
    Network(NetworkPermission),
    Process(ProcessPermission),
    /// Isolate creation, `Isolate.run`/`spawn` and port communication
    /// (d4rt's `IsolatePermission`).
    Isolate,
    /// `Platform.environment` reads (Deno's `--allow-env`).
    Env,
    /// Loading `import`/`export` modules from the filesystem. Not granted
    /// by default: `import './x.dart'` then resolves only from the
    /// virtual module registry (`Context::register_dart_module`) and
    /// `dart:`/`package:` URIs, so interpreted code can never read source
    /// files off the disk. Granting this does *not* bypass
    /// [`FilesystemPermission`]: the path is still checked with
    /// `require_fs_read`.
    ModuleFiles,
}

impl Permission {
    fn describe(&self) -> String {
        match self {
            Permission::Filesystem(f) => format!("filesystem {f:?}"),
            Permission::Network(n) => format!("network {n:?}"),
            Permission::Process(p) => format!("process {p:?}"),
            Permission::Isolate => "isolate".to_string(),
            Permission::Env => "environment".to_string(),
            Permission::ModuleFiles => "filesystem module loading".to_string(),
        }
    }
}

/// The set of permissions granted to one `Context`.
#[derive(Default)]
pub struct PermissionSet {
    pub(crate) granted: RefCell<Vec<Permission>>,
}

impl PermissionSet {
    pub fn new() -> Self {
        PermissionSet::default()
    }

    /// Adds a permission. Granting a weaker permission does not remove a
    /// stronger one already granted.
    pub fn grant(&self, permission: Permission) {
        let mut g = self.granted.borrow_mut();
        if !g.iter().any(|p| p == &permission) {
            g.push(permission);
        }
    }

    /// Removes exactly this permission. A broader grant (`Any`) keeps
    /// covering the capability afterwards, matching d4rt semantics.
    pub fn revoke(&self, permission: &Permission) {
        self.granted.borrow_mut().retain(|p| p != permission);
    }

    /// Whether the granted set covers the requested permission.
    pub fn allows(&self, requested: &Permission) -> bool {
        self.granted.borrow().iter().any(|g| covers(g, requested))
    }

    /// Denies `requested` by throwing an `InterpError` describing the
    /// capability and the hint to grant it.
    pub fn require(&self, requested: &Permission) -> Result<(), InterpError> {
        if self.allows(requested) {
            Ok(())
        } else {
            Err(InterpError::runtime(format!(
                "Permission denied: {} access is not granted to this context \
                 (use Context::grant to allow it)",
                requested.describe()
            )))
        }
    }
}

fn covers(granted: &Permission, requested: &Permission) -> bool {
    match (granted, requested) {
        (Permission::Filesystem(g), Permission::Filesystem(r)) => fs_covers(g, r),
        (Permission::Network(g), Permission::Network(r)) => net_covers(g, r),
        (Permission::Process(g), Permission::Process(r)) => proc_covers(g, r),
        (Permission::Isolate, Permission::Isolate) => true,
        (Permission::Env, Permission::Env) => true,
        (Permission::ModuleFiles, Permission::ModuleFiles) => true,
        _ => false,
    }
}

fn fs_covers(granted: &FilesystemPermission, requested: &FilesystemPermission) -> bool {
    match (granted, requested) {
        (FilesystemPermission::Any, _) => true,
        (FilesystemPermission::Read, FilesystemPermission::Read) => true,
        (FilesystemPermission::Read, FilesystemPermission::ReadPath(_)) => true,
        (FilesystemPermission::Write, FilesystemPermission::Write) => true,
        (FilesystemPermission::Write, FilesystemPermission::WritePath(_)) => true,
        (FilesystemPermission::ReadPath(g), FilesystemPermission::ReadPath(r)) => path_covers(g, r),
        (FilesystemPermission::WritePath(g), FilesystemPermission::WritePath(r)) => {
            path_covers(g, r)
        }
        (FilesystemPermission::Path(g), FilesystemPermission::ReadPath(r))
        | (FilesystemPermission::Path(g), FilesystemPermission::WritePath(r))
        | (FilesystemPermission::Path(g), FilesystemPermission::Path(r)) => path_covers(g, r),
        _ => false,
    }
}

fn path_covers(granted_prefix: &str, requested_path: &str) -> bool {
    let g = granted_prefix.trim_end_matches('/');
    requested_path == g || requested_path.starts_with(&format!("{g}/"))
}

fn net_covers(granted: &NetworkPermission, requested: &NetworkPermission) -> bool {
    match (granted, requested) {
        (NetworkPermission::Any, _) => true,
        (NetworkPermission::Connect(g), NetworkPermission::Connect(r)) => {
            let (gh, gp) = split_host_port(g);
            let (rh, rp) = split_host_port(r);
            match gp {
                Some(gp) => gh == rh && rp.is_none_or(|rp| rp == gp),
                None => gh == rh,
            }
        }
        (NetworkPermission::Connect(_), NetworkPermission::Any) => false,
    }
}

fn split_host_port(spec: &str) -> (&str, Option<u16>) {
    match spec.rsplit_once(':') {
        Some((host, port)) => match port.parse::<u16>() {
            Ok(p) => (host, Some(p)),
            Err(_) => (spec, None),
        },
        None => (spec, None),
    }
}

fn proc_covers(granted: &ProcessPermission, requested: &ProcessPermission) -> bool {
    match (granted, requested) {
        (ProcessPermission::Any, _) => true,
        (ProcessPermission::Command(g), ProcessPermission::Command(r)) => {
            r == g || r.ends_with(&format!("/{g}"))
        }
        (ProcessPermission::Command(_), ProcessPermission::Any) => false,
    }
}

/// Requests the current `Context`'s permission set.
///
/// Panics if called outside a `Context` scope (same contract as
/// [`crate::state::current`]).
pub fn current() -> PermissionSet {
    let state = crate::state::current();
    PermissionSet {
        granted: state.permissions.granted.clone(),
    }
}

/// Runs `f` against the permission set of the currently executing
/// `Context`. The borrow ends before `f`'s result is returned, so errors
/// can be propagated freely.
pub fn with_current_permissions<T>(
    f: impl FnOnce(&PermissionSet) -> Result<T, InterpError>,
) -> Result<T, InterpError> {
    let state = crate::state::current();
    f(&state.permissions)
}

/// Permission check for a filesystem read/metadata operation.
pub fn require_fs_read(path: &str) -> Result<(), InterpError> {
    with_current_permissions(|set| {
        set.require(&Permission::Filesystem(FilesystemPermission::ReadPath(
            path.to_string(),
        )))
    })
}

/// Permission check for a filesystem write/mutation operation.
pub fn require_fs_write(path: &str) -> Result<(), InterpError> {
    with_current_permissions(|set| {
        set.require(&Permission::Filesystem(FilesystemPermission::WritePath(
            path.to_string(),
        )))
    })
}

/// Permission check for a network connect/bind operation.
pub fn require_network(host: &str, port: u16) -> Result<(), InterpError> {
    with_current_permissions(|set| {
        set.require(&Permission::Network(NetworkPermission::Connect(format!(
            "{host}:{port}"
        ))))
    })
}

/// Permission check for network metadata (e.g. `NetworkInterface.list`).
pub fn require_network_any() -> Result<(), InterpError> {
    with_current_permissions(|set| set.require(&Permission::Network(NetworkPermission::Any)))
}

/// Permission check for running a child process.
pub fn require_process(executable: &str) -> Result<(), InterpError> {
    with_current_permissions(|set| {
        set.require(&Permission::Process(ProcessPermission::Command(
            executable.to_string(),
        )))
    })
}

/// Permission check for isolate creation/communication.
pub fn require_isolate() -> Result<(), InterpError> {
    with_current_permissions(|set| set.require(&Permission::Isolate))
}

/// Permission check for `Platform.environment` reads.
pub fn require_env() -> Result<(), InterpError> {
    with_current_permissions(|set| set.require(&Permission::Env))
}

/// Permission check for loading `import`/`export` modules from the
/// filesystem (the `Permission::ModuleFiles` flag).
pub fn require_module_files() -> Result<(), InterpError> {
    with_current_permissions(|set| set.require(&Permission::ModuleFiles))
}
