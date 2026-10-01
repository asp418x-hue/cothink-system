use crate::error::LiteRtError;
use std::env;

/// Binary subdirectory for the current platform/ABI, matching the official
/// LiteRT-LM release archive names.
pub fn platform_dir() -> Result<&'static str, LiteRtError> {
    if cfg!(target_os = "macos") {
        Ok("macos_arm64")
    } else if cfg!(target_os = "linux") {
        if cfg!(target_arch = "aarch64") {
            Ok("linux_arm64")
        } else {
            Ok("linux_x86_64")
        }
    } else {
        Err(LiteRtError::Unsupported)
    }
}

/// Absolute directory holding the LiteRT-LM `.dylib`/`.so` files.
///
/// Resolution order:
///   1. `LITERTLM_LIB_DIR` environment variable (always wins).
///   2. `<cwd>/native/<platform>` — what `fetch_natives` populates.
pub fn native_lib_dir() -> Result<String, LiteRtError> {
    if let Ok(dir) = env::var("LITERTLM_LIB_DIR") {
        if !dir.is_empty() {
            return Ok(dir);
        }
    }

    let platform = platform_dir()?;

    if let Ok(cwd) = env::current_dir() {
        let local = cwd.join("native").join(platform);
        if local.is_dir() {
            return Ok(local.to_string_lossy().into_owned());
        }
    }

    Err(LiteRtError::NativeLibsMissing(platform.to_string()))
}
