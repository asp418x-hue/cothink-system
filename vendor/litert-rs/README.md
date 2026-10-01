# litert-rs

**On-device LLM inference in Rust** over FFI, no cloud.
Load a LiteRT-LM model (`.litertlm`) — Gemma, Qwen, DeepSeek… — and generate text
100% locally.

```rust
use litert_rs::LiteRtModel;

let model = LiteRtModel::load("/path/gemma-4-E2B-it.litertlm", "cpu", 2048)?;
println!("{}", model.ask("What is 2 + 2? Answer briefly.")?);
# Ok::<(), litert_rs::LiteRtError>(())
```

This is the Rust sibling of [`litert_dart`](https://github.com/JhonaCodes/litert_dart):
same native engine (LiteRT-LM), same models, different host language.

> Inference is done by the native **LiteRT-LM (C++) engine**, called over FFI.
> Rust only orchestrates — it does not change generation speed.

## Install

```toml
[dependencies]
litert-rs = "0.1"
```

Then download the native libraries once into your project:

```bash
cargo run --bin fetch_natives   # → ./native/<platform>/
```

`fetch_natives` downloads the platform archive from this project's own
SHA256-verified GitHub release (`JhonaCodes/litert-rs`, tag `native-v0.13.1`)
and extracts it to `./native/<platform>/`. Hosting the binaries on our own
release means we don't depend on any third party for them.

To use the library without the fetch tool (no HTTP deps), disable features:

```toml
litert-rs = { version = "0.1", default-features = false }
```

## Models

- **Supported:** `.litertlm` files (LiteRT-LM format).
- **Not supported:** `.gguf` (llama.cpp/ollama), `.task`/`.bin` (MediaPipe).

## API

| Item | Description |
|---|---|
| `LiteRtModel::load(path, backend, max_tokens)` | Load a `.litertlm` into RAM. `backend`: `"cpu"` or `"gpu"`. |
| `model.ask(prompt) -> Result<String>` | Blocking generation. |
| `model.model_path() / backend() / max_tokens()` | Accessors. |

`LiteRtModel` is `Send` but not thread-safe for concurrent calls (the engine
allows one live session at a time); wrap it in a `Mutex` to share it.

## Native library resolution

`native_lib_dir()` searches, in order:

1. `LITERTLM_LIB_DIR` env var (always wins).
2. `./native/<platform>` — what `fetch_natives` populates.
Platforms: `macos_arm64`, `linux_x86_64`, `linux_arm64`.

## Notes

- **Backend:** CPU by default. GPU (Metal on macOS) is not loaded: the
  accelerator is looked up as a `.framework` bundle path that isn't present
  here; roadmap.
- **Logs:** the native engine writes diagnostics to stderr; redirect with
  `2>/dev/null` if noisy.
- **Streaming:** `ask` is blocking; a streaming API is on the roadmap.
- Status: **working** (verified with Gemma-4 E2B on macOS arm64, CPU). Linux is
  wired; run on a real Linux host to confirm.

## Acknowledgements

All Rust code in this crate is original. It stands on:

- **LiteRT-LM** (© Google, Apache-2.0) — the inference engine and its C API.
- **[flutter_gemma](https://github.com/DenisovAV/flutter_gemma)** by Sasha
  Denisov — the source of the patched C API header (`native/engine.h`) and of
  the prebuilt native binaries this crate loads.

See `LICENSE`.
