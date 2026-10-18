//! Everything that can be downloaded for offline dictation, plus the
//! install/delete bookkeeping. Portable: the engines that run the models
//! live in [`crate::transcription`], the transfer in [`crate::download`].

use std::path::{Path, PathBuf};

/// The inference engine behind an offline model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LocalEngine {
    /// whisper.cpp (`ggml-*.bin` files).
    Whisper,
    Moonshine,
    Canary,
    Qwen3Asr,
}

impl LocalEngine {
    /// Heading of the engine's group on the models page.
    pub fn group_title(self) -> &'static str {
        match self {
            Self::Whisper => "OpenAI Whisper",
            Self::Qwen3Asr => "Qwen3-ASR",
            Self::Canary => "NVIDIA Canary",
            Self::Moonshine => "Moonshine",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LocalModel {
    /// Whisper: the ggml file name. Others: the folder name inside the models folder.
    pub id: &'static str,
    pub title: &'static str,
    pub description: &'static str,
    pub engine: LocalEngine,
    /// Direct file (Whisper) or a `.tar.bz2` archive (everything else).
    pub url: &'static str,
    /// Short language note for the UI.
    pub languages: &'static str,
    /// Approximate download size in MB; 0 when unknown.
    pub size_mb: u64,
}

impl LocalModel {
    pub fn is_archive(&self) -> bool {
        self.url.ends_with(".tar.bz2")
    }

    /// Approximate download size in bytes; 0 when unknown.
    pub fn size_bytes(&self) -> u64 {
        self.size_mb * 1024 * 1024
    }
}

macro_rules! whisper {
    ($file:literal, $title:literal, $description:literal, $languages:literal, $mb:literal) => {
        LocalModel {
            id: $file,
            title: $title,
            description: $description,
            engine: LocalEngine::Whisper,
            url: concat!("https://huggingface.co/ggerganov/whisper.cpp/resolve/main/", $file),
            languages: $languages,
            size_mb: $mb,
        }
    };
}

macro_rules! sherpa {
    ($id:literal, $title:literal, $description:literal, $engine:expr, $languages:literal) => {
        LocalModel {
            id: $id,
            title: $title,
            description: $description,
            engine: $engine,
            url: concat!("https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/", $id, ".tar.bz2"),
            languages: $languages,
            size_mb: 0,
        }
    };
}

/// The download catalog, grouped by engine in display order.
pub const ALL: &[LocalModel] = &[
    whisper!("ggml-tiny.bin", "Whisper Tiny", "Fastest, lowest accuracy.", "99 languages", 75),
    whisper!("ggml-base.bin", "Whisper Base", "Good balance for short dictation.", "99 languages", 142),
    whisper!("ggml-small.bin", "Whisper Small", "Noticeably more accurate.", "99 languages", 466),
    whisper!("ggml-medium.bin", "Whisper Medium", "High accuracy, needs a fast CPU or GPU.", "99 languages", 1533),
    whisper!(
        "ggml-large-v3-turbo-q5_0.bin",
        "Whisper Large v3 Turbo (Q5)",
        "Near large-v3 quality at a fraction of the size.",
        "99 languages",
        574
    ),
    whisper!(
        "ggml-large-v3-turbo.bin",
        "Whisper Large v3 Turbo",
        "Best speed/quality trade-off for a GPU.",
        "99 languages",
        1624
    ),
    whisper!("ggml-large-v3.bin", "Whisper Large v3", "Highest Whisper accuracy, slowest.", "99 languages", 3095),
    whisper!("ggml-base.en.bin", "Whisper Base (English)", "English only.", "English", 142),
    whisper!("ggml-small.en.bin", "Whisper Small (English)", "English only, more accurate.", "English", 466),
    sherpa!(
        "sherpa-onnx-qwen3-asr-0.6B-int8-2026-03-25",
        "Qwen3-ASR 0.6B (int8)",
        "Alibaba's multilingual ASR with automatic language detection.",
        LocalEngine::Qwen3Asr,
        "~50 languages, auto-detect"
    ),
    sherpa!(
        "sherpa-onnx-nemo-canary-180m-flash-en-es-de-fr-int8",
        "NVIDIA Canary 180M Flash (int8)",
        "Fast NeMo model. Dictate in the language set under Transcription (it cannot auto-detect).",
        LocalEngine::Canary,
        "English, Spanish, German, French"
    ),
    sherpa!(
        "sherpa-onnx-moonshine-tiny-en-int8",
        "Moonshine Tiny (int8)",
        "Tiny and very fast on CPU.",
        LocalEngine::Moonshine,
        "English"
    ),
    sherpa!(
        "sherpa-onnx-moonshine-tiny-en-quantized-2026-02-27",
        "Moonshine v2 Tiny",
        "Newer Moonshine generation.",
        LocalEngine::Moonshine,
        "English"
    ),
    sherpa!(
        "sherpa-onnx-moonshine-base-en-quantized-2026-02-27",
        "Moonshine v2 Base",
        "More accurate Moonshine.",
        LocalEngine::Moonshine,
        "English"
    ),
    sherpa!(
        "sherpa-onnx-moonshine-base-es-quantized-2026-02-27",
        "Moonshine v2 Base (Spanish)",
        "",
        LocalEngine::Moonshine,
        "Spanish"
    ),
    sherpa!(
        "sherpa-onnx-moonshine-base-ja-quantized-2026-02-27",
        "Moonshine v2 Base (Japanese)",
        "",
        LocalEngine::Moonshine,
        "Japanese"
    ),
    sherpa!(
        "sherpa-onnx-moonshine-base-zh-quantized-2026-02-27",
        "Moonshine v2 Base (Chinese)",
        "",
        LocalEngine::Moonshine,
        "Chinese"
    ),
];

pub fn find(id: &str) -> Option<&'static LocalModel> {
    ALL.iter().find(|m| m.id.eq_ignore_ascii_case(id))
}

/// Whisper files end in `.bin`; everything unknown is treated as such for
/// models dropped into the folder by hand.
pub fn engine_of(id: &str) -> LocalEngine {
    find(id).map_or(LocalEngine::Whisper, |m| m.engine)
}

/// Display name of a model id (the id itself when it is not in the catalog).
pub fn title_of(id: &str) -> &str {
    find(id).map_or(id, |m| m.title)
}

pub fn path_of(models_dir: &Path, id: &str) -> PathBuf {
    models_dir.join(id)
}

pub fn is_installed(models_dir: &Path, model: &LocalModel) -> bool {
    let path = path_of(models_dir, model.id);
    if model.engine == LocalEngine::Whisper { path.is_file() } else { path.is_dir() }
}

/// Ids of everything usable in the folder, including Whisper files that are
/// not in the catalog.
pub fn installed(models_dir: &Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(models_dir) else {
        return Vec::new();
    };
    let mut ids: Vec<String> = entries
        .filter_map(Result::ok)
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|name| name.starts_with("ggml-") && Path::new(name).extension().is_some_and(|e| e == "bin"))
        .collect();
    ids.extend(
        ALL.iter()
            .filter(|m| m.engine != LocalEngine::Whisper && is_installed(models_dir, m))
            .map(|m| m.id.to_string()),
    );
    ids.sort_by_key(|id| id.to_lowercase());
    ids.dedup();
    ids
}

/// Removes a model from disk.
///
/// # Errors
/// Returns the I/O error when the files cannot be deleted.
pub fn delete(models_dir: &Path, model: &LocalModel) -> std::io::Result<()> {
    crate::transcription::unload_local();
    let path = path_of(models_dir, model.id);
    if model.engine == LocalEngine::Whisper {
        std::fs::remove_file(path)
    } else if path.is_dir() {
        std::fs::remove_dir_all(path)
    } else {
        Ok(())
    }
}

/// Downloads (and for archives, unpacks) a model.
///
/// `progress` is called a few times per second from the calling thread.
///
/// # Errors
/// Returns a user-facing message when the transfer or unpacking fails.
pub fn install(
    model: &LocalModel,
    models_dir: &Path,
    progress: &mut dyn FnMut(crate::download::Progress),
) -> Result<(), String> {
    use crate::download::{self, Progress};

    std::fs::create_dir_all(models_dir).map_err(|e| e.to_string())?;
    let target = path_of(models_dir, model.id);
    let part = models_dir.join(format!("{}.part", model.id));
    let result = (|| {
        download::fetch(model.url, &part, model.size_bytes(), progress)?;
        if model.is_archive() {
            progress(Progress::Unpacking);
            download::extract_tar_bz2(&part, &target)?;
            let _ = std::fs::remove_file(&part);
        } else {
            std::fs::rename(&part, &target).map_err(|e| e.to_string())?;
        }
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&part);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    const WHISPER_BASE: &str = "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/";
    const SHERPA_BASE: &str = "https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/";

    #[test]
    fn catalog_urls_use_the_documented_hosts() {
        for model in ALL {
            let base = if model.engine == LocalEngine::Whisper { WHISPER_BASE } else { SHERPA_BASE };
            assert!(model.url.starts_with(base), "{}", model.id);
            assert_eq!(model.is_archive(), model.engine != LocalEngine::Whisper, "{}", model.id);
        }
    }

    #[test]
    fn ids_are_unique_and_found_case_insensitively() {
        let mut ids: Vec<_> = ALL.iter().map(|m| m.id.to_lowercase()).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), ALL.len());
        assert_eq!(find("GGML-TINY.BIN").map(|m| m.title), Some("Whisper Tiny"));
        assert_eq!(engine_of("sherpa-onnx-moonshine-tiny-en-int8"), LocalEngine::Moonshine);
        assert_eq!(engine_of("ggml-custom.bin"), LocalEngine::Whisper);
        assert_eq!(title_of("ggml-custom.bin"), "ggml-custom.bin");
    }

    #[test]
    fn lists_installed_models_including_unknown_whisper_files() {
        let dir = std::env::temp_dir().join(format!("voiceink-models-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(dir.join("sherpa-onnx-moonshine-tiny-en-int8")).unwrap();
        std::fs::create_dir_all(dir.join("sherpa-onnx-nemo-canary-180m-flash-en-es-de-fr-int8")).unwrap();
        std::fs::write(dir.join("ggml-custom.bin"), b"x").unwrap();
        std::fs::write(dir.join("notes.txt"), b"x").unwrap();
        assert_eq!(
            installed(&dir),
            vec![
                "ggml-custom.bin",
                "sherpa-onnx-moonshine-tiny-en-int8",
                "sherpa-onnx-nemo-canary-180m-flash-en-es-de-fr-int8"
            ]
        );
        assert!(!is_installed(&dir, find("ggml-tiny.bin").unwrap()));
        assert!(is_installed(&dir, find("sherpa-onnx-moonshine-tiny-en-int8").unwrap()));
        std::fs::remove_dir_all(dir).unwrap();
    }
}
