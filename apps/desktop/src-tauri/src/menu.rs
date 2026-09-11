use crate::state::{AppState, Event};
use tauri::{
    Manager,
    menu::{Menu, MenuItem, PredefinedMenuItem as Predefined, Submenu},
};

pub(crate) fn install(app: &tauri::App) -> tauri::Result<()> {
    let settings = MenuItem::with_id(app, "settings", "Settings…", true, Some("CmdOrCtrl+,"))?;
    let new_session =
        MenuItem::with_id(app, "new_session", "New Session", true, Some("CmdOrCtrl+N"))?;
    let new_window = MenuItem::with_id(
        app,
        "new_window",
        "New Window",
        true,
        Some("CmdOrCtrl+Shift+N"),
    )?;
    let terminal = MenuItem::with_id(
        app,
        "terminal",
        "Terminal",
        true,
        Some("CmdOrCtrl+Backquote"),
    )?;
    let app_menu = Submenu::with_items(
        app,
        "Remote Codex",
        true,
        &[
            &settings,
            &Predefined::separator(app)?,
            &Predefined::hide(app, None)?,
            &Predefined::hide_others(app, None)?,
            &Predefined::show_all(app, None)?,
            &Predefined::separator(app)?,
            &Predefined::quit(app, None)?,
        ],
    )?;
    let file = Submenu::with_items(
        app,
        "File",
        true,
        &[
            &new_session,
            &new_window,
            &Predefined::separator(app)?,
            &Predefined::close_window(app, None)?,
        ],
    )?;
    let edit = Submenu::with_items(
        app,
        "Edit",
        true,
        &[
            &Predefined::undo(app, None)?,
            &Predefined::redo(app, None)?,
            &Predefined::separator(app)?,
            &Predefined::cut(app, None)?,
            &Predefined::copy(app, None)?,
            &Predefined::paste(app, None)?,
            &Predefined::select_all(app, None)?,
        ],
    )?;
    let view = Submenu::with_items(app, "View", true, &[&terminal])?;
    let windows = Submenu::with_items(
        app,
        "Window",
        true,
        &[
            &Predefined::minimize(app, None)?,
            &Predefined::maximize(app, None)?,
        ],
    )?;
    app.set_menu(Menu::with_items(
        app,
        &[&app_menu, &file, &edit, &view, &windows],
    )?)?;
    app.on_menu_event(|app, event| {
        if let Some(window) = app
            .webview_windows()
            .into_values()
            .find(|w| w.is_focused().unwrap_or(false))
            && let Ok(context) = app.state::<AppState>().window(&window)
        {
            context.send(Event::UiAction {
                action: event.id().as_ref().into(),
            });
        }
    });
    Ok(())
}
