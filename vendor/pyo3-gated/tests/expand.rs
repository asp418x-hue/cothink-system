#[test]
fn expansion_snapshots() {
    if !cargo_expand_supports_theme() {
        eprintln!("skipping expansion snapshots: cargo-expand is missing or too old");
        return;
    }

    macrotest::expand("tests/expand/*.rs");
}

fn cargo_expand_supports_theme() -> bool {
    let output = std::process::Command::new("cargo")
        .args(["expand", "--help"])
        .output();

    let Ok(output) = output else {
        return false;
    };

    String::from_utf8_lossy(&output.stdout).contains("--theme")
        || String::from_utf8_lossy(&output.stderr).contains("--theme")
}
