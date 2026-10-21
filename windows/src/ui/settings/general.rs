//! Shortcuts, microphone, overlay, sounds and autostart.

use super::super::style::{self, FIELD_WIDTH, Labeled, field, field_with_hint, indented, selected};
use super::capture::{self, Capture, Step};
use super::form::{self, Env, KeyChanges, Status};
use crate::config::{Config, HotkeyMode, OverlayPosition};
use iced::widget::{button, checkbox, column, pick_list, row, text, text_input};
use iced::{Element, Event, Length, Subscription, event, keyboard, mouse};

const HOTKEY_MODES: [Labeled<HotkeyMode>; 3] = [
    Labeled::new(HotkeyMode::Hybrid, "Tap to toggle, hold to talk"),
    Labeled::new(HotkeyMode::Toggle, "Toggle (press to start / stop)"),
    Labeled::new(HotkeyMode::PushToTalk, "Push to talk (hold)"),
];
const OVERLAY_POSITIONS: [Labeled<OverlayPosition>; 3] = [
    Labeled::new(OverlayPosition::BottomCenter, "Bottom center"),
    Labeled::new(OverlayPosition::TopCenter, "Top center"),
    Labeled::new(OverlayPosition::Hidden, "Hidden"),
];
const SYSTEM_DEFAULT_MIC: &str = "System default";
const THRESHOLD_RANGE_MS: (u64, u64) = (100, 5000);

/// Text fields that are validated when saving.
#[derive(Debug, Default)]
pub struct General {
    record: String,
    threshold: String,
    cancel: String,
    paste_last: String,
    /// "System default" followed by the microphones (and an unplugged configured one).
    microphones: Vec<String>,
    /// Set while the next key press is being read into the record field.
    capture: Option<Capture>,
}

#[derive(Debug, Clone)]
pub enum Message {
    Record(String),
    Mode(Labeled<HotkeyMode>),
    Threshold(String),
    Cancel(String),
    PasteLast(String),
    Microphone(String),
    Overlay(Labeled<OverlayPosition>),
    Sounds(bool),
    Mute(bool),
    Autostart(bool),
    EditModes,
    /// Start or stop reading the record shortcut from the keyboard or mouse.
    ToggleCapture,
    Key(keyboard::Event),
    Mouse(mouse::Button),
}

impl General {
    pub fn new(config: &Config, env: Env<'_>) -> Self {
        let hotkeys = &config.hotkeys;
        let mut microphones = vec![SYSTEM_DEFAULT_MIC.to_string()];
        microphones.extend(env.desktop.input_devices());
        // Keep a configured device that is currently unplugged.
        if let Some(device) = configured_device(config).filter(|d| !microphones.iter().any(|m| m == d)) {
            microphones.push(device.to_string());
        }
        Self {
            record: hotkeys.record.clone(),
            threshold: hotkeys.hybrid_threshold_ms.to_string(),
            cancel: hotkeys.cancel.clone(),
            paste_last: hotkeys.paste_last.clone().unwrap_or_default(),
            microphones,
            capture: None,
        }
    }

    pub fn update(&mut self, message: Message, config: &mut Config, env: Env<'_>) -> Option<Status> {
        match message {
            Message::Record(v) => self.record = v,
            Message::Mode(mode) => config.hotkeys.mode = mode.value,
            Message::Threshold(v) => self.threshold = v,
            Message::Cancel(v) => self.cancel = v,
            Message::PasteLast(v) => self.paste_last = v,
            Message::Microphone(name) => {
                config.audio.input_device = (name != SYSTEM_DEFAULT_MIC).then_some(name);
            }
            Message::Overlay(position) => config.overlay = position.value,
            Message::Sounds(on) => config.audio.play_sounds = on,
            Message::Mute(on) => config.audio.mute_system_audio = on,
            Message::Autostart(on) => config.start_with_windows = on,
            Message::EditModes => env.desktop.open_path(&env.app.paths.config()),
            Message::ToggleCapture => self.capture = self.capture.is_none().then(Capture::default),
            Message::Key(event) => {
                if let Some(step) = self.capture.as_mut().map(|c| c.key(&event)) {
                    self.finish_capture(step);
                }
            }
            Message::Mouse(button) => {
                if let Some(name) = self.capture.as_ref().and_then(|_| capture::mouse_name(button)) {
                    self.finish_capture(Step::Done(name.to_string()));
                }
            }
        }
        None
    }

    fn finish_capture(&mut self, step: Step) {
        match step {
            Step::Wait => {}
            Step::Cancel => self.capture = None,
            Step::Done(spec) => {
                self.record = spec;
                self.capture = None;
            }
        }
    }

    /// Key and mouse input, only while a shortcut is being captured.
    pub fn subscription(&self) -> Subscription<Message> {
        if self.capture.is_none() {
            return Subscription::none();
        }
        event::listen_with(|event, _, _| match event {
            Event::Keyboard(event) => Some(Message::Key(event)),
            Event::Mouse(mouse::Event::ButtonPressed(button)) => Some(Message::Mouse(button)),
            _ => None,
        })
    }

    pub fn apply(&self, config: &mut Config, _keys: &mut KeyChanges) -> Result<(), String> {
        let hotkeys = &mut config.hotkeys;
        hotkeys.record = form::shortcut(&self.record, "Record", false)?;
        hotkeys.hybrid_threshold_ms =
            form::number(&self.threshold, "Hold threshold")?.clamp(THRESHOLD_RANGE_MS.0, THRESHOLD_RANGE_MS.1);
        hotkeys.cancel = form::shortcut(&self.cancel, "Cancel", true)?;
        hotkeys.paste_last = form::optional(&form::shortcut(&self.paste_last, "Paste-last", true)?);
        Ok(())
    }

    pub fn view<'a>(&'a self, config: &'a Config) -> Element<'a, Message> {
        let mic = configured_device(config).unwrap_or(SYSTEM_DEFAULT_MIC).to_string();
        let modes: Vec<&str> = config.modes.iter().map(|m| m.name.as_str()).collect();
        let modes = if modes.is_empty() { "No modes configured".to_string() } else { modes.join(", ") };
        column![
            field_with_hint(
                "Record shortcut",
                row![
                    text_input("RightCtrl", &self.record).on_input(Message::Record).width(200),
                    button(if self.capture.is_some() { "Press a key… (Esc cancels)" } else { "Press to set" })
                        .on_press(Message::ToggleCapture)
                        .style(button::secondary),
                ]
                .spacing(8)
                .align_y(iced::Center),
                "Type it, or click \"Press to set\" and press the key, combination or mouse button. \
                 Examples: RightCtrl, F9, Ctrl+Shift+Space, Mouse4",
            ),
            field(
                "Shortcut behaviour",
                pick_list(HOTKEY_MODES, selected(&HOTKEY_MODES, config.hotkeys.mode), Message::Mode)
                    .width(Length::Fixed(FIELD_WIDTH)),
            ),
            field("Hold threshold (ms)", text_input("500", &self.threshold).on_input(Message::Threshold).width(100)),
            field("Cancel shortcut", text_input("Escape", &self.cancel).on_input(Message::Cancel).width(200)),
            field(
                "Paste last transcription",
                text_input("Alt+Shift+V", &self.paste_last).on_input(Message::PasteLast).width(200),
            ),
            field(
                "Microphone",
                pick_list(self.microphones.as_slice(), Some(mic), Message::Microphone).width(Length::Fill),
            ),
            field(
                "Recorder overlay",
                pick_list(OVERLAY_POSITIONS, selected(&OVERLAY_POSITIONS, config.overlay), Message::Overlay).width(200),
            ),
            indented(checkbox(config.audio.play_sounds).label("Play start / stop sounds").on_toggle(Message::Sounds)),
            indented(
                checkbox(config.audio.mute_system_audio)
                    .label("Mute system audio while recording")
                    .on_toggle(Message::Mute)
            ),
            indented(
                checkbox(config.start_with_windows).label("Start VoiceInk with Windows").on_toggle(Message::Autostart)
            ),
            field(
                "Modes",
                row![
                    text(modes).width(Length::Fill),
                    button("Edit modes in config.json").on_press(Message::EditModes).style(button::secondary),
                ]
                .spacing(8)
                .align_y(iced::Center),
            ),
        ]
        .spacing(style::SECTION_SPACING)
        .into()
    }
}

fn configured_device(config: &Config) -> Option<&str> {
    config.audio.input_device.as_deref().filter(|d| !d.trim().is_empty())
}
