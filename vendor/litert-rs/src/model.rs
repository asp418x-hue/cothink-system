use crate::bindings::*;
use crate::error::LiteRtError;
use crate::loader::{self, NativeLibs};
use std::ffi::{CStr, CString};

/// A LiteRT-LM (`.litertlm`) model loaded in memory for on-device inference.
///
/// Inference is done by the native engine, called over FFI. Each [`ask`] call
/// creates a fresh session (no context carried over). The engine allows a
/// single live session at a time, so this type is **not** safe to call
/// concurrently from multiple threads — wrap it in a `Mutex` if you need that.
///
/// [`ask`]: LiteRtModel::ask
pub struct LiteRtModel {
    libs: NativeLibs,
    engine: *mut LiteRtLmEngine,
    model_path: String,
    backend: String,
    max_tokens: i32,
    max_output_tokens: i32,
}

impl LiteRtModel {
    /// Loads a `.litertlm` model into memory.
    ///
    /// - `model_path`: absolute path to the `.litertlm` file.
    /// - `backend`: `"cpu"` (always available) or `"gpu"`.
    /// - `max_tokens`: context window size (KV cache).
    ///
    /// Native binaries are resolved via `LITERTLM_LIB_DIR` or
    /// `./native/<platform>` (run `fetch_natives` to populate it).
    pub fn load(model_path: &str, backend: &str, max_tokens: i32) -> Result<Self, LiteRtError> {
        let libs = loader::load()?;

        let c_model = CString::new(model_path)
            .map_err(|_| LiteRtError::EngineCreate("model path has NUL".into()))?;
        let c_backend = CString::new(backend)
            .map_err(|_| LiteRtError::EngineCreate("backend has NUL".into()))?;

        let engine = unsafe {
            let settings = libs.lib.litert_lm_engine_settings_create(
                c_model.as_ptr(),
                c_backend.as_ptr(),
                std::ptr::null(),
                std::ptr::null(),
            );
            if settings.is_null() {
                return Err(LiteRtError::EngineCreate(
                    "settings creation returned null".into(),
                ));
            }
            libs.lib
                .litert_lm_engine_settings_set_max_num_tokens(settings, max_tokens);
            let engine = libs.lib.litert_lm_engine_create(settings);
            libs.lib.litert_lm_engine_settings_delete(settings);
            engine
        };

        if engine.is_null() {
            return Err(LiteRtError::EngineCreate(format!(
                "engine creation returned null for '{model_path}' (backend={backend})"
            )));
        }

        Ok(Self {
            libs,
            engine,
            model_path: model_path.to_string(),
            backend: backend.to_string(),
            max_tokens,
            max_output_tokens: 1024,
        })
    }

    /// Absolute path of the loaded model.
    pub fn model_path(&self) -> &str {
        &self.model_path
    }

    /// Compute backend in use (`"cpu"` or `"gpu"`).
    pub fn backend(&self) -> &str {
        &self.backend
    }

    /// Context window (tokens) the engine was created with.
    pub fn max_tokens(&self) -> i32 {
        self.max_tokens
    }

    /// Answers `prompt` and returns the generated text (blocking).
    ///
    /// Uses the conversation API, which applies the model's chat template from
    /// the message roles (as the reference implementation does).
    pub fn ask(&self, prompt: &str) -> Result<String, LiteRtError> {
        let message_json = crate::response::build_user_message(prompt);
        let c_message = CString::new(message_json).map_err(|_| LiteRtError::InvalidPrompt)?;

        unsafe {
            // Session config: sampler + output cap. LiteRT-LM only implements
            // TopP at engine level, so use type TopP and pass top_k as a hint.
            let session_config = self.libs.lib.litert_lm_session_config_create();
            if session_config.is_null() {
                return Err(LiteRtError::SessionCreate);
            }
            let sampler = LiteRtLmSamplerParams {
                type_: LiteRtLmSamplerType_kLiteRtLmSamplerTypeTopP,
                top_k: 40,
                top_p: 0.95,
                temperature: 0.8,
                seed: 1,
            };
            self.libs
                .lib
                .litert_lm_session_config_set_sampler_params(session_config, &sampler);
            self.libs
                .lib
                .litert_lm_session_config_set_max_output_tokens(
                    session_config,
                    self.max_output_tokens,
                );

            // Conversation config owns what it needs from session_config, so we
            // delete session_config right after creating it.
            let conv_config = self.libs.lib.litert_lm_conversation_config_create(
                self.engine,
                session_config,
                std::ptr::null(),
                std::ptr::null(),
                std::ptr::null(),
                false,
            );
            self.libs
                .lib
                .litert_lm_session_config_delete(session_config);
            if conv_config.is_null() {
                return Err(LiteRtError::SessionCreate);
            }

            let conversation = self
                .libs
                .lib
                .litert_lm_conversation_create(self.engine, conv_config);
            self.libs
                .lib
                .litert_lm_conversation_config_delete(conv_config);
            if conversation.is_null() {
                return Err(LiteRtError::SessionCreate);
            }

            let response = self.libs.lib.litert_lm_conversation_send_message(
                conversation,
                c_message.as_ptr(),
                std::ptr::null(),
                std::ptr::null(),
            );
            if response.is_null() {
                self.libs.lib.litert_lm_conversation_delete(conversation);
                return Err(LiteRtError::Generation);
            }

            let json_ptr = self.libs.lib.litert_lm_json_response_get_string(response);
            let out = if json_ptr.is_null() {
                String::new()
            } else {
                let raw = CStr::from_ptr(json_ptr).to_string_lossy().into_owned();
                crate::response::extract_text(&raw)
            };

            self.libs.lib.litert_lm_json_response_delete(response);
            self.libs.lib.litert_lm_conversation_delete(conversation);

            Ok(out.trim().to_string())
        }
    }
}

impl Drop for LiteRtModel {
    fn drop(&mut self) {
        if !self.engine.is_null() {
            unsafe { self.libs.lib.litert_lm_engine_delete(self.engine) };
        }
    }
}

// The engine owns native memory; moving it across threads is fine, concurrent
// use is not (documented on the type).
unsafe impl Send for LiteRtModel {}
