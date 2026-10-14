#![windows_subsystem = "windows"]

mod win;

use std::fs::{self, File, OpenOptions};
use std::ops::ControlFlow;
use std::os::windows::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread;
use std::time::{Duration, SystemTime};

use anyhow::Context;
use tray_icon::menu::{CheckMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, TrayIconBuilder};
use trcc_core::config::{self, Config};
use trcc_core::frames::{self, FrameOptions};
use trcc_core::player::{self, Command, Status};
use trcc_core::usb::UsbTransport;
use trcc_core::{Lcd, autostart};

const SCREEN: u32 = 480;
const SETTINGS_POLL: Duration = Duration::from_secs(2);
const TOOLTIP_MAX: usize = 120;
const UI_EXE: &str = "trcc-lcd-ui.exe";

type Shared<T> = Arc<Mutex<T>>;

fn main() {
    if let Err(e) = run()
        && let Ok(dir) = config::app_dir()
    {
        let _ = fs::write(dir.join("tray-error.log"), format!("{e:#}\n"));
    }
}

fn run() -> anyhow::Result<()> {
    let dir = config::app_dir()?;
    fs::create_dir_all(&dir)?;
    let Some(_lock) = single_instance(&dir) else {
        return Ok(());
    };
    let settings = dir.join("config.toml");
    if !settings.exists() {
        Config::default().save(&settings)?;
    }

    let ui = win::LoopHandle::current();
    let status: Shared<Status> = Arc::new(Mutex::new(Status::Idle));
    let settings_error: Shared<Option<String>> = Arc::new(Mutex::new(None));
    let (player_tx, player_rx) = mpsc::channel();
    let (reload_tx, reload_rx) = mpsc::channel();

    spawn_player(player_rx, Arc::clone(&status), ui)?;

    let watch_tx = player_tx.clone();
    let watch_error = Arc::clone(&settings_error);
    let watch_path = settings.clone();
    thread::Builder::new()
        .name("settings".into())
        .spawn(move || watch_settings(&watch_path, &watch_tx, &reload_rx, &watch_error, ui))?;

    let exe = std::env::current_exe()?;
    let status_item = MenuItem::new("Starting", false, None);
    let open_window = MenuItem::new("Open TRCC-LCD", true, None);
    let pause = MenuItem::new("Pause", true, None);
    let reload = MenuItem::new("Reload settings", true, None);
    let open_folder = MenuItem::new("Open settings folder", true, None);
    let start_with_windows = CheckMenuItem::new(
        "Start with Windows",
        true,
        autostart::is_enabled(&exe),
        None,
    );
    let quit = MenuItem::new("Quit", true, None);
    let menu = Menu::with_items(&[
        &status_item,
        &PredefinedMenuItem::separator(),
        &open_window,
        &pause,
        &reload,
        &open_folder,
        &start_with_windows,
        &PredefinedMenuItem::separator(),
        &quit,
    ])?;
    let tray = TrayIconBuilder::new()
        .with_menu(Box::new(menu))
        .with_tooltip("TRCC-LCD")
        .with_icon(icon()?)
        .build()
        .context("could not create the tray icon")?;

    let mut paused = false;
    let mut shown = String::new();
    win::run(|| {
        while let Ok(event) = MenuEvent::receiver().try_recv() {
            if event.id == *quit.id() {
                let _ = player_tx.send(Command::Quit);
                return ControlFlow::Break(());
            } else if event.id == *open_window.id() {
                if let Some(dir) = exe.parent() {
                    let _ = std::process::Command::new(dir.join(UI_EXE)).spawn();
                }
            } else if event.id == *pause.id() {
                paused = !paused;
                let _ = player_tx.send(if paused {
                    Command::Pause
                } else {
                    Command::Resume
                });
                pause.set_text(if paused { "Resume" } else { "Pause" });
            } else if event.id == *reload.id() {
                let _ = reload_tx.send(());
            } else if event.id == *open_folder.id() {
                let _ = std::process::Command::new("explorer").arg(&dir).spawn();
            } else if event.id == *start_with_windows.id() {
                let wanted = start_with_windows.is_checked();
                if autostart::set(&exe, wanted).is_err() {
                    start_with_windows.set_checked(!wanted);
                }
            }
        }
        let text = describe(&lock(&status), lock(&settings_error).as_deref());
        if text != shown {
            status_item.set_text(&text);
            let tip: String = format!("TRCC-LCD: {text}")
                .chars()
                .take(TOOLTIP_MAX)
                .collect();
            let _ = tray.set_tooltip(Some(tip));
            shown = text;
        }
        ControlFlow::Continue(())
    });
    Ok(())
}

fn spawn_player(
    commands: Receiver<Command>,
    status: Shared<Status>,
    ui: win::LoopHandle,
) -> std::io::Result<()> {
    thread::Builder::new()
        .name("player".into())
        .spawn(move || {
            player::run(
                || Lcd::connect(UsbTransport::open()?),
                &commands,
                player::REFRESH,
                |s| {
                    *lock(&status) = s.clone();
                    ui.wake();
                },
            );
        })?;
    Ok(())
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Holding this file open without sharing keeps a second copy from starting.
fn single_instance(dir: &Path) -> Option<File> {
    OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(false)
        .share_mode(0)
        .open(dir.join(config::TRAY_LOCK))
        .ok()
}

fn describe(status: &Status, settings_error: Option<&str>) -> String {
    if let Some(e) = settings_error {
        return format!("Settings problem: {e}");
    }
    match status {
        Status::Idle => "No picture set".into(),
        Status::Showing => "Showing picture".into(),
        Status::Paused => "Paused".into(),
        Status::Disconnected(_) => "Screen not found".into(),
    }
}

fn modified(path: &Path) -> Option<SystemTime> {
    fs::metadata(path).and_then(|m| m.modified()).ok()
}

/// Polls the settings file and the chosen image. A `stat` every few seconds
/// costs nothing, and avoids a file-watcher dependency.
fn watch_settings(
    path: &Path,
    player: &Sender<Command>,
    reload: &Receiver<()>,
    error: &Mutex<Option<String>>,
    ui: win::LoopHandle,
) {
    let mut image: Option<PathBuf> = None;
    let mut seen: Option<(Option<SystemTime>, Option<SystemTime>)> = None;
    loop {
        let stamp = (modified(path), image.as_deref().and_then(modified));
        if seen != Some(stamp) {
            // Remember the configured image even if it fails to load, so fixing
            // the file on disk is picked up without touching the settings.
            image = Config::load(path).ok().and_then(|c| c.image);
            *lock(error) = apply(path, player).err().map(|e| format!("{e:#}"));
            seen = Some((modified(path), image.as_deref().and_then(modified)));
            ui.wake();
        }
        match reload.recv_timeout(SETTINGS_POLL) {
            Ok(()) => seen = None,
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => return,
        }
    }
}

fn apply(path: &Path, player: &Sender<Command>) -> anyhow::Result<()> {
    let cfg = Config::load(path)?;
    let Some(image) = cfg.image.clone() else {
        let _ = player.send(Command::Clear);
        return Ok(());
    };
    let opts = FrameOptions {
        width: SCREEN,
        height: SCREEN,
        fit: cfg.fit,
        rotation: cfg.rotation(),
        quality: cfg.quality,
    };
    let frames = frames::from_file(&image, opts)
        .with_context(|| format!("could not load {}", image.display()))?;
    let _ = player.send(Command::Show(frames));
    Ok(())
}

/// A small square "screen" drawn in code, so no image file ships with the app.
fn icon() -> anyhow::Result<Icon> {
    const S: u32 = 32;
    let mut rgba = Vec::with_capacity((S * S * 4) as usize);
    for y in 0..S {
        for x in 0..S {
            let edge = !(3..S - 3).contains(&x) || !(3..S - 3).contains(&y);
            let corner = !(3..S - 3).contains(&x) && !(3..S - 3).contains(&y);
            let wave =
                (i64::from(y) - (20 - i64::from(x) / 3)).abs() <= 1 && (6..S - 6).contains(&x);
            let px: [u8; 4] = if corner {
                [0, 0, 0, 0]
            } else if edge {
                [235, 235, 235, 255]
            } else if wave {
                [96, 205, 255, 255]
            } else {
                [24, 24, 24, 255]
            };
            rgba.extend_from_slice(&px);
        }
    }
    Ok(Icon::from_rgba(rgba, S, S)?)
}
