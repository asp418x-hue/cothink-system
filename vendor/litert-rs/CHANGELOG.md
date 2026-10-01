# Changelog

## 0.1.2

- Acknowledge the sources: LiteRT-LM (Google, Apache-2.0) and the flutter_gemma
  project (Sasha Denisov) for the patched C API header and prebuilt binaries.

## 0.1.1

- MSRV set to Rust 1.95 (the toolchain the crate is built and verified with).
- Attribution names only LiteRT-LM (Google, Apache-2.0); removed unrelated
  references. Dropped the third-party cache fallback in native lib resolution.

## 0.1.0

- First working release.
- `LiteRtModel::load / ask`: on-device inference of `.litertlm` models over FFI
  to the native LiteRT-LM engine (via the conversation API, chat template applied).
- `cargo run --bin fetch_natives`: downloads the native libraries from this
  project's own SHA256-verified GitHub release into `./native/<platform>`.
- CPU backend. Platforms: macOS arm64, Linux x86_64/arm64.
- FFI bindings generated with bindgen from the LiteRT-LM C API; native libraries
  built from LiteRT-LM (Apache-2.0).
