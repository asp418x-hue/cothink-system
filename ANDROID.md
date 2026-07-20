# Cothink System — Bazel Android App

This repository is packaged as an **Android application built with Bazel**
(`rules_android` + Bzlmod). The original multi-language orchestrator
(Rust / Go / Python / Perl / Lua) is ported into a pure-Java core that runs
natively on Android (API 24+), with the same spiral allocation, dynamic
semaphore, history FOV, classifier, and thermal/metrics probes.

## What’s included

| Path | Role |
|------|------|
| `MODULE.bazel` | Bzlmod deps: `rules_android`, `rules_java`, JUnit |
| `.bazelrc` / `.bazelversion` | Java 17 + C++17 flags for Android builder tools |
| `java/com/cothink/system/` | Android app sources, resources, manifest, BUILD |
| `java/com/cothink/system/core/` | Ported cothink runtime (orchestrator, spiral, FOV…) |
| `javatests/com/cothink/system/core/` | Host JVM unit tests |
| `scripts/build_android.sh` | One-shot APK build |
| `scripts/run_tests.sh` | One-shot unit tests |
| `tools/debug.keystore` | Debug signing keystore |

### Core modules (ported)

- **SpiralAllocator** — φ-log spiral task order (`src/main.rs`)
- **Orchestrator** — concurrent subagent dispatch + ScalarSpawn semantics
- **DynamicSemaphore** — resizable concurrency choke-point (`cothink/semaphore.go`)
- **HistoryTracker + FOV** — ring-buffer traces (`src/history.rs` / `cothink/history.go`)
- **Classifier** — immutable sensor-frame anomaly scoring (`src/bin/classifier.rs`)
- **SystemMetrics** — thermal / CPU / disk probes with simulated fallbacks

On-device, the perl/lua/python agent binaries are replaced by in-process Java
stages so the app does not require a full Linux userland toolchain.

## Prerequisites

1. **JDK 17+** (OpenJDK 21 works)
2. **Bazelisk / Bazel 7.4+**
3. **Android SDK** with:
   - `platforms;android-34`
   - `build-tools;34.0.0`
   - `platform-tools`

```bash
export ANDROID_HOME=$HOME/android-sdk
export JAVA_HOME=/usr/lib/jvm/java-21-openjdk-amd64
yes | sdkmanager --licenses
sdkmanager "platforms;android-34" "build-tools;34.0.0" "platform-tools"
```

Point Bazel at the SDK via `user.bazelrc` (already set for `$HOME/android-sdk`)
or:

```bash
echo "build --action_env=ANDROID_HOME=$ANDROID_HOME" >> user.bazelrc
```

## Build the APK

```bash
./scripts/build_android.sh
# or:
bazel build //java/com/cothink/system:cothink_app
```

Output (typical):

```
bazel-bin/java/com/cothink/system/cothink_app.apk
```

Install:

```bash
adb install -r bazel-bin/java/com/cothink/system/cothink_app.apk
```

## Run unit tests

```bash
./scripts/run_tests.sh
# or:
bazel test //javatests/com/cothink/system/core:all
```

## App usage

1. Launch **Cothink System**
2. Adjust the **Agents** slider (1–16)
3. Tap **Run** — spiral-ordered agents execute under the dynamic semaphore
4. Watch the live log + FOV (temp / CPU / success·fail counters)
5. **Metrics** refreshes thermal/CPU/disk probes
6. **Cancel** aborts an in-flight run

## Targets

```bash
bazel build //:cothink_app          # alias → android binary
bazel build //java/com/cothink/system:cothink_lib
bazel build //java/com/cothink/system:cothink_core
bazel test  //javatests/com/cothink/system/core:all
```

## Notes

- Original Rust/Go/Python sources remain in-tree for reference and desktop use.
- The Android package id is `com.cothink.system`.
- Debug keystore password: `android` / alias `androiddebugkey`.
- First `rules_android` APK builds pull a large toolchain graph (Go builder tools,
  protobuf, Android SDK stubs). Prefer a machine with ≥4 GB free RAM; this repo’s
  `.bazelrc` already caps jobs/RAM for constrained CI sandboxes.
- Core logic can be validated without the Android toolchain:

  ```bash
  javac -d /tmp/cothink-classes java/com/cothink/system/core/*.java
  # + JUnit tests under javatests/… (see scripts/run_tests.sh via Bazel)
  ```
