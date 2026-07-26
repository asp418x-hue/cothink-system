#!/usr/bin/env bash
# Run host-side JVM unit tests for the Cothink core.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

export JAVA_HOME="${JAVA_HOME:-/usr/lib/jvm/java-21-openjdk-amd64}"

# Find bazel or bazelisk
BAZEL_BIN=""
if command -v bazel >/dev/null 2>&1; then
    BAZEL_BIN="bazel"
elif command -v bazelisk >/dev/null 2>&1; then
    BAZEL_BIN="bazelisk"
else
    echo "ERROR: 'bazel' or 'bazelisk' command not found."
    echo "Please install Bazelisk (e.g. npm install -g @bazel/bazelisk or download from https://github.com/bazelbuild/bazelisk)"
    exit 127
fi

echo "==> Running //javatests/com/cothink/system/core:all via $BAZEL_BIN"
exec "$BAZEL_BIN" test //javatests/com/cothink/system/core:all --test_output=errors "$@"

