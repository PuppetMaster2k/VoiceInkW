//! Offline models: download, use and delete (Whisper, Moonshine, Canary,
//! Qwen3-ASR). Downloads run on their own thread and report back through a
//! stream, so the window stays responsive and the page can be left meanwhile.

use super::super::style::{self, LABEL_WIDTH, hint};
use super::form::{Env, Status};
use crate::config::{Config, TranscriptionProvider};
use crate::download::Progress;
use crate::local_models::{self, LocalEngine, LocalModel};
use iced::futures::channel::mpsc;
use iced::futures::{SinkExt, StreamExt};
use iced::widget::{button, column, container, progress_bar, row, space, text};
use iced::{Element, Fill, Length, Task};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

const STREAM_BUFFER: usize = 32;
const MB: f64 = 1024.0 * 1024.0;

#[derive(Debug, Default)]
pub struct Models {
    /// Models being downloaded or unpacked, by id.
    busy: HashMap<&'static str, Progress>,
}

#[derive(Debug, Clone)]
pub enum Message {
    Download(&'static str),
    Progress(&'static str, Progress),
    Finished(&'static str, Result<(), String>),
    Use(&'static str),
    Delete(&'static str),
    OpenFolder,
}

/// What the window has to do after a [`Models`] update.
pub enum Reaction {
    None,
    /// Start [`download`]ing.
    Download(&'static LocalModel, PathBuf),
    /// Make the model the active local one.
    Use(&'static str),
    /// The set of installed models changed.
    InstalledChanged(Option<Status>),
}

/// The folder the local provider reads models from.
pub fn models_dir(config: &Config, env: Env<'_>) -> PathBuf {
    config.transcription.local_models_dir.clone().unwrap_or_else(|| env.app.paths.models())
}

impl Models {
    pub fn update(&mut self, message: Message, config: &Config, env: Env<'_>) -> Reaction {
        let dir = models_dir(config, env);
        match message {
            Message::Download(id) => {
                let Some(model) = local_models::find(id) else { return Reaction::None };
                if self.busy.contains_key(id) {
                    return Reaction::None;
                }
                self.busy.insert(id, Progress::Downloading { fraction: 0.0, bytes_per_second: 0.0 });
                Reaction::Download(model, dir)
            }
            Message::Progress(id, progress) => {
                if let Some(slot) = self.busy.get_mut(id) {
                    *slot = progress;
                }
                Reaction::None
            }
            Message::Finished(id, result) => {
                self.busy.remove(id);
                let title = local_models::title_of(id);
                Reaction::InstalledChanged(Some(match result {
                    Ok(()) => Status::info(format!("{title} is ready.")),
                    Err(e) => Status::error(format!("Download of {title} failed: {e}")),
                }))
            }
            Message::Use(id) => Reaction::Use(id),
            Message::Delete(id) => {
                let Some(model) = local_models::find(id) else { return Reaction::None };
                Reaction::InstalledChanged(Some(match local_models::delete(&dir, model) {
                    Ok(()) => Status::info(format!("Deleted {}.", model.title)),
                    Err(e) => Status::error(format!("Could not delete {}: {e}", model.title)),
                }))
            }
            Message::OpenFolder => {
                let _ = std::fs::create_dir_all(&dir);
                env.desktop.open_path(&dir);
                Reaction::None
            }
        }
    }

    pub fn view<'a>(&'a self, config: &'a Config, env: Env<'_>) -> Element<'a, Message> {
        let dir = models_dir(config, env);
        let active =
            (config.transcription.provider == TranscriptionProvider::Local).then(|| config.transcription.model_name());

        let mut page = column![hint(
            "Models run on your PC. Nothing leaves your device. Whisper uses whisper.cpp; the others run on ONNX Runtime."
        )]
        .spacing(style::SECTION_SPACING);
        for engine in [LocalEngine::Whisper, LocalEngine::Qwen3Asr, LocalEngine::Canary, LocalEngine::Moonshine] {
            let rows = local_models::ALL
                .iter()
                .filter(|m| m.engine == engine)
                .map(|m| self.model_row(m, &dir, active.as_deref()));
            page =
                page.push(text(engine.group_title()).size(17).font(style::semibold())).push(column(rows).spacing(10));
        }
        page.push(text("Storage").size(17).font(style::semibold()))
            .push(
                row![
                    column![text("Models folder"), hint(dir.display().to_string())].width(Fill),
                    button("Open").on_press(Message::OpenFolder).style(button::secondary),
                ]
                .align_y(iced::Center),
            )
            .into()
    }

    #[allow(clippy::cast_possible_truncation)] // a progress fraction fits f32
    fn model_row<'a>(&'a self, model: &'static LocalModel, dir: &Path, active: Option<&str>) -> Element<'a, Message> {
        let in_use = active.is_some_and(|a| a.eq_ignore_ascii_case(model.id));
        let title = if in_use { format!("{}  ·  in use", model.title) } else { model.title.to_string() };
        let size = if model.size_mb > 0 { format!(", {} MB", model.size_mb) } else { String::new() };
        let note = [model.description.to_string(), format!("({}{size})", model.languages)]
            .into_iter()
            .filter(|t| !t.is_empty())
            .collect::<Vec<_>>()
            .join("  ");

        let actions: Element<'a, Message> = if let Some(progress) = self.busy.get(model.id) {
            let (label, fraction) = match *progress {
                Progress::Downloading { fraction, bytes_per_second } => (
                    format!("{:.0}%  ·  {:.1} MB/s", fraction * 100.0, bytes_per_second / MB),
                    fraction.clamp(0.0, 1.0) as f32,
                ),
                Progress::Unpacking => ("Unpacking…".to_string(), 1.0),
            };
            row![
                text(label).size(13).width(Length::Fixed(LABEL_WIDTH - 60.0)),
                container(progress_bar(0.0..=1.0, fraction)).width(Length::Fixed(140.0)).height(8),
            ]
            .spacing(8)
            .align_y(iced::Center)
            .into()
        } else if local_models::is_installed(dir, model) {
            let use_button = button("Use").on_press(Message::Use(model.id));
            row![
                if in_use { use_button.style(button::secondary) } else { use_button.style(button::primary) },
                button("Delete").on_press(Message::Delete(model.id)).style(button::secondary),
            ]
            .spacing(8)
            .into()
        } else {
            button("Download").on_press(Message::Download(model.id)).style(button::primary).into()
        };

        row![column![text(title), hint(note)].spacing(2).width(Fill), space().width(12), actions]
            .align_y(iced::Center)
            .into()
    }
}

/// Downloads and installs `model`, reporting [`Message::Progress`] a few
/// times per second and [`Message::Finished`] at the end.
pub fn download(model: &'static LocalModel, dir: PathBuf) -> Task<Message> {
    Task::stream(iced::stream::channel(STREAM_BUFFER, async move |mut out: mpsc::Sender<Message>| {
        let (events, mut inbox) = mpsc::unbounded::<Message>();
        std::thread::spawn(move || {
            let progress = events.clone();
            let result = local_models::install(model, &dir, &mut |p| {
                let _ = progress.unbounded_send(Message::Progress(model.id, p));
            });
            let _ = events.unbounded_send(Message::Finished(model.id, result));
        });
        while let Some(message) = inbox.next().await {
            let finished = matches!(message, Message::Finished(..));
            if out.send(message).await.is_err() || finished {
                break;
            }
        }
    }))
}
