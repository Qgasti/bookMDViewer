use std::path::{Path, PathBuf};
use std::sync::Mutex;

#[derive(serde::Serialize)]
struct FileEntry {
    name: String,
    path: String,
}

use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

/// Holds the file we are currently viewing plus the live filesystem watcher.
#[derive(Default)]
struct AppState {
    current: Mutex<Option<PathBuf>>,
    watcher: Mutex<Option<RecommendedWatcher>>,
}

/// Pull a markdown file path out of the process arguments (set when the OS
/// launches us via a `.md` file association on Windows / Linux).
fn path_from_args() -> Option<PathBuf> {
    std::env::args_os()
        .skip(1)
        .map(PathBuf::from)
        .find(|p| p.extension().map(|e| e.eq_ignore_ascii_case("md")).unwrap_or(false) && p.exists())
}

/// Whether the app was launched with `--edit` (open straight into edit mode).
#[tauri::command]
fn start_in_edit() -> bool {
    std::env::args().any(|a| a == "--edit")
}

/// Optional `--zoom=<factor>` launch flag (0.0 means "not set").
#[tauri::command]
fn start_zoom() -> f64 {
    std::env::args()
        .find_map(|a| a.strip_prefix("--zoom=").and_then(|v| v.parse::<f64>().ok()))
        .unwrap_or(0.0)
}

/// Returns the file path the app was opened with, if any.
#[tauri::command]
fn get_initial_path(state: State<AppState>) -> Option<String> {
    state
        .current
        .lock()
        .unwrap()
        .as_ref()
        .map(|p| p.to_string_lossy().into_owned())
}

/// Read a markdown file's raw text.
#[tauri::command]
fn read_md(path: String) -> Result<String, String> {
    std::fs::read_to_string(&path).map_err(|e| format!("Failed to read {path}: {e}"))
}

/// Write text back to a markdown file.
#[tauri::command]
fn write_md(path: String, content: String) -> Result<(), String> {
    std::fs::write(&path, content).map_err(|e| format!("Failed to write {path}: {e}"))
}

/// Begin watching `path`; emits `md-changed` whenever the file is modified.
/// We watch the *parent directory* (non-recursive) because many editors save
/// by replacing the file, which breaks a watch placed directly on the file.
#[tauri::command]
fn watch_file(path: String, app: tauri::AppHandle, state: State<AppState>) -> Result<(), String> {
    let target = PathBuf::from(&path);
    let parent = target
        .parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| "File has no parent directory".to_string())?;

    *state.current.lock().unwrap() = Some(target.clone());

    let watched = target.clone();
    let handle = app.clone();
    let mut watcher = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
        if let Ok(event) = res {
            if matches!(
                event.kind,
                EventKind::Modify(_) | EventKind::Create(_) | EventKind::Any
            ) && event.paths.iter().any(|p| p == &watched)
            {
                let _ = handle.emit("md-changed", watched.to_string_lossy().into_owned());
            }
        }
    })
    .map_err(|e| e.to_string())?;

    watcher
        .watch(&parent, RecursiveMode::NonRecursive)
        .map_err(|e| e.to_string())?;

    // Keep the watcher alive by stashing it in state (replacing any previous one).
    *state.watcher.lock().unwrap() = Some(watcher);
    Ok(())
}

/// Toggle the always-on-top (pin) state of the quick-note window.
#[tauri::command]
fn set_always_on_top(window: tauri::WebviewWindow, on_top: bool) -> Result<(), String> {
    window.set_always_on_top(on_top).map_err(|e| e.to_string())
}

/// List all .md / .txt files (non-recursive) in `dir`, sorted by name.
#[tauri::command]
fn list_dir(dir: String) -> Result<Vec<FileEntry>, String> {
    let path = std::path::Path::new(&dir);
    let mut entries: Vec<FileEntry> = std::fs::read_dir(path)
        .map_err(|e| format!("無法讀取目錄: {e}"))?
        .filter_map(|res| res.ok())
        .filter(|e| e.path().is_file())
        .filter_map(|e| {
            let p = e.path();
            let ext = p.extension()?.to_string_lossy().to_lowercase();
            if ext == "md" || ext == "txt" || ext == "markdown" {
                Some(FileEntry {
                    name: p
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .into_owned(),
                    path: p.to_string_lossy().into_owned(),
                })
            } else {
                None
            }
        })
        .collect();
    entries.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    Ok(entries)
}

/// Register the global shortcut that opens the Quick Note window.
/// Called by the frontend on startup with the user's saved shortcut string.
#[tauri::command]
fn register_quick_note_shortcut(shortcut: String, app: AppHandle) -> Result<(), String> {
    app.global_shortcut()
        .register(shortcut.as_str())
        .map_err(|e| e.to_string())
}

/// Replace the current Quick Note global shortcut with a new one.
#[tauri::command]
fn update_quick_note_shortcut(
    old_shortcut: String,
    new_shortcut: String,
    app: AppHandle,
) -> Result<(), String> {
    let gs = app.global_shortcut();
    // Unregister old (ignore error – it might not be registered yet).
    let _ = gs.unregister(old_shortcut.as_str());
    gs.register(new_shortcut.as_str())
        .map_err(|e| e.to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let state = AppState::default();
    if let Some(p) = path_from_args() {
        *state.current.lock().unwrap() = Some(p);
    }

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let show =
                MenuItem::with_id(app, "show", "Show", true, None::<&str>)?;
            let quit =
                MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&show, &quit])?;

            TrayIconBuilder::new()
                .icon(
                    app.default_window_icon()
                        .expect("app icon should be configured")
                        .clone(),
                )
                .tooltip("Markdown Viewer")
                .menu(&menu)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "quit" => app.exit(0),
                    "show" => {
                        if let Some(win) = app.get_webview_window("main") {
                            let _ = win.show();
                            let _ = win.set_focus();
                        }
                    }
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::DoubleClick {
                        button: MouseButton::Left,
                        ..
                    } = event
                    {
                        let app = tray.app_handle();
                        if let Some(win) = app.get_webview_window("main") {
                            let _ = win.show();
                            let _ = win.set_focus();
                        }
                    }
                })
                .build(app)?;
            Ok(())
        })
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, _shortcut, event| {
                    // The only registered global shortcut is the Quick Note shortcut,
                    // so any "Pressed" event means: show or create the Quick Note window.
                    if event.state() == ShortcutState::Pressed {
                        if let Some(win) = app.get_webview_window("quick-note") {
                            let _ = win.show();
                            let _ = win.set_focus();
                        } else {
                            let _ = tauri::WebviewWindowBuilder::new(
                                app,
                                "quick-note",
                                tauri::WebviewUrl::App("quick-note.html".into()),
                            )
                            .title("Quick Note")
                            .inner_size(440.0, 340.0)
                            .min_inner_size(280.0, 200.0)
                            .always_on_top(true)
                            .decorations(true)
                            .resizable(true)
                            .build();
                        }
                    }
                })
                .build(),
        )
        .manage(state)
        .invoke_handler(tauri::generate_handler![
            get_initial_path,
            start_in_edit,
            start_zoom,
            read_md,
            write_md,
            watch_file,
            set_always_on_top,
            list_dir,
            register_quick_note_shortcut,
            update_quick_note_shortcut
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app, event| {
            // Hide the main window to the tray instead of closing it.
            if let tauri::RunEvent::WindowEvent { label, event: win_event, .. } = &event {
                if label == "main" {
                    if let tauri::WindowEvent::CloseRequested { api, .. } = win_event {
                        api.prevent_close();
                        if let Some(win) = app.get_webview_window("main") {
                            let _ = win.hide();
                        }
                    }
                }
            }

            // macOS delivers file-association opens as a runtime event.
            #[cfg(target_os = "macos")]
            if let tauri::RunEvent::Opened { urls } = &event {
                for url in urls {
                    if let Ok(p) = url.to_file_path() {
                        let _ = app.emit("open-file", p.to_string_lossy().into_owned());
                    }
                }
            }
            let _ = (app, &event);
        });
}
