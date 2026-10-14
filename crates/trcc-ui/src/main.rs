#![windows_subsystem = "windows"]
// Tauri commands receive `State` by value.
#![allow(clippy::needless_pass_by_value)]

use std::fs::{self, OpenOptions};
use std::os::windows::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, PoisonError};

use serde::Serialize;
use tauri::ipc::Response;
use tauri::{DragDropEvent, Emitter, Manager, State, WindowEvent};
use tauri_plugin_dialog::DialogExt;
use trcc_core::config::{self, Config};
use trcc_core::render::{self, Fit, Rotation};
use trcc_core::{Error, autostart};

const SCREEN: u32 = 480;
const TRAY_EXE: &str = "trcc-lcd-tray.exe";
const IMAGE_TYPES: [&str; 5] = ["png", "jpg", "jpeg", "gif", "webp"];
const ERROR_SHARING_VIOLATION: i32 = 32;

struct App {
    dir: PathBuf,
    settings: PathBuf,
    tray: PathBuf,
    /// Serializes load, change, save so two quick changes cannot overwrite each other.
    edit: Mutex<()>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct View {
    image: Option<String>,
    image_info: Option<String>,
    fit: Fit,
    rotation: u16,
    start_with_windows: bool,
    tray_running: bool,
}

fn message(e: impl std::fmt::Display) -> String {
    e.to_string()
}

fn view(app: &App, cfg: &Config) -> View {
    let image = cfg.image.as_deref();
    View {
        image: image
            .and_then(Path::file_name)
            .map(|n| n.to_string_lossy().into_owned()),
        image_info: image.and_then(describe),
        fit: cfg.fit,
        rotation: cfg.rotation,
        start_with_windows: autostart::is_enabled(&app.tray),
        tray_running: tray_running(&app.dir),
    }
}

fn describe(path: &Path) -> Option<String> {
    let kind = path.extension()?.to_string_lossy().to_uppercase();
    let bytes = fs::metadata(path).ok()?.len();
    #[allow(clippy::cast_precision_loss)]
    let size = if bytes >= 1024 * 1024 {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    } else {
        format!("{} KB", bytes.div_ceil(1024))
    };
    Some(format!("{kind}, {size}"))
}

/// The tray app holds its lock file open without sharing, so a sharing
/// violation here means it is running.
fn tray_running(dir: &Path) -> bool {
    OpenOptions::new()
        .read(true)
        .share_mode(0)
        .open(dir.join(config::TRAY_LOCK))
        .is_err_and(|e| e.raw_os_error() == Some(ERROR_SHARING_VIOLATION))
}

fn update(
    app: &App,
    change: impl FnOnce(&mut Config) -> Result<(), Error>,
) -> Result<View, String> {
    let _guard = app.edit.lock().unwrap_or_else(PoisonError::into_inner);
    let mut cfg = Config::load(&app.settings).map_err(message)?;
    change(&mut cfg).map_err(message)?;
    cfg.save(&app.settings).map_err(message)?;
    Ok(view(app, &cfg))
}

/// Only image files that decode are saved, so the tray never gets a broken setting.
fn set_image(app: &App, path: &Path) -> Result<View, String> {
    let ext = path
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    if !IMAGE_TYPES.contains(&ext.as_str()) || !path.is_file() {
        return Err("Pick a PNG, JPEG, GIF or WebP image.".into());
    }
    render::load(path).map_err(message)?;
    update(app, |c| {
        c.image = Some(path.to_path_buf());
        Ok(())
    })
}

#[tauri::command]
fn state(app: State<'_, App>) -> Result<View, String> {
    let cfg = Config::load(&app.settings).map_err(message)?;
    Ok(view(&app, &cfg))
}

#[tauri::command]
fn set_fit(app: State<'_, App>, fit: Fit) -> Result<View, String> {
    update(&app, |c| {
        c.fit = fit;
        Ok(())
    })
}

#[tauri::command]
fn set_rotation(app: State<'_, App>, degrees: u16) -> Result<View, String> {
    update(&app, |c| {
        Rotation::from_degrees(degrees)?;
        c.rotation = degrees;
        Ok(())
    })
}

#[tauri::command]
fn set_start_with_windows(app: State<'_, App>, enabled: bool) -> Result<View, String> {
    autostart::set(&app.tray, enabled).map_err(message)?;
    state(app)
}

#[tauri::command]
fn start_tray(app: State<'_, App>) -> Result<View, String> {
    if !tray_running(&app.dir) {
        std::process::Command::new(&app.tray)
            .spawn()
            .map_err(message)?;
    }
    state(app)
}

#[tauri::command]
async fn pick_image(handle: tauri::AppHandle) -> Result<Option<View>, String> {
    let picked = handle
        .dialog()
        .file()
        .add_filter("Images", &IMAGE_TYPES)
        .blocking_pick_file();
    let Some(path) = picked.and_then(|p| p.into_path().ok()) else {
        return Ok(None);
    };
    set_image(&handle.state::<App>(), &path).map(Some)
}

/// The first frame exactly as the screen gets it, as JPEG bytes.
#[tauri::command]
async fn preview(handle: tauri::AppHandle) -> Result<Response, String> {
    let app = handle.state::<App>();
    let cfg = Config::load(&app.settings).map_err(message)?;
    let path = cfg.image.clone().ok_or("No picture set.")?;
    let img = render::load(&path).map_err(message)?;
    let frame = render::prepare(&img, SCREEN, SCREEN, cfg.fit, cfg.rotation());
    let jpeg = render::encode_jpeg(&frame, cfg.quality).map_err(message)?;
    Ok(Response::new(jpeg))
}

fn on_drop(window: &tauri::Window, event: &DragDropEvent) {
    match event {
        DragDropEvent::Enter { .. } => {
            let _ = window.emit("drag", true);
        }
        DragDropEvent::Leave => {
            let _ = window.emit("drag", false);
        }
        DragDropEvent::Drop { paths, .. } => {
            let _ = window.emit("drag", false);
            if let Some(path) = paths.first() {
                let result = set_image(&window.state::<App>(), path);
                let _ = window.emit("dropped", result);
            }
        }
        _ => {}
    }
}

fn main() {
    let setup = || -> Result<App, String> {
        let dir = config::app_dir().map_err(message)?;
        fs::create_dir_all(&dir).map_err(message)?;
        let settings = dir.join("config.toml");
        if !settings.exists() {
            Config::default().save(&settings).map_err(message)?;
        }
        let exe = std::env::current_exe().map_err(message)?;
        let tray = exe.with_file_name(TRAY_EXE);
        Ok(App {
            dir,
            settings,
            tray,
            edit: Mutex::new(()),
        })
    };
    let app = match setup() {
        Ok(a) => a,
        Err(e) => {
            if let Ok(dir) = config::app_dir() {
                let _ = fs::write(dir.join("ui-error.log"), format!("{e}\n"));
            }
            return;
        }
    };
    if !tray_running(&app.dir) {
        let _ = std::process::Command::new(&app.tray).spawn();
    }

    let result = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(app)
        .on_window_event(|window, event| {
            if let WindowEvent::DragDrop(e) = event {
                on_drop(window, e);
            }
        })
        .invoke_handler(tauri::generate_handler![
            state,
            set_fit,
            set_rotation,
            set_start_with_windows,
            start_tray,
            pick_image,
            preview,
        ])
        .run(tauri::generate_context!());
    if let (Err(e), Ok(dir)) = (result, config::app_dir()) {
        let _ = fs::write(dir.join("ui-error.log"), format!("{e}\n"));
    }
}
