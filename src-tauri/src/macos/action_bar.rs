//! Rewrite bar: a small floating row of buttons shown after a reply.
//!
//! [Mode ▾] Shorter ⌥1 · Friendlier ⌥2 · Professional ⌥3 · Explain more ⌥4 · Regenerate ⌥5 · ★ Save · ✕
//!
//! Like the HUD it is a native non-activating NSPanel: clicking it does not
//! take keyboard focus away from the reply box, so the new version can be
//! pasted straight back. It hides itself after a short while.

use crate::flow::{self, Followup};
use crate::modes::Mode;
use crate::rewrite::RewriteAction;
use crate::shortcut;
use crate::state::AppState;
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, Bool};
use objc2::{define_class, msg_send, sel, MainThreadMarker, MainThreadOnly};
use objc2_app_kit::{
    NSAppearance, NSAppearanceCustomization, NSAppearanceNameDarkAqua, NSBackingStoreType, NSBezelStyle, NSButton,
    NSColor, NSControlSize, NSFont, NSPanel, NSPopUpButton, NSScreen, NSStatusWindowLevel, NSVisualEffectBlendingMode,
    NSVisualEffectMaterial, NSVisualEffectState, NSVisualEffectView, NSWindowCollectionBehavior, NSWindowStyleMask,
};
use objc2_foundation::{NSObject, NSPoint, NSRect, NSSize, NSString};
use std::cell::RefCell;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::OnceLock;
use std::time::Duration;
use tauri::{AppHandle, Manager};

const HEIGHT: f64 = 38.0;
const PAD: f64 = 8.0;
const GAP: f64 = 4.0;
const AUTO_HIDE: Duration = Duration::from_secs(30);

const TAG_SAVE: isize = 6;
const TAG_CLOSE: isize = 7;
const TAG_DETAILS: isize = 8;

/// Shows the "Diagnosis" button (replies that came from GitHub Debug).
static DETAILS: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

pub fn set_details_available(on: bool) {
    DETAILS.store(on, Ordering::SeqCst);
}

static APP: OnceLock<AppHandle> = OnceLock::new();
static GENERATION: AtomicU64 = AtomicU64::new(0);

define_class!(
    // SAFETY: NSObject has no subclassing requirements and we don't implement Drop.
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "PastePilotBarTarget"]
    struct BarTarget;

    impl BarTarget {
        #[unsafe(method(clicked:))]
        fn clicked(&self, sender: &NSButton) {
            on_button(sender.tag());
        }

        #[unsafe(method(modeChanged:))]
        fn mode_changed(&self, sender: &NSPopUpButton) {
            on_mode(sender.indexOfSelectedItem());
        }
    }
);

define_class!(
    // SAFETY: NSButton can be subclassed; we only add acceptsFirstMouse.
    #[unsafe(super(NSButton, objc2_app_kit::NSControl, objc2_app_kit::NSView, objc2_app_kit::NSResponder, NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "PastePilotBarButton"]
    struct BarButton;

    impl BarButton {
        /// Respond to the first click even though our app is never active.
        #[unsafe(method(acceptsFirstMouse:))]
        fn accepts_first_mouse(&self, _event: Option<&AnyObject>) -> bool {
            true
        }
    }
);

define_class!(
    // SAFETY: NSPopUpButton can be subclassed; we only add acceptsFirstMouse.
    #[unsafe(super(NSPopUpButton, NSButton, objc2_app_kit::NSControl, objc2_app_kit::NSView, objc2_app_kit::NSResponder, NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "PastePilotBarPopUp"]
    struct BarPopUp;

    impl BarPopUp {
        #[unsafe(method(acceptsFirstMouse:))]
        fn accepts_first_mouse(&self, _event: Option<&AnyObject>) -> bool {
            true
        }
    }
);

struct Bar {
    panel: Retained<NSPanel>,
    content: Retained<NSVisualEffectView>,
    popup: Retained<BarPopUp>,
    buttons: Vec<(isize, Retained<BarButton>)>,
    _target: Retained<BarTarget>,
    mode: Mode,
}

thread_local! {
    static BAR: RefCell<Option<Bar>> = const { RefCell::new(None) };
}

/// Shows the bar for the reply that was just produced, with `mode` selected.
pub fn show(app: &AppHandle, mode: Mode) {
    let _ = APP.set(app.clone());
    let shortcuts_on = app.state::<AppState>().settings().rewrite_shortcuts;
    let generation = GENERATION.fetch_add(1, Ordering::SeqCst) + 1;
    let details = DETAILS.load(Ordering::SeqCst);
    let _ = app.run_on_main_thread(move || present(mode, shortcuts_on, details));
    shortcut::set_rewrite_keys(app, shortcuts_on);

    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(AUTO_HIDE).await;
        if GENERATION.load(Ordering::SeqCst) == generation {
            hide(&app);
        }
    });
}

pub fn hide(app: &AppHandle) {
    GENERATION.fetch_add(1, Ordering::SeqCst);
    let _ = app.run_on_main_thread(|| {
        BAR.with(|cell| {
            if let Some(bar) = cell.borrow().as_ref() {
                bar.panel.orderOut(None);
            }
        })
    });
    shortcut::set_rewrite_keys(app, false);
}

fn on_button(tag: isize) {
    let Some(app) = APP.get() else { return };
    match tag {
        1..=5 => flow::trigger_followup(app.clone(), Followup::Rewrite(RewriteAction::ALL[(tag - 1) as usize])),
        TAG_SAVE => flow::save_last_as_example(app),
        TAG_DETAILS => crate::debug::open_review(app),
        TAG_CLOSE => hide(app),
        _ => {}
    }
}

fn on_mode(index: isize) {
    let Some(app) = APP.get() else { return };
    let Some(mode) = usize::try_from(index).ok().and_then(|i| Mode::ALL.get(i).copied()) else { return };
    let changed = BAR.with(|cell| cell.borrow().as_ref().is_some_and(|b| b.mode != mode));
    if changed {
        flow::trigger_followup(app.clone(), Followup::Mode(mode));
    }
}

fn present(mode: Mode, shortcuts_on: bool, details: bool) {
    let Some(mtm) = MainThreadMarker::new() else { return };
    BAR.with(|cell| {
        let mut slot = cell.borrow_mut();
        let bar = slot.get_or_insert_with(|| build(mtm));
        bar.mode = mode;
        bar.popup.selectItemAtIndex(mode.index() as isize);

        for (tag, button) in &bar.buttons {
            if *tag == TAG_DETAILS {
                button.setHidden(!details);
            }
            if (1..=5).contains(tag) {
                let action = RewriteAction::ALL[(*tag - 1) as usize];
                let title = if shortcuts_on {
                    format!("{} {}", short_label(action), shortcut_hint(action))
                } else {
                    short_label(action).to_string()
                };
                button.setTitle(&NSString::from_str(&title));
            }
        }
        let width = layout(bar);
        let (x, y) = match NSScreen::mainScreen(mtm).map(|s| s.visibleFrame()) {
            // Just below the status pill at the top of the screen.
            Some(vf) => (vf.origin.x + (vf.size.width - width) / 2.0, vf.origin.y + vf.size.height - 34.0 - 12.0 - 8.0 - HEIGHT),
            None => (100.0, 100.0),
        };
        bar.panel.setFrame_display(NSRect::new(NSPoint::new(x, y), NSSize::new(width, HEIGHT)), true);
        bar.panel.orderFrontRegardless();
        bar.panel.invalidateShadow();
    });
}

fn short_label(action: RewriteAction) -> &'static str {
    match action {
        RewriteAction::Professional => "Professional",
        other => other.label(),
    }
}

fn shortcut_hint(action: RewriteAction) -> &'static str {
    match action {
        RewriteAction::Shorter => "⌥1",
        RewriteAction::Friendlier => "⌥2",
        RewriteAction::Professional => "⌥3",
        RewriteAction::ExplainMore => "⌥4",
        RewriteAction::Regenerate => "⌥5",
    }
}

/// Lays controls out left to right; returns the total width.
fn layout(bar: &Bar) -> f64 {
    let mut x = PAD;
    bar.popup.sizeToFit();
    let size = bar.popup.frame().size;
    bar.popup.setFrameOrigin(NSPoint::new(x, ((HEIGHT - size.height) / 2.0).round()));
    x += size.width + GAP * 2.0;
    for (_, button) in &bar.buttons {
        if button.isHidden() {
            continue;
        }
        button.sizeToFit();
        let size = button.frame().size;
        button.setFrameOrigin(NSPoint::new(x, ((HEIGHT - size.height) / 2.0).round()));
        x += size.width + GAP;
    }
    let width = (x - GAP + PAD).ceil();
    bar.content.setFrame(NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(width, HEIGHT)));
    width
}

fn build(mtm: MainThreadMarker) -> Bar {
    let rect = NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(600.0, HEIGHT));
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
    panel.setHidesOnDeactivate(false);
    panel.setCollectionBehavior(
        NSWindowCollectionBehavior::CanJoinAllSpaces
            | NSWindowCollectionBehavior::FullScreenAuxiliary
            | NSWindowCollectionBehavior::Stationary
            | NSWindowCollectionBehavior::IgnoresCycle,
    );
    if let Some(dark) = NSAppearance::appearanceNamed(unsafe { NSAppearanceNameDarkAqua }) {
        panel.setAppearance(Some(&dark));
    }

    let content = NSVisualEffectView::initWithFrame(NSVisualEffectView::alloc(mtm), rect);
    content.setMaterial(NSVisualEffectMaterial::HUDWindow);
    content.setBlendingMode(NSVisualEffectBlendingMode::BehindWindow);
    content.setState(NSVisualEffectState::Active);
    content.setWantsLayer(true);
    unsafe {
        let layer: Option<Retained<AnyObject>> = msg_send![&*content, layer];
        if let Some(layer) = layer {
            let _: () = msg_send![&*layer, setCornerRadius: 10.0f64];
            let _: () = msg_send![&*layer, setMasksToBounds: Bool::YES];
        }
    }

    let target: Retained<BarTarget> = {
        let this = BarTarget::alloc(mtm).set_ivars(());
        unsafe { msg_send![super(this), init] }
    };
    let font = NSFont::systemFontOfSize(12.0);

    let popup: Retained<BarPopUp> = {
        let this = BarPopUp::alloc(mtm).set_ivars(());
        unsafe { msg_send![super(this), initWithFrame: NSRect::ZERO, pullsDown: false] }
    };
    for mode in Mode::ALL {
        popup.addItemWithTitle(&NSString::from_str(mode.label()));
    }
    popup.setControlSize(NSControlSize::Small);
    popup.setFont(Some(&font));
    popup.setToolTip(Some(&NSString::from_str("Detected mode. Pick another to regenerate in that mode.")));
    unsafe {
        popup.setTarget(Some(&target));
        popup.setAction(Some(sel!(modeChanged:)));
    }
    content.addSubview(&popup);

    let mut buttons = Vec::new();
    let specs: Vec<(isize, String, String)> = RewriteAction::ALL
        .iter()
        .enumerate()
        .map(|(i, a)| (i as isize + 1, short_label(*a).to_string(), format!("{} ({})", a.label(), shortcut_hint(*a))))
        .chain([
            (TAG_DETAILS, "Diagnosis".to_string(), "See what GitHub Debug found: files, lines, diff".to_string()),
            (TAG_SAVE, "★ Save".to_string(), "Save this reply as a style example".to_string()),
            (TAG_CLOSE, "✕".to_string(), "Hide".to_string()),
        ])
        .collect();
    for (tag, title, tip) in specs {
        let button: Retained<BarButton> = {
            let this = BarButton::alloc(mtm).set_ivars(());
            unsafe { msg_send![super(this), initWithFrame: NSRect::ZERO] }
        };
        button.setTitle(&NSString::from_str(&title));
        button.setBezelStyle(NSBezelStyle::AccessoryBarAction);
        button.setControlSize(NSControlSize::Small);
        button.setFont(Some(&font));
        button.setTag(tag);
        button.setToolTip(Some(&NSString::from_str(&tip)));
        unsafe {
            button.setTarget(Some(&target));
            button.setAction(Some(sel!(clicked:)));
        }
        content.addSubview(&button);
        buttons.push((tag, button));
    }

    panel.setContentView(Some(&content));
    Bar { panel, content, popup, buttons, _target: target, mode: Mode::General }
}
