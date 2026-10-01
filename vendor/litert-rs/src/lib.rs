//! On-device inference of LiteRT-LM (`.litertlm`) models in Rust, over FFI to
//! the native LiteRT-LM engine.
//!
//! ```no_run
//! use litert_rs::LiteRtModel;
//!
//! let model = LiteRtModel::load("/path/gemma-4-E2B-it.litertlm", "cpu", 2048)?;
//! let answer = model.ask("What is 2 + 2? Answer briefly.")?;
//! println!("{answer}");
//! # Ok::<(), litert_rs::LiteRtError>(())
//! ```
//!
//! Inference is performed by the native LiteRT-LM (C++) engine; this crate only
//! orchestrates over FFI. Download the native libraries once with
//! `cargo run --bin fetch_natives` (or set `LITERTLM_LIB_DIR`).

mod bindings;
mod error;
mod loader;
mod model;
mod native_libs;
mod response;

pub use error::LiteRtError;
pub use model::LiteRtModel;
pub use native_libs::{native_lib_dir, platform_dir};
