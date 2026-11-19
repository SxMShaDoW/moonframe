//! Panel mode: the app is a menu bar app. It has no Dock icon, and its main
//! window is a panel under one mod's menu bar item (`app:status-item`). A
//! click on the item shows the panel under it; a click anywhere else hides
//! it; closing, minimizing and Cmd-W hide it too, so the app does not quit
//! with its window. Quit stays a command of its own.
//!
//! An app asks with [`Moonframe::panel`](crate::Moonframe::panel). Only macOS
//! has it (and only with the `status-item` feature, which draws the item);
//! elsewhere the window stays a normal one.
//!
//! The item keeps its menu: a right click or a Ctrl-click opens it, as before.
//!
//! The system reopening the running app shows the panel too: a click on a
//! Dock tile kept for it, a double-click in Finder, Spotlight, or `open`.

#[cfg(all(feature = "status-item", target_os = "macos"))]
mod macos;

use gpui_kit::{App, Application, Window};

/// Turns panel mode on for the mod `owner`'s item. Before the window opens.
#[cfg(all(feature = "status-item", target_os = "macos"))]
pub(crate) fn start(owner: &str, cx: &mut App) {
    macos::start(owner, cx);
}

/// Whether the app runs as a panel.
#[cfg(all(feature = "status-item", target_os = "macos"))]
pub(crate) fn on() -> bool {
    macos::on()
}

/// Makes `window`, the main window, the panel: hidden, and hiding itself when
/// it loses focus.
#[cfg(all(feature = "status-item", target_os = "macos"))]
pub(crate) fn attach(window: &Window) {
    macos::attach(window);
}

/// Shows the panel under its item, in front, and gives it the keys.
#[cfg(all(feature = "status-item", target_os = "macos"))]
pub(crate) fn show() {
    macos::show();
}

/// Hides the panel. The window stays open.
#[cfg(all(feature = "status-item", target_os = "macos"))]
pub(crate) fn hide() {
    macos::hide();
}

/// Shows the panel when the system reopens the running app. A hidden panel is
/// ordered out, not minimized, so AppKit has no window to bring back by
/// itself. Call it before the application runs; it does nothing while panel
/// mode is off.
pub(crate) fn listen(application: &Application) {
    application.on_reopen(|_| {
        if on() {
            show();
        }
    });
}

// Elsewhere there is no panel, and the window is a normal one.
#[cfg(not(all(feature = "status-item", target_os = "macos")))]
pub(crate) fn start(_: &str, _: &mut App) {}

#[cfg(not(all(feature = "status-item", target_os = "macos")))]
pub(crate) fn on() -> bool {
    false
}

#[cfg(not(all(feature = "status-item", target_os = "macos")))]
pub(crate) fn attach(_: &Window) {}

#[cfg(not(all(feature = "status-item", target_os = "macos")))]
pub(crate) fn show() {}

#[cfg(not(all(feature = "status-item", target_os = "macos")))]
pub(crate) fn hide() {}
