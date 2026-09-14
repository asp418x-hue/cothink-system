# Cothink System

Multi-runtime thinkspace orchestrator (Rust / Go / Python) packaged as a
**Bazel-built Android app**.

## Android (Bazel)

```bash
export ANDROID_HOME=$HOME/android-sdk
./scripts/build_android.sh          # → cothink_app.apk
./scripts/run_tests.sh              # host JVM unit tests
adb install -r bazel-bin/java/com/cothink/system/cothink_app.apk
```

Full docs: **[ANDROID.md](./ANDROID.md)**

### Quick Bazel targets

| Target | Description |
|--------|-------------|
| `//java/com/cothink/system:cothink_app` | Android APK |
| `//java/com/cothink/system:cothink_core` | Ported orchestrator core |
| `//javatests/com/cothink/system/core:all` | Unit tests |

## Standalone Runtimes & Prerequisites

### Native System Dependencies
```bash
# Ubuntu/Debian native dependencies (required for Rust rdkafka-sys bindings)
sudo apt-get install -y libcurl4-openssl-dev build-essential
```

### Go Runtime
```bash
go test ./...
```

### Python Runtime
```bash
pip install -r requirements.txt
python3 crypto_test_suite.py
```

### Rust Runtime
```bash
cargo test
```

## License

This is free and unencumbered software released into the public domain.

Anyone is free to copy, modify, publish, use, compile, sell, or
distribute this software, either in source code form or as a compiled
binary, for any purpose, commercial or non-commercial, and by any means.
