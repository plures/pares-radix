//! Pares Radix — Tauri 2 desktop shell core.
//!
//! Architecture mirrors `src/lib/platform/tauri.ts` (events-not-commands):
//! commands are thin wrappers that emit praxis events; the Rust core holds no
//! business logic. The only state we own is **window geometry**, which is a
//! genuine OS-level concern and is persisted to disk under the app config dir
//! so it survives restarts (NOT an in-memory map).
//!
//! Contract implemented (must match the frontend bridge exactly):
//!   Commands : navigate, get_window_state, set_tray_menu, save_window_state
//!   Events   : app-booted{version}, window-state-changed{x,y,width,height,maximized},
//!              user-navigated{path}
//!   Types    : WindowStatePayload{x,y,width,height,maximized}, TrayMenuItem{id,label,path}

use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use tauri::{
    menu::{Menu, MenuItem},
    tray::{TrayIcon, TrayIconBuilder},
    AppHandle, Emitter, Manager, State, WindowEvent,
};

/// Window geometry payload — mirrors `WindowStatePayload` in `tauri.ts`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowStatePayload {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub maximized: bool,
}

impl Default for WindowStatePayload {
    fn default() -> Self {
        // Sane defaults if no saved state exists yet (first launch).
        Self {
            x: 0.0,
            y: 0.0,
            width: 1280.0,
            height: 800.0,
            maximized: false,
        }
    }
}

/// A single tray menu entry — mirrors `TrayMenuItem` in `tauri.ts`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrayMenuItem {
    pub id: String,
    pub label: String,
    pub path: String,
}

/// Holds the current tray menu item set so click handlers can resolve a menu
/// item id back to the navigation path it should emit.
#[derive(Default)]
struct TrayState {
    items: Mutex<Vec<TrayMenuItem>>,
    icon: Mutex<Option<TrayIcon>>,
}

// ─── Persistence helpers (real, restart-surviving — std::fs JSON) ────────────

const WINDOW_STATE_FILE: &str = "window-state.json";

/// Resolve the on-disk path for persisted window geometry, under the OS app
/// config dir (e.g. `%APPDATA%/ai.plures.radix/window-state.json` on Windows).
fn window_state_path(app: &AppHandle) -> Result<std::path::PathBuf, String> {
    let dir = app
        .path()
        .app_config_dir()
        .map_err(|e| format!("app_config_dir unavailable: {e}"))?;
    if !dir.exists() {
        std::fs::create_dir_all(&dir).map_err(|e| format!("create_dir_all failed: {e}"))?;
    }
    Ok(dir.join(WINDOW_STATE_FILE))
}

fn read_window_state(app: &AppHandle) -> Option<WindowStatePayload> {
    let path = window_state_path(app).ok()?;
    let bytes = std::fs::read(&path).ok()?;
    serde_json::from_slice::<WindowStatePayload>(&bytes).ok()
}

fn write_window_state(app: &AppHandle, state: &WindowStatePayload) -> Result<(), String> {
    let path = window_state_path(app)?;
    let json = serde_json::to_vec_pretty(state).map_err(|e| format!("serialize failed: {e}"))?;
    std::fs::write(&path, json).map_err(|e| format!("write failed: {e}"))?;
    Ok(())
}

// ─── Tray ────────────────────────────────────────────────────────────────────

/// (Re)build the tray menu from the given items and wire each entry's click to
/// emit `user-navigated { path }`. Stores the items so the click handler can map
/// menu-item id → path. Uses the concrete default (Wry) runtime — the standard
/// Tauri desktop-app pattern — so the `TrayIcon` can live in managed state.
fn rebuild_tray(app: &AppHandle, items: &[TrayMenuItem]) -> Result<(), String> {
    // Persist the items for the click handler to consult.
    {
        let state: State<TrayState> = app.state();
        *state.items.lock().map_err(|_| "tray items lock poisoned")? = items.to_vec();
    }

    // Build the menu from the items. Each MenuItem id == TrayMenuItem.id.
    let menu = Menu::new(app).map_err(|e| e.to_string())?;
    for item in items {
        let mi = MenuItem::with_id(app, &item.id, &item.label, true, None::<&str>)
            .map_err(|e| e.to_string())?;
        menu.append(&mi).map_err(|e| e.to_string())?;
    }

    let state: State<TrayState> = app.state();
    let mut icon_guard = state.icon.lock().map_err(|_| "tray icon lock poisoned")?;

    if let Some(tray) = icon_guard.as_ref() {
        // Update the existing tray's menu in place.
        tray.set_menu(Some(menu)).map_err(|e| e.to_string())?;
    } else {
        // First build: create the tray icon with a menu + click router. The
        // icon image itself comes from tauri.conf.json `app.trayIcon.iconPath`,
        // so we don't pass one here (avoids a borrowed-image lifetime bind).
        let app_for_menu = app.clone();
        let tray_builder = TrayIconBuilder::with_id("radix-tray")
            .tooltip("Pares Radix")
            .menu(&menu)
            .on_menu_event(move |_tray, event| {
                let menu_id = event.id().0.clone();
                let st: State<TrayState> = app_for_menu.state();
                let path = st.items.lock().ok().and_then(|items| {
                    items
                        .iter()
                        .find(|i| i.id == menu_id)
                        .map(|i| i.path.clone())
                });
                if let Some(path) = path {
                    let _ = app_for_menu.emit("user-navigated", NavigatedPayload { path });
                }
            });
        let tray = tray_builder.build(app).map_err(|e| e.to_string())?;
        *icon_guard = Some(tray);
    }

    Ok(())
}

// ─── Event payloads ───────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
struct BootedPayload {
    version: String,
}

#[derive(Debug, Clone, Serialize)]
struct NavigatedPayload {
    path: String,
}

// ─── Commands (thin wrappers — mirror tauri.ts) ───────────────────────────────

/// Emit `user-navigated { path }`. The frontend routes via emitFact.
#[tauri::command]
fn navigate(app: AppHandle, path: String) -> Result<(), String> {
    app.emit("user-navigated", NavigatedPayload { path })
        .map_err(|e| e.to_string())
}

/// Read persisted window geometry (or sane defaults on first run).
#[tauri::command]
fn get_window_state(app: AppHandle) -> WindowStatePayload {
    read_window_state(&app).unwrap_or_default()
}

/// Persist window geometry to disk (survives restart).
#[tauri::command]
fn save_window_state(app: AppHandle, state: WindowStatePayload) -> Result<(), String> {
    write_window_state(&app, &state)
}

/// Rebuild the tray menu from the supplied items; clicks emit `user-navigated`.
#[tauri::command]
fn set_tray_menu(app: AppHandle, items: Vec<TrayMenuItem>) -> Result<(), String> {
    rebuild_tray(&app, &items)
}

// ─── Window event hook ────────────────────────────────────────────────────────

/// Emit `window-state-changed` on move/resize so the frontend can persist it.
fn emit_window_state(app: &AppHandle) {
    let Some(window) = app.get_webview_window("main") else {
        return;
    };
    let maximized = window.is_maximized().unwrap_or(false);
    let pos = window.outer_position().ok();
    let size = window.outer_size().ok();
    let (x, y) = pos.map(|p| (p.x as f64, p.y as f64)).unwrap_or((0.0, 0.0));
    let (width, height) = size
        .map(|s| (s.width as f64, s.height as f64))
        .unwrap_or((0.0, 0.0));
    let payload = WindowStatePayload {
        x,
        y,
        width,
        height,
        maximized,
    };
    let _ = app.emit("window-state-changed", payload);
}

// ─── Entry point ──────────────────────────────────────────────────────────────

/// Build and run the Tauri application.
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .manage(TrayState::default())
        .invoke_handler(tauri::generate_handler![
            navigate,
            get_window_state,
            set_tray_menu,
            save_window_state
        ])
        .setup(|app| {
            let handle = app.handle().clone();

            // Updater is desktop-only (no mobile updater); gate so mobile builds compile.
            #[cfg(desktop)]
            {
                handle.plugin(tauri_plugin_updater::Builder::new().build())?;
            }

            // Build an initial (empty) tray so an icon is present from boot;
            // the frontend repopulates it via set_tray_menu once nav.visible
            // facts are known.
            if let Err(e) = rebuild_tray(&handle, &[]) {
                eprintln!("[radix] initial tray build failed: {e}");
            }

            // Announce boot with the compile-time package version.
            let version = env!("CARGO_PKG_VERSION").to_string();
            handle
                .emit("app-booted", BootedPayload { version })
                .expect("failed to emit app-booted");

            Ok(())
        })
        .on_window_event(|window, event| {
            // Emit geometry changes on move/resize so the FE can persist them.
            match event {
                WindowEvent::Moved(_) | WindowEvent::Resized(_) => {
                    emit_window_state(&window.app_handle().clone());
                }
                _ => {}
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running pares-radix tauri application");
}
