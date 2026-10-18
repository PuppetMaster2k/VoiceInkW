# Repository Guidelines

## Project Structure & Module Organization

- `VoiceInk/`: macOS Swift app, organized into App, Core, Features, Infrastructure, and DesignSystem. Assets are in Assets.xcassets and Resources; UI tests are in `Tests/VoiceInkUITests/`. Open `VoiceInk.xcodeproj` for Xcode development.
- `windows/`: Rust Windows app (iced UI), with code in src and assets in resources.
- `scripts/` and `release-notes/`: release tooling and version notes.

## Build, Test, and Development Commands

Run macOS commands from the repository root with macOS 15+ and Xcode:
- `make check`: verify required tools.
- `make local`: prepare Whisper, build Release, and copy the app to ~/Downloads.
- `make dev`: build and launch the development app.

From `windows/`, with the Rust toolchain:
- `cargo test`: run tests.
- `cargo build --release`: build `target/release/voiceink.exe`.
- `cargo fmt` and `cargo clippy --all-targets -- -D warnings`: format and lint (enforced in CI).

Consult platform READMEs and `BUILDING.md` for additional setup.

## Coding Style & Naming Conventions

Match surrounding code and use four-space indentation. Use PascalCase for Swift types, camelCase for Swift methods and local variables, and Rust snake_case conventions. Format Rust changes with `cargo fmt`. Keep functions focused and explain complex logic. Keep portable logic independent of platform APIs and UI.

## Testing Guidelines

Add regression tests for behavior changes (Rust tests live next to the code; UI tests use `iced_test`). Run relevant existing tests and manually validate microphone capture, shortcuts, clipboard restoration, and UI changes. No numeric coverage threshold is specified.

## Commit & Pull Request Guidelines

Recent commits use short, action-oriented subjects, such as “Add native Windows version written in Rust”; platform prefixes such as “Windows:” also appear. Keep commits focused.

`CONTRIBUTING.md` currently states that upstream pull requests are not accepted. Use issues for reproducible bugs and feature suggestions. For fork reviews, include the change description, related issues, validation results, and screenshots for UI changes.
