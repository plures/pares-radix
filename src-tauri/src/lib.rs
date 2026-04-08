/*!
 * pares-radix Tauri 2 backend — events not commands
 *
 * Architecture principle: Tauri commands are thin wrappers that emit praxis
 * events to the frontend. No domain logic lives here — all behaviour is
 * expressed as praxis facts, events, and rules in the frontend engine.
 *
 * Command → praxis event mapping:
 *   navigate(path)            → emits "user-navigated"  to frontend
 *   set_tray_menu(items)      → updates system tray from nav.visible items
 *   save_window_state(state)  → persists window geometry via praxis adapter
 *   get_window_state()        → returns persisted window geometry fact
 *
 * Anti-patterns (DO NOT):
 *   ✗ No business logic in commands — commands emit events only
 *   ✗ No Rust structs for app state — praxis facts are the state
 *   ✗ No direct filesystem calls — PluresDB persistence layer handles this
 */

use serde::{Deserialize, Serialize};
use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    App, AppHandle, Emitter, Manager, Runtime, WebviewUrl, WebviewWindowBuilder,
};

// ─── Praxis Event Payloads ────────────────────────────────────────────────────

/// Payload for the "user-navigated" praxis event.
/// Frontend rule `rule.user-navigation` handles routing.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserNavigatedPayload {
    pub path: String,
}

/// Payload for the "window-state-changed" praxis event.
/// Frontend rule `rule.window-state` persists via PluresDB adapter.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WindowStatePayload {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub maximized: bool,
}

/// A single tray menu item (mirrors the nav.visible fact shape).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrayMenuItem {
    pub id: String,
    pub label: String,
    pub path: String,
}

/// Payload for the "app-booted" praxis event.
/// Frontend rule `rule.window-state` restores window geometry on receipt.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppBootedPayload {
    pub version: String,
}

// ─── Tauri Commands (thin wrappers — emit events only) ────────────────────────

/// Navigate to a path by emitting the "user-navigated" praxis event.
///
/// The frontend listens for this event and calls `emitFact('user.navigated', { path })`,
/// which triggers `rule.user-navigation` in the praxis engine.
///
/// # Anti-pattern guard
/// This command MUST NOT contain any routing logic. Path resolution is a
/// frontend praxis rule.
#[tauri::command]
fn navigate<R: Runtime>(app: AppHandle<R>, path: String) -> Result<(), String> {
    app.emit("user-navigated", UserNavigatedPayload { path })
        .map_err(|e| e.to_string())
}

/// Persist window geometry and emit the "window-state-changed" praxis event.
///
/// The frontend listens for this event and calls
/// `emitFact('app.window', state)`, which persists via the PluresDB adapter
/// (persist: true fact).
///
/// # Anti-pattern guard
/// Window state is a praxis fact — this command only emits the event.
/// Persistence is handled entirely by the frontend praxis adapter.
#[tauri::command]
fn save_window_state<R: Runtime>(
    app: AppHandle<R>,
    state: WindowStatePayload,
) -> Result<(), String> {
    app.emit("window-state-changed", state)
        .map_err(|e| e.to_string())
}

/// Read the current window geometry directly from the main window.
/// Returns a `WindowStatePayload` so the frontend can seed `app.window`.
#[tauri::command]
fn get_window_state<R: Runtime>(app: AppHandle<R>) -> Result<WindowStatePayload, String> {
    let window = app
        .get_webview_window("main")
        .ok_or_else(|| "main window not found".to_string())?;

    let scale = window.scale_factor().map_err(|e| e.to_string())?;
    let pos = window.outer_position().map_err(|e| e.to_string())?;
    let size = window.outer_size().map_err(|e| e.to_string())?;
    let maximized = window.is_maximized().map_err(|e| e.to_string())?;

    // Convert physical pixels → logical pixels for cross-DPI consistency
    let logical_x = (pos.x as f64 / scale).round() as i32;
    let logical_y = (pos.y as f64 / scale).round() as i32;
    let logical_w = (size.width as f64 / scale).round() as u32;
    let logical_h = (size.height as f64 / scale).round() as u32;

    Ok(WindowStatePayload {
        x: logical_x,
        y: logical_y,
        width: logical_w,
        height: logical_h,
        maximized,
    })
}

/// Update the system tray menu from the `nav.visible` fact.
///
/// Called by the frontend when the `nav.visible` fact changes.
/// Tray items are derived exclusively from praxis facts — no static menu.
#[tauri::command]
fn set_tray_menu<R: Runtime>(app: AppHandle<R>, items: Vec<TrayMenuItem>) -> Result<(), String> {
    build_tray_menu(&app, &items).map_err(|e| e.to_string())
}

// ─── Tray Menu Builder ────────────────────────────────────────────────────────

/// Build (or rebuild) the tray menu from `nav.visible` items.
///
/// Each item emits a "user-navigated" event when clicked, keeping the tray
/// consistent with the rest of the event-driven architecture.
fn build_tray_menu<R: Runtime>(app: &AppHandle<R>, items: &[TrayMenuItem]) -> tauri::Result<()> {
    let menu = Menu::new(app)?;

    for item in items {
        let menu_item = MenuItem::with_id(app, &item.id, &item.label, true, None::<&str>)?;
        menu.append(&menu_item)?;
    }

    // Separator before Quit
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    menu.append(&quit)?;

    if let Some(tray) = app.tray_by_id("main") {
        tray.set_menu(Some(menu))?;
    }

    Ok(())
}

// ─── Window Event Wiring ──────────────────────────────────────────────────────

/// Wire window events so that geometry changes emit "window-state-changed".
///
/// The frontend praxis adapter persists `app.window` fact automatically.
fn wire_window_events<R: Runtime>(app: &App<R>) -> tauri::Result<()> {
    let window = app
        .get_webview_window("main")
        .ok_or(tauri::Error::WebviewNotFound)?;

    let handle = app.handle().clone();
    window.on_window_event(move |event| {
        let should_emit = matches!(
            event,
            tauri::WindowEvent::Moved(_) | tauri::WindowEvent::Resized(_)
        );

        if should_emit {
            if let Some(win) = handle.get_webview_window("main") {
                if let (Ok(scale), Ok(pos), Ok(size), Ok(maximized)) = (
                    win.scale_factor(),
                    win.outer_position(),
                    win.outer_size(),
                    win.is_maximized(),
                ) {
                    let payload = WindowStatePayload {
                        x: (pos.x as f64 / scale).round() as i32,
                        y: (pos.y as f64 / scale).round() as i32,
                        width: (size.width as f64 / scale).round() as u32,
                        height: (size.height as f64 / scale).round() as u32,
                        maximized,
                    };
                    let _ = handle.emit("window-state-changed", payload);
                }
            }
        }
    });

    Ok(())
}

// ─── Application Entry Point ──────────────────────────────────────────────────

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .setup(|app| {
            // Build the main window
            WebviewWindowBuilder::new(app, "main", WebviewUrl::default())
                .title("Pares Radix")
                .inner_size(1200.0, 800.0)
                .min_inner_size(800.0, 600.0)
                .build()?;

            // Build the initial tray with an empty menu (updated from frontend
            // once the nav.visible fact is seeded by initPraxisFacts).
            let initial_menu = Menu::new(app)?;
            let quit = MenuItem::with_id(app, "quit", "Quit Pares Radix", true, None::<&str>)?;
            initial_menu.append(&quit)?;

            let handle = app.handle().clone();
            TrayIconBuilder::with_id("main")
                .icon(
                    app.default_window_icon()
                        .cloned()
                        .unwrap_or_else(|| tauri::image::Image::new(&[], 0, 0)),
                )
                .menu(&initial_menu)
                .on_menu_event(move |app, event| {
                    if event.id() == "quit" {
                        app.exit(0);
                    } else {
                        // Tray item IDs are the route paths (href). Emit directly.
                        let _ = handle.emit(
                            "user-navigated",
                            UserNavigatedPayload {
                                path: event.id().0.to_string(),
                            },
                        );
                    }
                })
                .on_tray_icon_event(|_tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        // Left-click on tray shows the main window
                    }
                })
                .build(app)?;

            wire_window_events(app)?;

            // Emit app-booted so the frontend can restore window state and
            // seed the nav.visible fact, which in turn calls set_tray_menu.
            app.emit(
                "app-booted",
                AppBootedPayload {
                    version: env!("CARGO_PKG_VERSION").to_string(),
                },
            )?;

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            navigate,
            save_window_state,
            get_window_state,
            set_tray_menu,
        ])
        .run(tauri::generate_context!())
        .expect("error while running pares-radix");
}
