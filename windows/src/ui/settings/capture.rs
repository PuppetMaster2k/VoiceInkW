//! Turns a key press into the shortcut text the Record field accepts, so the
//! user can press the shortcut instead of typing it.

use iced::keyboard::key::{Code, Physical};
use iced::keyboard::{Event, Modifiers};
use iced::mouse;

/// What a key or mouse event did to a capture in progress.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Step {
    /// Keep listening.
    Wait,
    /// Stop without changing the shortcut.
    Cancel,
    /// The shortcut that was pressed.
    Done(String),
}

/// A modifier pressed on its own only counts when it is released again
/// without another key in between, like the `RightCtrl` shortcut itself.
#[derive(Debug, Default)]
pub struct Capture {
    lone_modifier: Option<&'static str>,
}

impl Capture {
    pub fn key(&mut self, event: &Event) -> Step {
        match event {
            Event::KeyPressed { physical_key: Physical::Code(code), modifiers, repeat: false, .. } => {
                self.pressed(*code, *modifiers)
            }
            Event::KeyReleased { physical_key: Physical::Code(code), .. } => {
                match (modifier_name(*code), self.lone_modifier) {
                    (Some(name), Some(lone)) if name == lone => Step::Done(name.to_string()),
                    _ => Step::Wait,
                }
            }
            _ => Step::Wait,
        }
    }

    fn pressed(&mut self, code: Code, modifiers: Modifiers) -> Step {
        if let Some(name) = modifier_name(code) {
            // A second modifier makes this a chord, which needs a regular key too.
            self.lone_modifier = self.lone_modifier.is_none().then_some(name);
            return Step::Wait;
        }
        self.lone_modifier = None;
        if code == Code::Escape && modifiers.is_empty() {
            return Step::Cancel;
        }
        let Some(key) = key_name(code) else { return Step::Wait };
        let held = [
            (modifiers.control(), "Ctrl"),
            (modifiers.alt(), "Alt"),
            (modifiers.shift(), "Shift"),
            (modifiers.logo(), "Win"),
        ];
        let mut parts: Vec<&str> = held.iter().filter(|(on, _)| *on).map(|(_, name)| *name).collect();
        parts.push(key);
        Step::Done(parts.join("+"))
    }
}

/// The shortcut for a mouse button; left and right stay for clicking the UI.
pub fn mouse_name(button: mouse::Button) -> Option<&'static str> {
    match button {
        mouse::Button::Middle => Some("MiddleMouse"),
        mouse::Button::Back => Some("Mouse4"),
        mouse::Button::Forward => Some("Mouse5"),
        _ => None,
    }
}

fn modifier_name(code: Code) -> Option<&'static str> {
    Some(match code {
        Code::ControlLeft => "LeftCtrl",
        Code::ControlRight => "RightCtrl",
        Code::AltLeft => "LeftAlt",
        Code::AltRight => "RightAlt",
        Code::ShiftLeft => "LeftShift",
        Code::ShiftRight => "RightShift",
        Code::SuperLeft => "LeftWin",
        Code::SuperRight => "RightWin",
        _ => return None,
    })
}

fn key_name(code: Code) -> Option<&'static str> {
    const LETTERS: [&str; 26] = [
        "A", "B", "C", "D", "E", "F", "G", "H", "I", "J", "K", "L", "M", "N", "O", "P", "Q", "R", "S", "T", "U", "V",
        "W", "X", "Y", "Z",
    ];
    const DIGITS: [&str; 10] = ["0", "1", "2", "3", "4", "5", "6", "7", "8", "9"];
    const FUNCTION: [&str; 24] = [
        "F1", "F2", "F3", "F4", "F5", "F6", "F7", "F8", "F9", "F10", "F11", "F12", "F13", "F14", "F15", "F16", "F17",
        "F18", "F19", "F20", "F21", "F22", "F23", "F24",
    ];
    // The code names are `KeyA`, `Digit7` and `F9`; the rest are listed below.
    let debug = format!("{code:?}");
    if let Some(letter) = debug.strip_prefix("Key").and_then(|l| LETTERS.iter().find(|&&n| n == l)) {
        return Some(letter);
    }
    if let Some(digit) = debug.strip_prefix("Digit").and_then(|d| DIGITS.iter().find(|&&n| n == d)) {
        return Some(digit);
    }
    if let Some(function) = FUNCTION.iter().find(|&&n| n == debug) {
        return Some(function);
    }
    Some(match code {
        Code::Space => "Space",
        Code::Enter => "Enter",
        Code::Tab => "Tab",
        Code::Escape => "Escape",
        Code::Backspace => "Backspace",
        Code::Pause => "Pause",
        Code::CapsLock => "CapsLock",
        Code::ScrollLock => "ScrollLock",
        Code::PrintScreen => "PrintScreen",
        Code::Insert => "Insert",
        Code::Delete => "Delete",
        Code::Home => "Home",
        Code::End => "End",
        Code::PageUp => "PageUp",
        Code::PageDown => "PageDown",
        Code::ArrowLeft => "Left",
        Code::ArrowRight => "Right",
        Code::ArrowUp => "Up",
        Code::ArrowDown => "Down",
        Code::ContextMenu => "Menu",
        Code::Backquote => "`",
        Code::Comma => ",",
        Code::Period => ".",
        Code::Slash => "/",
        Code::Semicolon => ";",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::Hotkey;
    use iced::keyboard::{Key, Location};

    fn press(code: Code, modifiers: Modifiers) -> Event {
        Event::KeyPressed {
            key: Key::Unidentified,
            modified_key: Key::Unidentified,
            physical_key: Physical::Code(code),
            location: Location::Standard,
            modifiers,
            text: None,
            repeat: false,
        }
    }

    fn release(code: Code) -> Event {
        Event::KeyReleased {
            key: Key::Unidentified,
            modified_key: Key::Unidentified,
            physical_key: Physical::Code(code),
            location: Location::Standard,
            modifiers: Modifiers::empty(),
        }
    }

    #[test]
    fn captures_a_lone_modifier_on_release() {
        let mut capture = Capture::default();
        assert_eq!(capture.key(&press(Code::ControlRight, Modifiers::CTRL)), Step::Wait);
        assert_eq!(capture.key(&release(Code::ControlRight)), Step::Done("RightCtrl".into()));
    }

    #[test]
    fn captures_a_chord_with_its_key() {
        let mut capture = Capture::default();
        capture.key(&press(Code::ControlLeft, Modifiers::CTRL));
        capture.key(&press(Code::ShiftLeft, Modifiers::CTRL | Modifiers::SHIFT));
        let step = capture.key(&press(Code::Space, Modifiers::CTRL | Modifiers::SHIFT));
        assert_eq!(step, Step::Done("Ctrl+Shift+Space".into()));
    }

    #[test]
    fn a_modifier_released_after_a_chord_is_not_a_shortcut() {
        let mut capture = Capture::default();
        capture.key(&press(Code::ControlLeft, Modifiers::CTRL));
        capture.key(&press(Code::ShiftLeft, Modifiers::CTRL | Modifiers::SHIFT));
        assert_eq!(capture.key(&release(Code::ShiftLeft)), Step::Wait);
    }

    #[test]
    fn captures_plain_keys_and_cancels_on_escape() {
        let mut capture = Capture::default();
        assert_eq!(capture.key(&press(Code::F9, Modifiers::empty())), Step::Done("F9".into()));
        assert_eq!(capture.key(&press(Code::KeyQ, Modifiers::ALT)), Step::Done("Alt+Q".into()));
        assert_eq!(capture.key(&press(Code::Escape, Modifiers::empty())), Step::Cancel);
    }

    #[test]
    fn captured_names_parse_as_hotkeys() {
        for code in [Code::F24, Code::Digit7, Code::KeyZ, Code::PageDown, Code::Backquote, Code::ControlRight] {
            let name = key_name(code).or_else(|| modifier_name(code)).unwrap();
            assert!(name.parse::<Hotkey>().is_ok(), "{name}");
        }
        for button in [mouse::Button::Middle, mouse::Button::Back, mouse::Button::Forward] {
            assert!(mouse_name(button).unwrap().parse::<Hotkey>().is_ok());
        }
    }
}
