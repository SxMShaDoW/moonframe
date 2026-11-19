//! Panel mode with AppKit; see the parent module for what it does.
//!
//! What trying it on a real app found:
//!
//! - GPUI makes an app `Regular` while it opens a windowed app, so the
//!   `Accessory` policy is set from `App::set_activation_policy`, which
//!   applies at once.
//! - A click on a status item reaches the app as a mouse down in the item's
//!   own window, active app or not. The button's action does not always
//!   fire, so a local monitor takes the click, and the button never sees it.
//! - `NSApp.activate()` is asynchronous on macOS 26: the app is active about
//!   400 ms later. With `activate()` alone another app stayed in front, so
//!   the panel asks with `activateIgnoringOtherApps` too.
//! - A click elsewhere makes the panel resign key. That is what hides it.

#![allow(unsafe_code, reason = "AppKit through objc2; see the SAFETY notes")]

use {
    block2::RcBlock,
    gpui_kit::{ActivationPolicy, App, Window},
    objc2::{
        MainThreadMarker,
        rc::Retained,
        runtime::{NSObjectProtocol, ProtocolObject},
    },
    objc2_app_kit::{
        NSApplication, NSEvent, NSEventMask, NSEventModifierFlags, NSScreen, NSView, NSWindow,
        NSWindowDidResignKeyNotification,
    },
    objc2_foundation::{NSNotification, NSNotificationCenter, NSPoint, NSRect, NSSize},
    raw_window_handle::RawWindowHandle,
    std::{
        cell::{Cell, RefCell},
        ptr::{self, NonNull},
        sync::OnceLock,
    },
};

/// The panel hangs this far under the item, in points.
const GAP: f64 = 4.0;
/// The panel keeps this far from the screen's edges.
const EDGE: f64 = 8.0;

/// The mod whose item the panel hangs under. Set once, by [`start`].
static OWNER: OnceLock<String> = OnceLock::new();

thread_local! {
    /// The main window, once [`attach`] has it.
    static PANEL: RefCell<Option<Retained<NSWindow>>> = const { RefCell::new(None) };
    /// What watches the panel lose focus.
    static WATCH: RefCell<Vec<Retained<ProtocolObject<dyn NSObjectProtocol>>>> =
        const { RefCell::new(Vec::new()) };
    /// A hide is under way: AppKit tells the panel it resigned key inside it.
    static HIDING: Cell<bool> = const { Cell::new(false) };
}

pub(super) fn on() -> bool {
    OWNER.get().is_some()
}

pub(super) fn start(owner: &str, cx: &mut App) {
    if OWNER.set(owner.to_owned()).is_err() {
        return;
    }
    cx.set_activation_policy(ActivationPolicy::Accessory);
    // A click on the item: the app's local monitor sees it first.
    let clicks = RcBlock::new(|event: NonNull<NSEvent>| -> *mut NSEvent {
        // SAFETY: AppKit hands the monitor a live event.
        let event_ref = unsafe { event.as_ref() };
        if on_item(event_ref) {
            toggle();
            return ptr::null_mut();
        }
        event.as_ptr()
    });
    // SAFETY: the block returns the event it got, or null.
    let monitor = unsafe {
        NSEvent::addLocalMonitorForEventsMatchingMask_handler(NSEventMask::LeftMouseDown, &clicks)
    };
    // The monitors live as long as the app.
    std::mem::forget(monitor);

    // Cmd-W: an app with no Dock icon has no menu bar, so the menu's Close
    // Window never sees the key. The panel hides on it.
    let keys = RcBlock::new(|event: NonNull<NSEvent>| -> *mut NSEvent {
        // SAFETY: AppKit hands the monitor a live event.
        let event_ref = unsafe { event.as_ref() };
        if closes_panel(event_ref) {
            hide();
            return ptr::null_mut();
        }
        event.as_ptr()
    });
    // SAFETY: the block returns the event it got, or null.
    let monitor = unsafe {
        NSEvent::addLocalMonitorForEventsMatchingMask_handler(NSEventMask::KeyDown, &keys)
    };
    std::mem::forget(monitor);
}

pub(super) fn attach(window: &Window) {
    let Some(panel) = native(window) else {
        tracing::warn!("panel mode: the main window has no AppKit window");
        return;
    };
    // A click elsewhere: the panel loses key status.
    let lost = RcBlock::new(|_: NonNull<NSNotification>| hide());
    // SAFETY: main thread; the block keeps no borrow across the call.
    let watch = unsafe {
        NSNotificationCenter::defaultCenter().addObserverForName_object_queue_usingBlock(
            Some(NSWindowDidResignKeyNotification),
            Some(&panel),
            None,
            &lost,
        )
    };
    WATCH.with_borrow_mut(|all| all.push(watch));
    panel.orderOut(None);
    PANEL.with_borrow_mut(|slot| *slot = Some(panel));
}

pub(super) fn show() {
    let (Some(panel), Some(main)) = (panel(), MainThreadMarker::new()) else {
        return;
    };
    let size = panel.frame().size;
    match item_frame() {
        Some((icon, area)) => panel.setFrameTopLeftPoint(top_left_under(icon, size, area)),
        // No item yet: the middle of the screen.
        None => panel.center(),
    }
    let app = NSApplication::sharedApplication(main);
    app.activate();
    #[allow(
        deprecated,
        reason = "the cooperative `activate` did not bring the app forward"
    )]
    app.activateIgnoringOtherApps(true);
    panel.makeKeyAndOrderFront(None);
}

pub(super) fn hide() {
    let Some(panel) = panel() else {
        return;
    };
    if HIDING.replace(true) || !panel.isVisible() {
        HIDING.set(false);
        return;
    }
    panel.orderOut(None);
    HIDING.set(false);
}

/// A click on the item: shows the panel, or hides it.
fn toggle() {
    let Some(panel) = panel() else {
        return;
    };
    if panel.isVisible() {
        hide();
    } else {
        show();
    }
}

/// The panel, a clone: AppKit calls back into this module from its own
/// calls, so no borrow of the slot lives across one.
fn panel() -> Option<Retained<NSWindow>> {
    PANEL.with_borrow(Clone::clone)
}

/// The AppKit window of a GPUI window.
fn native(window: &Window) -> Option<Retained<NSWindow>> {
    // Window has an inherent `window_handle` of its own.
    let handle = raw_window_handle::HasWindowHandle::window_handle(window).ok()?;
    let RawWindowHandle::AppKit(handle) = handle.as_raw() else {
        return None;
    };
    // SAFETY: the handle holds the window's live view, read on the main
    // thread, where this runs.
    let view = unsafe { handle.ns_view.cast::<NSView>().as_ref() };
    view.window()
}

/// Whether `event` is a plain click on the item of the panel's mod. A
/// Ctrl-click goes on: it opens the item's menu.
fn on_item(event: &NSEvent) -> bool {
    let (Some(owner), Some(main)) = (OWNER.get(), MainThreadMarker::new()) else {
        return false;
    };
    if event
        .modifierFlags()
        .contains(NSEventModifierFlags::Control)
    {
        return false;
    }
    let (Some(item), Some(hit)) = (crate::status_item::window(owner), event.window(main)) else {
        return false;
    };
    ptr::eq(Retained::as_ptr(&item), Retained::as_ptr(&hit))
}

/// Whether `event` is Cmd-W typed in the panel. Another window of the app
/// keeps its own Cmd-W.
fn closes_panel(event: &NSEvent) -> bool {
    let (Some(panel), Some(main)) = (panel(), MainThreadMarker::new()) else {
        return false;
    };
    let flags = event.modifierFlags() & NSEventModifierFlags::DeviceIndependentFlagsMask;
    let w = event
        .charactersIgnoringModifiers()
        .is_some_and(|keys| keys.to_string() == "w");
    let Some(hit) = event.window(main) else {
        return false;
    };
    flags == NSEventModifierFlags::Command
        && w
        && ptr::eq(Retained::as_ptr(&panel), Retained::as_ptr(&hit))
}

/// The item's frame, and the part of its screen free of the menu bar and Dock.
fn item_frame() -> Option<(NSRect, NSRect)> {
    let main = MainThreadMarker::new()?;
    let item = crate::status_item::window(OWNER.get()?)?;
    let area = item
        .screen()
        .or_else(|| NSScreen::mainScreen(main))?
        .visibleFrame();
    Some((item.frame(), area))
}

/// Where the panel's top left goes: under `icon`, centered on it, and inside
/// `area` however narrow it is. AppKit's origin is the screen's bottom left.
fn top_left_under(icon: NSRect, size: NSSize, area: NSRect) -> NSPoint {
    let left = area.origin.x + EDGE;
    let right = area.origin.x + area.size.width - size.width - EDGE;
    let centered = icon.origin.x + icon.size.width / 2.0 - size.width / 2.0;
    NSPoint::new(centered.clamp(left, right.max(left)), icon.origin.y - GAP)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(x: f64, y: f64, w: f64, h: f64) -> NSRect {
        NSRect::new(NSPoint::new(x, y), NSSize::new(w, h))
    }

    #[test]
    fn the_panel_hangs_centered_under_the_item() {
        let screen = rect(0.0, 0.0, 2560.0, 1410.0);
        let icon = rect(2177.0, 1410.0, 40.0, 30.0);
        let at = top_left_under(icon, NSSize::new(640.0, 560.0), screen);
        // Centered on the icon, a few points under it.
        assert_eq!(at.x, 2177.0 + 20.0 - 320.0);
        assert_eq!(at.y, 1410.0 - GAP);
    }

    #[test]
    fn the_panel_stays_inside_the_screen() {
        let screen = rect(100.0, 0.0, 1000.0, 800.0);
        let size = NSSize::new(640.0, 560.0);
        let near_right = top_left_under(rect(1080.0, 800.0, 20.0, 30.0), size, screen);
        assert_eq!(near_right.x, 100.0 + 1000.0 - 640.0 - EDGE);
        let near_left = top_left_under(rect(100.0, 800.0, 20.0, 30.0), size, screen);
        assert_eq!(near_left.x, 100.0 + EDGE);
    }

    #[test]
    fn a_panel_wider_than_the_screen_keeps_its_left_edge() {
        let screen = rect(0.0, 0.0, 500.0, 800.0);
        let at = top_left_under(
            rect(250.0, 800.0, 20.0, 30.0),
            NSSize::new(640.0, 560.0),
            screen,
        );
        assert_eq!(at.x, EDGE);
    }

    #[test]
    fn without_a_panel_every_call_does_nothing() {
        // Not started and not attached: no panel, no item, no crash.
        assert!(!on());
        show();
        hide();
        toggle();
        assert!(panel().is_none());
        assert!(item_frame().is_none());
    }
}
