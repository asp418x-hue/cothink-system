use thiserror::Error;

/// Errors returned by litert-rs.
#[derive(Debug, Error)]
pub enum LiteRtError {
    /// The current OS/arch is not supported.
    #[error("unsupported platform")]
    Unsupported,

    /// Native libraries were not found for the current platform.
    #[error(
        "native libraries not found for {0}; run `cargo run --bin fetch_natives` \
         or set LITERTLM_LIB_DIR to the directory with the .dylib/.so files"
    )]
    NativeLibsMissing(String),

    /// A native library failed to load (dlopen).
    #[error("failed to load native library: {0}")]
    Load(String),

    /// The engine could not be created (invalid model, unavailable backend, or
    /// broken native libraries).
    #[error("failed to create engine: {0}")]
    EngineCreate(String),

    /// A session could not be created for the engine.
    #[error("failed to create session")]
    SessionCreate,

    /// Content generation failed.
    #[error("generation failed")]
    Generation,

    /// The prompt contained an interior NUL byte and cannot cross the FFI.
    #[error("invalid prompt (contains a NUL byte)")]
    InvalidPrompt,
}
