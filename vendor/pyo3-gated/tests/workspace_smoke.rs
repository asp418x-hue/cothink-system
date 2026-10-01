use std::path::{Path, PathBuf};
use std::process::Command;

fn workspace_root() -> PathBuf {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let candidate = manifest_dir.join("../..");

    if candidate.join("Cargo.toml").exists() && candidate.join("crates").exists() {
        candidate
    } else {
        manifest_dir.to_path_buf()
    }
}

fn configure_cargo_command(cmd: &mut Command) {
    if std::env::var_os("CARGO_TARGET_DIR").is_none() {
        cmd.env("CARGO_TARGET_DIR", workspace_root().join("target"));
    }
}

fn cargo_check(package: &str, features: &[&str]) {
    let mut cmd = Command::new("cargo");
    configure_cargo_command(&mut cmd);
    cmd.arg("check").arg("-p").arg(package);

    if !features.is_empty() {
        cmd.arg("--features").arg(features.join(","));
    }

    let status = cmd.status().expect("failed to run cargo check");
    assert!(status.success(), "cargo check failed for {package}");
}

fn cargo_check_manifest(manifest_path: &str, features: &[&str]) {
    let mut cmd = Command::new("cargo");
    configure_cargo_command(&mut cmd);
    cmd.arg("check").arg("--manifest-path").arg(manifest_path);

    if !features.is_empty() {
        cmd.arg("--features").arg(features.join(","));
    }

    let status = cmd.status().expect("failed to run cargo check");
    assert!(status.success(), "cargo check failed for {manifest_path}");
}

fn fixture_manifest(relative_path: &str) -> String {
    if Path::new(relative_path).exists() {
        return relative_path.to_string();
    }

    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(relative_path)
        .display()
        .to_string()
}

#[test]
fn plain_user_compiles_without_pyo3() {
    cargo_check("plain-user", &[]);
}

#[test]
fn missing_pyo3_user_compiles_without_python_feature() {
    cargo_check_manifest(
        &fixture_manifest("tests/crates/missing-pyo3-user/Cargo.toml"),
        &[],
    );
}

#[test]
fn missing_pyo3_user_compiles_with_facade_python_feature() {
    cargo_check_manifest(
        &fixture_manifest("tests/crates/missing-pyo3-user/Cargo.toml"),
        &["python"],
    );
}

#[test]
fn python_user_compiles_without_direct_stub_gen_dep() {
    cargo_check("python-user", &["python"]);
}

#[test]
fn stub_user_compiles_without_direct_stub_gen_dep() {
    cargo_check("stub-user", &["stub-gen"]);
}

#[test]
fn renamed_dependency_user_compiles() {
    cargo_check("renamed-dep-user", &["python"]);
}

#[test]
fn renamed_pyo3_dependency_user_compiles() {
    cargo_check("renamed-pyo3-user", &["python"]);
}

#[test]
fn facade_pyo3_user_compiles_without_direct_pyo3() {
    cargo_check("facade-pyo3-user", &[]);
    cargo_check("facade-pyo3-user", &["python"]);
    cargo_check("facade-pyo3-user", &["stub-gen"]);
    cargo_check("facade-pyo3-user", &["anyhow"]);
    cargo_check("facade-pyo3-user", &["abi3-py39"]);
}

#[test]
fn renamed_facade_user_compiles() {
    cargo_check("renamed-facade-user", &["python"]);
}

#[test]
fn direct_pyo3_override_user_compiles() {
    cargo_check("direct-pyo3-override-user", &["python"]);
}
