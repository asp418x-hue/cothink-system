#!/usr/bin/env bash
# Build the Cothink System Android APK with Bazel.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

export ANDROID_HOME="${ANDROID_HOME:-${ANDROID_SDK_ROOT:-$HOME/android-sdk}}"
export ANDROID_SDK_ROOT="$ANDROID_HOME"
export JAVA_HOME="${JAVA_HOME:-/usr/lib/jvm/java-21-openjdk-amd64}"

if [[ ! -d "$ANDROID_HOME/platforms" ]]; then
  echo "ERROR: Android SDK not found at ANDROID_HOME=$ANDROID_HOME" >&2
  echo "Install cmdline-tools and run:" >&2
  echo "  sdkmanager \"platforms;android-34\" \"build-tools;34.0.0\" \"platform-tools\"" >&2
  exit 1
fi

if ! command -v bazel >/dev/null 2>&1; then
  echo "ERROR: bazel not found on PATH (install bazelisk)." >&2
  exit 1
fi

echo "==> ANDROID_HOME=$ANDROID_HOME"
echo "==> Building //java/com/cothink/system:cothink_app"

bazel build //java/com/cothink/system:cothink_app "$@"

APK="$(bazel cquery --output=files //java/com/cothink/system:cothink_app 2>/dev/null | head -1 || true)"
if [[ -z "${APK:-}" ]]; then
  APK="bazel-bin/java/com/cothink/system/cothink_app.apk"
fi

echo
echo "APK: $APK"
if [[ -f "$APK" ]]; then
  ls -lh "$APK"
fi
echo
echo "Install on a device/emulator:"
echo "  adb install -r $APK"
