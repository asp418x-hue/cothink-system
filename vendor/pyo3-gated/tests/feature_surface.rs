use std::collections::{BTreeMap, BTreeSet};
use std::process::Command;

use serde_json::Value;

const FACADE_OWNED: &[&str] = &["python", "stub-gen"];
const PYO3_EXCLUDED: &[&str] = &["default", "macros", "pyo3-macros"];
const REQUIRED_ABI: &[&str] = &["abi3-py315", "abi3t", "abi3t-py315"];

#[test]
fn facade_exposes_current_pyo3_features() {
    let workspace_manifest =
        std::fs::read_to_string(format!("{}/../../Cargo.toml", env!("CARGO_MANIFEST_DIR")))
            .expect("read workspace Cargo.toml");
    assert!(
        workspace_manifest.contains("[workspace.dependencies]")
            && workspace_manifest.contains("pyo3 = { version = \"0.29.0\"")
            && workspace_manifest.contains("features = [\"macros\"]"),
        "workspace Cargo.toml should own the PyO3 dependency with macros enabled"
    );

    let metadata = cargo_metadata();
    let packages = metadata["packages"]
        .as_array()
        .expect("metadata packages should be an array");

    let facade = package(packages, "pyo3-gated");
    let pyo3 = package(packages, "pyo3");
    let facade_features = feature_map(facade);
    let pyo3_features = feature_map(pyo3);

    for owned in FACADE_OWNED {
        assert!(
            facade_features.contains_key(*owned),
            "facade-owned feature `{owned}` should exist"
        );
    }

    assert_eq!(
        facade_features.get("default"),
        Some(&BTreeSet::new()),
        "facade `default` feature should exist and stay empty"
    );
    assert!(
        !facade_features.contains_key("macros"),
        "PyO3 feature `macros` should remain intentionally unexposed"
    );
    assert!(
        !facade_features.contains_key("pyo3-macros"),
        "PyO3 feature `pyo3-macros` should remain intentionally unexposed"
    );

    for feature in pyo3_features.keys() {
        if PYO3_EXCLUDED.contains(&feature.as_str()) {
            continue;
        }

        let Some(values) = facade_features.get(feature) else {
            panic!(
                "PyO3 feature `{feature}` is not exposed by pyo3-gated; expose it or add it to PYO3_EXCLUDED"
            );
        };

        assert!(
            values.contains("python"),
            "facade feature `{feature}` should enable `python`"
        );
        assert!(
            values.contains(&format!("pyo3/{feature}")),
            "facade feature `{feature}` should pass through to `pyo3/{feature}`"
        );
    }

    let full = facade_features
        .get("full")
        .expect("facade should expose PyO3 full");
    for feature in abi_features(&pyo3_features) {
        assert!(
            facade_features.contains_key(&feature),
            "ABI feature `{feature}` should be exposed by the facade"
        );
        assert!(
            !full.contains(&feature) && !full.contains(&format!("pyo3/{feature}")),
            "ABI feature `{feature}` must not be included in facade `full`"
        );
    }

    for feature in REQUIRED_ABI {
        assert!(
            pyo3_features.contains_key(*feature),
            "test fixture expects PyO3 0.29 feature `{feature}`"
        );
        assert!(
            facade_features.contains_key(*feature),
            "facade should expose PyO3 0.29 feature `{feature}`"
        );
    }
}

fn cargo_metadata() -> Value {
    let output = Command::new("cargo")
        .args([
            "metadata",
            "--format-version",
            "1",
            "--features",
            "stub-gen",
        ])
        .output()
        .expect("run cargo metadata");
    assert!(
        output.status.success(),
        "cargo metadata failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("parse cargo metadata JSON")
}

fn package<'a>(packages: &'a [Value], name: &str) -> &'a Value {
    packages
        .iter()
        .find(|package| package["name"].as_str() == Some(name))
        .unwrap_or_else(|| panic!("metadata should include package `{name}`"))
}

fn feature_map(package: &Value) -> BTreeMap<String, BTreeSet<String>> {
    package["features"]
        .as_object()
        .expect("package features should be an object")
        .iter()
        .map(|(name, values)| {
            let values = values
                .as_array()
                .expect("feature values should be an array")
                .iter()
                .map(|value| {
                    value
                        .as_str()
                        .expect("feature value should be a string")
                        .to_owned()
                })
                .collect();
            (name.to_owned(), values)
        })
        .collect()
}

fn abi_features(
    features: &BTreeMap<String, BTreeSet<String>>,
) -> impl Iterator<Item = String> + '_ {
    features
        .keys()
        .filter(|feature| feature.starts_with("abi3"))
        .cloned()
}
