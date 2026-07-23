#!/usr/bin/env bash
# Run host-side JVM unit tests for the Cothink core.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

export JAVA_HOME="${JAVA_HOME:-/usr/lib/jvm/java-21-openjdk-amd64}"

echo "==> Running //javatests/com/cothink/system/core:all"
bazel test //javatests/com/cothink/system/core:all --test_output=errors "$@"
