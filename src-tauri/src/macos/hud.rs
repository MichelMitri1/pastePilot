//! Tiny floating status pill ("Generating...", "Reply pasted", errors).
//!
//! A native non-activating NSPanel rather than a webview: it costs almost no
//! memory, appears instantly, ignores the mouse and never steals keyboard
//! focus from the app you are replying in.

use objc2::rc::Retained;
use objc2::runtime::{AnyObject, Bool};
use objc2::{msg_send, MainThreadMarker, MainThreadOnly};
use objc2_app_kit::{
    NSBackingStoreType, NSColor, NSFont, NSFontWeightMedium, NSPanel,
    NSScreen, NSStatusWindowLevel, NSTextAlignment, NSTextField, NSVisualEffectBlendingMode, NSVisualEffectMaterial,
    NSVisualEffectState, NSVisualEffectView, NSWindowCollectionBehavior, NSWindowStyleMask,
};
use objc2_foundation::{NSPoint, NSRect, NSSize, NSString};
use std::cell::RefCell;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use tauri::AppHandle;

struct Hud {
    panel: Retained<NSPanel>,
    label: Retained<NSTextField>,
}

thread_local! {
    static HUD: RefCell<Option<Hud>> = const { RefCell::new(None) };
}

/// Bumped on every show/hide so a stale auto-hide timer doesn't hide a newer message.
static GENERATION: AtomicU64 = AtomicU64::new(0);

const HEIGHT: f64 = 34.0;
const RADIUS: f64 = 10.0;

/// Shows `text`. With `hide_after`, hides itself after that long.
pub fn show(app: &AppHandle, text: &str, hide_after: Option<Duration>) {
    let generation = GENERATION.fetch_add(1, Ordering::SeqCst) + 1;
    let text = text.to_owned();
    let _ = app.run_on_main_thread(move || present(&text));
    if let Some(delay) = hide_after {
        let app = app.clone();
        tauri::async_runtime::spawn(async move {
            tokio::time::sleep(delay).await;
            if GENERATION.load(Ordering::SeqCst) == generation {
                let _ = app.run_on_main_thread(dismiss);
            }
        });
    }
}

pub fn hide(app: &AppHandle) {
    GENERATION.fetch_add(1, Ordering::SeqCst);
    let _ = app.run_on_main_thread(dismiss);
}

fn dismiss() {
    HUD.with(|cell| {
        if let Some(hud) = cell.borrow().as_ref() {
            hud.panel.orderOut(None);
        }
    });
}

fn present(text: &str) {
    let Some(mtm) = MainThreadMarker::new() else { return };
    HUD.with(|cell| {
        let mut slot = cell.borrow_mut();
        let hud = slot.get_or_insert_with(|| build(mtm));

        hud.label.setStringValue(&NSString::from_str(text));
        hud.label.sizeToFit();
        let label_size = hud.label.frame().size;
        let width = (label_size.width + 36.0).max(120.0).ceil();

        let screen = NSScreen::mainScreen(mtm).map(|s| s.visibleFrame());
        let (x, y) = match screen {
            Some(vf) => (
                vf.origin.x + (vf.size.width - width) / 2.0,
                vf.origin.y + vf.size.height - HEIGHT - 12.0,
            ),
            None => (100.0, 100.0),
        };
        hud.panel.setFrame_display(NSRect::new(NSPoint::new(x, y), NSSize::new(width, HEIGHT)), true);
        hud.label.setFrameOrigin(NSPoint::new(
            ((width - label_size.width) / 2.0).round(),
            ((HEIGHT - label_size.height) / 2.0).round(),
        ));
        hud.panel.orderFrontRegardless();
        hud.panel.invalidateShadow();
    });
}

fn build(mtm: MainThreadMarker) -> Hud {
    let rect = NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(160.0, HEIGHT));
    let panel = NSPanel::initWithContentRect_styleMask_backing_defer(
        NSPanel::alloc(mtm),
        rect,
        NSWindowStyleMask::Borderless | NSWindowStyleMask::NonactivatingPanel,
        NSBackingStoreType::Buffered,
        false,
    );
    unsafe { panel.setReleasedWhenClosed(false) };
    panel.setLevel(NSStatusWindowLevel);
    panel.setOpaque(false);
    panel.setBackgroundColor(Some(&NSColor::clearColor()));
    panel.setHasShadow(true);
    panel.setIgnoresMouseEvents(true);
    panel.setHidesOnDeactivate(false);
    panel.setCollectionBehavior(
        NSWindowCollectionBehavior::CanJoinAllSpaces
            | NSWindowCollectionBehavior::FullScreenAuxiliary
            | NSWindowCollectionBehavior::Stationary
            | NSWindowCollectionBehavior::IgnoresCycle,
    );

    let effect = NSVisualEffectView::initWithFrame(NSVisualEffectView::alloc(mtm), rect);
    effect.setMaterial(NSVisualEffectMaterial::HUDWindow);
    effect.setBlendingMode(NSVisualEffectBlendingMode::BehindWindow);
    effect.setState(NSVisualEffectState::Active);
    effect.setWantsLayer(true);
    unsafe {
        let layer: Option<Retained<AnyObject>> = msg_send![&*effect, layer];
        if let Some(layer) = layer {
            let _: () = msg_send![&*layer, setCornerRadius: RADIUS];
            let _: () = msg_send![&*layer, setMasksToBounds: Bool::YES];
        }
    }

    let label = NSTextField::labelWithString(&NSString::from_str(""), mtm);
    label.setFont(Some(&NSFont::systemFontOfSize_weight(13.0, unsafe { NSFontWeightMedium })));
    label.setTextColor(Some(&NSColor::whiteColor()));
    label.setAlignment(NSTextAlignment::Center);
    effect.addSubview(&label);
    panel.setContentView(Some(&effect));

    Hud { panel, label }
}
