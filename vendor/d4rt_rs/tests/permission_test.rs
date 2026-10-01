//! Permission-gating tests: by default every dangerous dart:io /
//! dart:isolate operation is denied, and `Context::grant` / `revoke` /
//! `has_permission` control what is allowed.

use d4rt_rs::Context;
use d4rt_rs::permission::{FilesystemPermission, NetworkPermission, Permission, ProcessPermission};

use std::sync::atomic::{AtomicU64, Ordering};

static DIR_COUNTER: AtomicU64 = AtomicU64::new(0);

fn fresh_dir(tag: &str) -> String {
    let n = DIR_COUNTER.fetch_add(1, Ordering::SeqCst);
    let d = std::env::temp_dir().join(format!("d4rt_rs_perm_{tag}_{}_{}", std::process::id(), n));
    std::fs::create_dir_all(&d).unwrap();
    d.to_string_lossy().to_string()
}

fn eval_io(c: &mut Context, body: &str) -> Result<d4rt_rs::Value, d4rt_rs::InterpError> {
    let src = format!("import 'dart:io';\nvoid main() {{ {body} }}");
    c.eval(&src)
}

/// Runs `body` in an async main that catches errors and returns
/// `"OK"` or `"ERR: <message>"`, so assertions read the message the way
/// interpreted code would.
fn err_of(c: &mut Context, body: &str) -> String {
    let src = format!(
        "import 'dart:io';\nFuture<String> main() async {{ try {{ {body}; return 'OK'; }} catch (e) {{ return 'ERR: $e'; }} }}"
    );
    match c.eval(&src) {
        Ok(v) => v.as_str().unwrap_or("NO-STR").to_string(),
        Err(e) => format!("EVAL-ERR: {e}"),
    }
}

// ---------------------------------------------------------------------------
// Default deny
// ---------------------------------------------------------------------------

#[test]
fn default_denies_filesystem() {
    let mut c = Context::new();
    let d = fresh_dir("deny");
    std::fs::write(format!("{d}/f.txt"), "secret").unwrap();
    assert!(
        err_of(&mut c, &format!("File('{d}/f.txt').readAsStringSync();"))
            .contains("Permission denied")
    );
    assert!(
        err_of(
            &mut c,
            &format!("File('{d}/g.txt').writeAsStringSync('x');")
        )
        .contains("Permission denied")
    );
    assert!(err_of(&mut c, &format!("File('{d}').statSync();")).contains("Permission denied"));
    assert!(err_of(&mut c, &format!("Directory('{d}').listSync();")).contains("Permission denied"));
}

#[test]
fn default_denies_network_process_isolate_env() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();

    let mut c = Context::new();
    assert!(err_of(&mut c, "Platform.environment['PATH'];").contains("Permission denied"));
    assert!(
        err_of(
            &mut c,
            &format!("await Socket.connect('127.0.0.1', {port});")
        )
        .contains("Permission denied")
    );
    assert!(err_of(&mut c, "Process.runSync('echo', ['hi']);").contains("Permission denied"));

    let src = "import 'dart:isolate';\nFuture<String> main() async { try { ReceivePort(); return 'OK'; } catch (e) { return 'ERR: $e'; } }";
    let v = c.eval(src).unwrap();
    assert!(v.as_str().unwrap().contains("Permission denied"));
}

// ---------------------------------------------------------------------------
// Granting
// ---------------------------------------------------------------------------

#[test]
fn grant_any_filesystem_allows_everything() {
    let mut c = Context::new();
    c.grant(Permission::Filesystem(FilesystemPermission::Any));
    let d = fresh_dir("any");
    eval_io(
        &mut c,
        &format!("File('{d}/f.txt').writeAsStringSync('hi');"),
    )
    .unwrap();
    let v = eval_io(&mut c, &format!("File('{d}/f.txt').readAsStringSync();")).unwrap();
    assert_eq!(v.as_str(), Some("hi"));
}

#[test]
fn grant_read_allows_read_not_write() {
    let mut c = Context::new();
    c.grant(Permission::Filesystem(FilesystemPermission::Read));
    let d = fresh_dir("readonly");
    std::fs::write(format!("{d}/f.txt"), "secret").unwrap();
    assert_eq!(
        err_of(&mut c, &format!("File('{d}/f.txt').readAsStringSync();")),
        "OK"
    );
    assert!(
        err_of(
            &mut c,
            &format!("File('{d}/g.txt').writeAsStringSync('x');")
        )
        .contains("Permission denied")
    );
}

#[test]
fn grant_read_path_scopes_to_prefix() {
    let mut c = Context::new();
    let base = fresh_dir("prefix");
    let inside = format!("{base}/inside");
    std::fs::create_dir_all(&inside).unwrap();
    std::fs::write(format!("{inside}/a.txt"), "in").unwrap();
    let outside = fresh_dir("outside");
    std::fs::write(format!("{outside}/b.txt"), "out").unwrap();

    c.grant(Permission::Filesystem(FilesystemPermission::ReadPath(
        base.clone(),
    )));
    assert_eq!(
        err_of(
            &mut c,
            &format!("File('{inside}/a.txt').readAsStringSync();")
        ),
        "OK"
    );
    assert!(
        err_of(
            &mut c,
            &format!("File('{outside}/b.txt').readAsStringSync();")
        )
        .contains("Permission denied")
    );
    assert!(
        err_of(
            &mut c,
            &format!("File('{base}/w.txt').writeAsStringSync('x');")
        )
        .contains("Permission denied")
    );
}

#[test]
fn grant_path_allows_read_and_write_under_prefix() {
    let mut c = Context::new();
    let base = fresh_dir("both");
    c.grant(Permission::Filesystem(FilesystemPermission::Path(
        base.clone(),
    )));
    eval_io(
        &mut c,
        &format!("File('{base}/f.txt').writeAsStringSync('hi');"),
    )
    .unwrap();
    let v = eval_io(&mut c, &format!("File('{base}/f.txt').readAsStringSync();")).unwrap();
    assert_eq!(v.as_str(), Some("hi"));
}

#[test]
fn grant_network_connect_host_scoped() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();

    let mut c = Context::new();
    c.grant(Permission::Network(NetworkPermission::Connect(
        "127.0.0.1".into(),
    )));
    assert_eq!(
        err_of(
            &mut c,
            &format!("final s = await Socket.connect('127.0.0.1', {port}); s.destroy();")
        ),
        "OK"
    );
    assert!(
        err_of(
            &mut c,
            &format!("await Socket.connect('localhost', {port});")
        )
        .contains("Permission denied")
    );
}

#[test]
fn grant_process_command_scoped() {
    let mut c = Context::new();
    c.grant(Permission::Process(ProcessPermission::Command(
        "echo".into(),
    )));
    assert_eq!(
        err_of(&mut c, "final r = Process.runSync('echo', ['hi']);"),
        "OK"
    );
    assert!(
        err_of(&mut c, "Process.runSync('cat', ['/etc/hostname']);").contains("Permission denied")
    );
}

#[test]
fn grant_isolate_allows_ports_and_spawn() {
    let mut c = Context::new();
    c.grant(Permission::Isolate);
    let src = "import 'dart:isolate';\nvoid main() {\n  final rp = ReceivePort();\n  rp.sendPort.send('x');\n  rp.close();\n}";
    c.eval(src).unwrap();
}

#[test]
fn grant_env_allows_platform_environment() {
    let mut c = Context::new();
    c.grant(Permission::Env);
    assert_eq!(
        err_of(&mut c, "Platform.environment['PATH'] != null;"),
        "OK"
    );
}

// ---------------------------------------------------------------------------
// has_permission / revoke
// ---------------------------------------------------------------------------

#[test]
fn has_permission_coverage_rules() {
    let mut c = Context::new();
    assert!(!c.has_permission(&Permission::Filesystem(FilesystemPermission::Read)));
    c.grant(Permission::Filesystem(FilesystemPermission::Any));
    assert!(c.has_permission(&Permission::Filesystem(FilesystemPermission::Read)));
    assert!(
        c.has_permission(&Permission::Filesystem(FilesystemPermission::ReadPath(
            "/x/y".into()
        )))
    );

    let mut c = Context::new();
    c.grant(Permission::Filesystem(FilesystemPermission::ReadPath(
        "/a".into(),
    )));
    assert!(
        c.has_permission(&Permission::Filesystem(FilesystemPermission::ReadPath(
            "/a/b".into()
        )))
    );
    assert!(
        !c.has_permission(&Permission::Filesystem(FilesystemPermission::ReadPath(
            "/a2".into()
        )))
    );
    assert!(
        !c.has_permission(&Permission::Filesystem(FilesystemPermission::WritePath(
            "/a/b".into()
        )))
    );
}

#[test]
fn revoke_withdraws_grant() {
    let mut c = Context::new();
    c.grant(Permission::Filesystem(FilesystemPermission::Read));
    let d = fresh_dir("revoke");
    std::fs::write(format!("{d}/f.txt"), "x").unwrap();
    assert_eq!(
        err_of(&mut c, &format!("File('{d}/f.txt').readAsStringSync();")),
        "OK"
    );

    c.revoke(&Permission::Filesystem(FilesystemPermission::Read));
    assert!(!c.has_permission(&Permission::Filesystem(FilesystemPermission::Read)));
    assert!(
        err_of(&mut c, &format!("File('{d}/f.txt').readAsStringSync();"))
            .contains("Permission denied")
    );
}

#[test]
fn broad_grant_survives_specific_revoke() {
    let mut c = Context::new();
    c.grant(Permission::Filesystem(FilesystemPermission::Any));
    c.revoke(&Permission::Filesystem(FilesystemPermission::Read));
    assert!(c.has_permission(&Permission::Filesystem(FilesystemPermission::Read)));
}

#[test]
fn dart_code_can_catch_permission_error() {
    let mut c = Context::new();
    let d = fresh_dir("catch");
    let v = c
        .eval(&format!(
            "import 'dart:io';\nString main() {{
               try {{
                 File('{d}/f.txt').writeAsStringSync('x');
                 return 'no-throw';
               }} catch (e) {{
                 return 'caught';
               }}
             }}"
        ))
        .unwrap();
    assert_eq!(v.as_str(), Some("caught"));
}

// ---------------------------------------------------------------------------
// One-shot construction (Deno-style `sandbox({ permissions })`)
// ---------------------------------------------------------------------------

#[test]
fn new_with_options_grants_at_construction() {
    let d = fresh_dir("opts");
    std::fs::write(format!("{d}/f.txt"), "hi").unwrap();

    let mut c = Context::new_with(d4rt_rs::SandboxOptions {
        filesystem: Some(FilesystemPermission::ReadPath(d.clone())),
        ..Default::default()
    });
    assert_eq!(
        err_of(&mut c, &format!("File('{d}/f.txt').readAsStringSync();")),
        "OK"
    );
    assert!(
        err_of(
            &mut c,
            &format!("File('{d}/g.txt').writeAsStringSync('x');")
        )
        .contains("Permission denied")
    );

    // Unset options stay denied.
    let mut c = Context::new_with(d4rt_rs::SandboxOptions {
        filesystem: Some(FilesystemPermission::Any),
        ..Default::default()
    });
    assert!(err_of(&mut c, "Process.runSync('echo', ['hi']);").contains("Permission denied"));
}

#[test]
fn new_with_allow_all_matches_permissive() {
    let d = fresh_dir("allowall");
    let mut c = Context::new_with(d4rt_rs::SandboxOptions::allow_all());
    assert_eq!(
        err_of(
            &mut c,
            &format!(
                "final f = File('{d}/a.txt'); f.writeAsStringSync('hi'); f.readAsStringSync();"
            )
        ),
        "OK"
    );
    assert_eq!(
        err_of(&mut c, "Platform.environment['PATH'] != null;"),
        "OK"
    );
}

// ---------------------------------------------------------------------------
// Module loading: virtual registry vs. filesystem flag (Permission::ModuleFiles)
// ---------------------------------------------------------------------------

#[test]
fn import_from_filesystem_denied_by_default() {
    let mut c = Context::new();
    let d = fresh_dir("moddeny");
    std::fs::write(format!("{d}/mylib.dart"), "int answer() => 42;").unwrap();

    // Even with filesystem permission, import refuses to touch the disk
    // without Permission::ModuleFiles.
    let err = c
        .eval(&format!(
            "import '{d}/mylib.dart';\nint main() => answer();"
        ))
        .unwrap_err()
        .to_string();
    assert!(err.contains("Unresolved import"), "got: {err}");
    assert!(err.contains("ModuleFiles"), "got: {err}");
}

#[test]
fn grant_module_files_allows_filesystem_import() {
    let mut c = Context::new();
    c.grant(Permission::ModuleFiles);
    c.grant(Permission::Filesystem(FilesystemPermission::Any));
    let d = fresh_dir("modok");
    std::fs::write(format!("{d}/mylib.dart"), "int answer() => 42;").unwrap();
    let v = c
        .eval(&format!(
            "import '{d}/mylib.dart';\nString main() => '${{answer()}}';"
        ))
        .unwrap();
    assert_eq!(v.as_str(), Some("42"));
}

#[test]
fn module_files_does_not_bypass_fs_read_permission() {
    let mut c = Context::new();
    c.grant(Permission::ModuleFiles);
    let d = fresh_dir("modpath");
    std::fs::write(format!("{d}/mylib.dart"), "int answer() => 42;").unwrap();
    let err = c
        .eval(&format!(
            "import '{d}/mylib.dart';\nint main() => answer();"
        ))
        .unwrap_err()
        .to_string();
    assert!(err.contains("Unresolved import"), "got: {err}");
}

#[test]
fn virtual_module_import_works_without_filesystem() {
    let mut c = Context::new();
    c.register_dart_module("./mylib.dart", "int triple(int x) => x * 3;");
    let v = c
        .eval("import './mylib.dart';\nString main() => '${triple(7)}';")
        .unwrap();
    assert_eq!(v.as_str(), Some("21"));
}

#[test]
fn virtual_module_takes_precedence_over_filesystem_file() {
    let mut c = Context::new();
    c.grant(Permission::ModuleFiles);
    c.grant(Permission::Filesystem(FilesystemPermission::Any));
    let d = fresh_dir("modprec");
    std::fs::write(format!("{d}/mylib.dart"), "int answer() => 1;").unwrap();
    c.register_dart_module(&format!("{d}/mylib.dart"), "int answer() => 2;");
    let v = c
        .eval(&format!(
            "import '{d}/mylib.dart';\nString main() => '${{answer()}}';"
        ))
        .unwrap();
    assert_eq!(v.as_str(), Some("2"));
}

#[test]
fn re_registered_module_replaces_source() {
    let mut c = Context::new();
    c.register_dart_module("./mylib.dart", "int answer() => 1;");
    c.register_dart_module("./mylib.dart", "int answer() => 2;");
    let v = c
        .eval("import './mylib.dart';\nString main() => '${answer()}';")
        .unwrap();
    assert_eq!(v.as_str(), Some("2"));
}

#[test]
fn new_with_filesystem_modules_flag_enables_fs_import() {
    let d = fresh_dir("modflag");
    std::fs::write(format!("{d}/mylib.dart"), "int answer() => 42;").unwrap();

    // filesystem permission alone is not enough.
    let mut c = Context::new_with(d4rt_rs::SandboxOptions {
        filesystem: Some(FilesystemPermission::Any),
        ..Default::default()
    });
    assert!(
        c.eval(&format!(
            "import '{d}/mylib.dart';\nint main() => answer();"
        ))
        .unwrap_err()
        .to_string()
        .contains("Unresolved import")
    );

    // filesystem_modules: true allows it (fs permission is also granted).
    let mut c = Context::new_with(d4rt_rs::SandboxOptions {
        filesystem: Some(FilesystemPermission::Any),
        filesystem_modules: true,
        ..Default::default()
    });
    let v = c
        .eval(&format!(
            "import '{d}/mylib.dart';\nString main() => '${{answer()}}';"
        ))
        .unwrap();
    assert_eq!(v.as_str(), Some("42"));
}

#[test]
fn allow_all_enables_filesystem_import() {
    let mut c = Context::new_with(d4rt_rs::SandboxOptions::allow_all());
    let d = fresh_dir("modallow");
    std::fs::write(format!("{d}/mylib.dart"), "int answer() => 42;").unwrap();
    let v = c
        .eval(&format!(
            "import '{d}/mylib.dart';\nString main() => '${{answer()}}';"
        ))
        .unwrap();
    assert_eq!(v.as_str(), Some("42"));
}

// ---------------------------------------------------------------------------
// Export: re-exports public names through the same resolution rules
// ---------------------------------------------------------------------------

#[test]
fn export_virtual_module_reexports_public_names() {
    let mut c = Context::new();
    c.register_dart_module("./lib.dart", "int pub() => 1;\nint _priv() => 2;");
    let v = c
        .eval("export './lib.dart';\nString main() => '${pub()}';")
        .unwrap();
    assert_eq!(v.as_str(), Some("1"));
}

#[test]
fn export_does_not_leak_private_names() {
    let mut c = Context::new();
    c.register_dart_module("./lib.dart", "int pub() => 1;\nint _priv() => 2;");
    assert!(
        c.eval("export './lib.dart';\nString main() => '${_priv()}';")
            .is_err()
    );
}

#[test]
fn export_show_filters_names() {
    let mut c = Context::new();
    c.register_dart_module("./lib.dart", "int a() => 1;\nint b() => 2;");
    let v = c
        .eval("export './lib.dart' show a;\nString main() => '${a()}';")
        .unwrap();
    assert_eq!(v.as_str(), Some("1"));

    let mut c = Context::new();
    c.register_dart_module("./lib.dart", "int a() => 1;\nint b() => 2;");
    assert!(
        c.eval("export './lib.dart' show a;\nString main() => '${b()}';")
            .is_err()
    );
}

#[test]
fn export_chains_through_import() {
    let mut c = Context::new();
    c.register_dart_module("./a.dart", "export './b.dart';");
    c.register_dart_module("./b.dart", "int bval() => 7;");
    let v = c
        .eval("import './a.dart';\nString main() => '${bval()}';")
        .unwrap();
    assert_eq!(v.as_str(), Some("7"));
}

#[test]
fn export_does_not_propagate_prefixed_imports() {
    let mut c = Context::new();
    c.register_dart_module("./b.dart", "import 'dart:math' as m;");
    assert!(
        c.eval("export './b.dart';\nString main() => '${m.pi}';")
            .is_err()
    );
}

#[test]
fn export_from_filesystem_requires_module_files() {
    let d = fresh_dir("expgate");
    std::fs::write(format!("{d}/lib.dart"), "int pub() => 5;").unwrap();

    let mut c = Context::new();
    let err = c
        .eval(&format!(
            "export '{d}/lib.dart';\nString main() => '${{pub()}}';"
        ))
        .unwrap_err()
        .to_string();
    assert!(err.contains("Unresolved import"), "got: {err}");

    let mut c = Context::new_with(d4rt_rs::SandboxOptions::allow_all());
    let v = c
        .eval(&format!(
            "export '{d}/lib.dart';\nString main() => '${{pub()}}';"
        ))
        .unwrap();
    assert_eq!(v.as_str(), Some("5"));
}

// ---------------------------------------------------------------------------
// Rust module libraries (bare-name registration, like `import 'cheerio'`)
// ---------------------------------------------------------------------------

fn rust_thing_class(name: &str, value: i64) -> std::sync::Arc<d4rt_rs::class::BridgedClassDef> {
    let mut bc = d4rt_rs::class::BridgedClassDef::new(name);
    bc.static_getters.insert(
        "answer".to_string(),
        d4rt_rs::stdlib::helpers::static_getter(move |_v| Ok(d4rt_rs::Value::Int(value))),
    );
    std::sync::Arc::new(bc)
}

#[test]
fn rust_module_library_bare_name_import_works_without_any_permission() {
    d4rt_rs::stdlib::register_module_library(
        "bare_cheerio_mod",
        vec![rust_thing_class("CheerioThing", 42)],
    );

    // No permissions at all: host-registered Rust modules are config, not
    // filesystem access.
    let mut c = Context::new();
    let v = c
        .eval("import 'bare_cheerio_mod';\nvoid main() { return CheerioThing.answer; }")
        .unwrap();
    assert_eq!(v, d4rt_rs::Value::Int(42));
}

#[test]
fn rust_module_library_supports_export_and_show_hide() {
    d4rt_rs::stdlib::register_module_library(
        "bare_cheerio_mod2",
        vec![rust_thing_class("CheerioThing", 7)],
    );

    let mut c = Context::new();
    let v = c
        .eval("import 'bare_cheerio_mod2' show CheerioThing;\nvoid main() { return CheerioThing.answer; }")
        .unwrap();
    assert_eq!(v, d4rt_rs::Value::Int(7));
}

#[test]
fn rust_module_library_wins_over_virtual_dart_source() {
    d4rt_rs::stdlib::register_module_library(
        "./prec_rust_vs_dart.dart",
        vec![rust_thing_class("PrecThing", 1)],
    );

    let mut c = Context::new();
    c.register_dart_module("./prec_rust_vs_dart.dart", "int answer() => 2;");
    let v = c
        .eval("import './prec_rust_vs_dart.dart';\nvoid main() { return PrecThing.answer; }")
        .unwrap();
    assert_eq!(v, d4rt_rs::Value::Int(1));
}
