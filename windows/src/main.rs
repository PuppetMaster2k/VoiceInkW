// Release builds are GUI-subsystem apps (no console window); CLI subcommands
// re-attach to the parent console, see `platform::attach_parent_console`.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod cli;

use cli::Command;
use std::process::ExitCode;
use voiceink::context::AppContext;
use voiceink::settings_page::SettingsPage;

/// The log restarts once it grows beyond this size.
const MAX_LOG_BYTES: u64 = 2 * 1024 * 1024;

/// Level for the app's own messages: `VOICEINK_LOG=error|warn|info|debug|trace`,
/// defaulting to `debug` in debug builds and `info` in release builds.
fn log_level() -> simplelog::LevelFilter {
    use simplelog::LevelFilter;
    let default = if cfg!(debug_assertions) { LevelFilter::Debug } else { LevelFilter::Info };
    std::env::var("VOICEINK_LOG").ok().and_then(|v| v.trim().parse().ok()).unwrap_or(default)
}

fn init_logging(ctx: &AppContext) {
    use simplelog::{CombinedLogger, ConfigBuilder, LevelFilter, SharedLogger, WriteLogger};
    let path = ctx.paths.log();
    let _ = std::fs::create_dir_all(ctx.paths.root());
    if std::fs::metadata(&path).is_ok_and(|m| m.len() > MAX_LOG_BYTES) {
        let _ = std::fs::remove_file(&path);
    }
    // Append, so a second instance (which exits immediately) keeps the log.
    let open = || std::fs::OpenOptions::new().create(true).append(true).open(&path);
    if let (Ok(ours), Ok(libraries)) = (open(), open()) {
        // Our own messages from `log_level()` up; only problems from the UI and
        // graphics libraries, which are chatty at Info.
        let ours_config = ConfigBuilder::new().set_time_format_rfc3339().add_filter_allow_str("voiceink").build();
        let libraries_config = ConfigBuilder::new().set_time_format_rfc3339().add_filter_ignore_str("voiceink").build();
        let level = log_level();
        let mut loggers: Vec<Box<dyn SharedLogger>> = vec![
            WriteLogger::new(level, ours_config.clone(), ours),
            WriteLogger::new(LevelFilter::Warn, libraries_config, libraries),
        ];
        // Debug builds also print to the debugger's console / terminal.
        if cfg!(debug_assertions) {
            loggers.push(WriteLogger::new(level, ours_config, std::io::stderr()));
        }
        let _ = CombinedLogger::init(loggers);
    }
    log_panics();
}

/// Release builds have no console, so a panic would vanish without a trace:
/// write it to the log as well.
fn log_panics() {
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let thread = std::thread::current();
        log::error!("panic on thread '{}': {info}", thread.name().unwrap_or("unnamed"));
        default_hook(info);
    }));
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let command = match cli::parse(&args) {
        Ok(command) => command,
        Err(e) => {
            eprintln!("error: {e:#}");
            return ExitCode::FAILURE;
        }
    };
    let ctx = AppContext::detect();

    match command {
        Command::App { page } => run_app(&ctx, page),
        command => {
            #[cfg(windows)]
            voiceink::platform::attach_parent_console();
            match cli::execute(command, &ctx, &mut std::io::stdout().lock()) {
                Ok(()) => ExitCode::SUCCESS,
                Err(e) => {
                    eprintln!("error: {e:#}");
                    ExitCode::FAILURE
                }
            }
        }
    }
}

#[cfg(windows)]
fn run_app(ctx: &AppContext, page: Option<SettingsPage>) -> ExitCode {
    use voiceink::platform::app;
    if page.is_some_and(app::open_settings_in_running_instance) {
        return ExitCode::SUCCESS;
    }
    init_logging(ctx);
    log::info!("VoiceInk for Windows {} starting", voiceink::APP_VERSION);
    match app::run(ctx, page) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            log::error!("fatal: {e:#}");
            voiceink::platform::show_error(&format!("{e:#}"));
            ExitCode::FAILURE
        }
    }
}

/// Elsewhere the tray app is unavailable, but `voiceink settings` /
/// `voiceink dashboard` open the (portable) settings window on its own.
#[cfg(not(windows))]
fn run_app(ctx: &AppContext, page: Option<SettingsPage>) -> ExitCode {
    use std::rc::Rc;
    use voiceink::ui::{BasicDesktop, Launch, NoHost};
    init_logging(ctx);
    let Some(page) = page else {
        eprintln!("The VoiceInk tray app only runs on Windows. CLI commands work everywhere:\n\n{}", cli::HELP);
        return ExitCode::from(2);
    };
    let launch = Launch { page: Some(page), exit_on_close: true };
    match voiceink::ui::run(ctx.clone(), Rc::new(BasicDesktop), Rc::new(NoHost), launch) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}
