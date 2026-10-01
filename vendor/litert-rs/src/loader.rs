use crate::bindings::LiteRtLm;
use crate::error::LiteRtError;
use crate::native_libs::native_lib_dir;

#[cfg(unix)]
use libloading::os::unix::{Library as UnixLibrary, RTLD_GLOBAL, RTLD_NOW};

/// The loaded native engine plus the sibling libraries it depends on. The
/// preloaded handles are kept alive for the lifetime of the model so dyld does
/// not unload them.
pub struct NativeLibs {
    pub lib: LiteRtLm,
    #[cfg(unix)]
    _preloaded: Vec<UnixLibrary>,
}

/// Loads `libLiteRtLm` and its siblings from [`native_lib_dir`].
///
/// `libLiteRtLm` has an `@rpath`/SONAME dependency on the constraint provider
/// and `dlopen`s the GPU accelerator by basename, so we preload the siblings
/// with `RTLD_GLOBAL` (by absolute path) first; dyld then resolves those
/// dependencies against the already-loaded, globally-visible images.
#[cfg(unix)]
pub fn load() -> Result<NativeLibs, LiteRtError> {
    let dir = native_lib_dir()?;

    let siblings: &[&str] = if cfg!(target_os = "macos") {
        &[
            "libGemmaModelConstraintProvider.dylib",
            "libLiteRtMetalAccelerator.dylib",
            "libStreamProxy.dylib",
        ]
    } else {
        &[
            "libStreamProxy.so",
            "libLiteRt.so",
            "libGemmaModelConstraintProvider.so",
            "libLiteRtWebGpuAccelerator.so",
        ]
    };

    let mut preloaded = Vec::with_capacity(siblings.len() + 1);
    for name in siblings {
        let path = format!("{dir}/{name}");
        let handle = unsafe { UnixLibrary::open(Some(&path), RTLD_NOW | RTLD_GLOBAL) }
            .map_err(|e| LiteRtError::Load(format!("{path}: {e}")))?;
        preloaded.push(handle);
    }

    let main = if cfg!(target_os = "macos") {
        format!("{dir}/libLiteRtLm.dylib")
    } else {
        format!("{dir}/libLiteRtLm.so")
    };

    // Preload the main library globally too (so accelerator plugins can resolve
    // LiteRt* symbols via the global table), then hand its path to the bindgen
    // wrapper which opens its own handle for symbol lookup.
    let main_handle = unsafe { UnixLibrary::open(Some(&main), RTLD_NOW | RTLD_GLOBAL) }
        .map_err(|e| LiteRtError::Load(format!("{main}: {e}")))?;
    preloaded.push(main_handle);

    let lib =
        unsafe { LiteRtLm::new(&main) }.map_err(|e| LiteRtError::Load(format!("{main}: {e}")))?;

    Ok(NativeLibs {
        lib,
        _preloaded: preloaded,
    })
}

#[cfg(not(unix))]
pub fn load() -> Result<NativeLibs, LiteRtError> {
    Err(LiteRtError::Unsupported)
}
