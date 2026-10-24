//! Offline transcription with whisper.cpp (the macOS app's `LocalTranscriptionService`).
//!
//! Models are the standard `ggml-*.bin` files from
//! <https://huggingface.co/ggerganov/whisper.cpp>. The loaded context is cached
//! between dictations because loading a model takes far longer than decoding.

use super::{Transcriber, TranscriptionError, TranscriptionRequest, non_empty};
use std::ffi::c_void;
use std::num::NonZero;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

/// Loaded model, kept across dictations (shared by all `LocalWhisper` instances).
static CONTEXT: Mutex<Option<(PathBuf, WhisperContext)>> = Mutex::new(None);
const MAX_THREADS: usize = 8;

/// Frees the loaded model (before deleting its file).
pub fn unload() {
    if let Ok(mut guard) = CONTEXT.lock() {
        *guard = None;
    }
}

pub struct LocalWhisper {
    model_path: PathBuf,
}

impl LocalWhisper {
    pub fn new(model_path: PathBuf) -> Self {
        Self { model_path }
    }
}

impl Transcriber for LocalWhisper {
    fn describe(&self) -> String {
        let name = self.model_path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        format!("Local {name}")
    }

    fn transcribe(&self, request: &TranscriptionRequest<'_>) -> Result<String, TranscriptionError> {
        let text = run(&self.model_path, request).map_err(TranscriptionError::Local)?;
        non_empty(&text)
    }
}

/// whisper.cpp polls this between compute steps; a `true` result stops decoding.
unsafe extern "C" fn canceled(flag: *mut c_void) -> bool {
    // SAFETY: `run` passes a pointer to the request's `AtomicBool`, which
    // outlives the `full` call that invokes this callback.
    unsafe { (*flag.cast::<AtomicBool>()).load(Ordering::SeqCst) }
}

fn run(model_path: &Path, request: &TranscriptionRequest<'_>) -> Result<String, String> {
    if !model_path.exists() {
        return Err(format!("model not found: {} (download a ggml model into this folder)", model_path.display()));
    }

    let mut guard = CONTEXT.lock().map_err(|_| "whisper context poisoned".to_string())?;
    if guard.as_ref().map(|(p, _)| p.as_path()) != Some(model_path) {
        *guard = None;
        let started = Instant::now();
        let ctx = WhisperContext::new_with_params(model_path, WhisperContextParameters::default())
            .map_err(|e| format!("failed to load {}: {e}", model_path.display()))?;
        log::info!("loaded {} in {:.1}s", model_path.display(), started.elapsed().as_secs_f64());
        *guard = Some((model_path.to_path_buf(), ctx));
    }
    let (_, ctx) = guard.as_ref().expect("context loaded above");
    let mut state = ctx.create_state().map_err(|e| e.to_string())?;

    let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
    let threads = std::thread::available_parallelism().map_or(4, NonZero::get).min(MAX_THREADS);
    params.set_n_threads(i32::try_from(threads).unwrap_or(4));
    params.set_language(Some(request.language.unwrap_or("auto")));
    params.set_translate(false);
    params.set_no_context(true);
    params.set_suppress_blank(true);
    params.set_print_special(false);
    params.set_print_progress(false);
    params.set_print_realtime(false);
    params.set_print_timestamps(false);
    let prompt = request.vocabulary.iter().map(|w| w.trim()).filter(|w| !w.is_empty()).collect::<Vec<_>>().join(", ");
    if !prompt.is_empty() {
        params.set_initial_prompt(&prompt);
    }

    // SAFETY: the flag is borrowed for the whole of `run`, so it outlives `full`.
    unsafe {
        params.set_abort_callback(Some(canceled));
        params.set_abort_callback_user_data(std::ptr::from_ref(request.cancel).cast_mut().cast());
    }

    let started = Instant::now();
    let result = state.full(params, request.samples);
    if request.cancel.load(Ordering::SeqCst) {
        return Err("canceled".into());
    }
    result.map_err(|e| e.to_string())?;
    log::info!(
        "decoded {:.1}s of audio in {:.1}s on {threads} threads",
        crate::audio::duration_secs(request.samples.len(), crate::audio::TARGET_SAMPLE_RATE),
        started.elapsed().as_secs_f64()
    );

    let mut text = String::new();
    for segment in state.as_iter() {
        let s = segment.to_str_lossy().map_err(|e| e.to_string())?;
        text.push_str(&s);
    }
    Ok(text.trim().to_string())
}
