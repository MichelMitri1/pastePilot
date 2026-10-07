//! UI Automation: the Windows counterpart of the macOS Accessibility wrapper.
//!
//! Used to: read the selected text of the focused element, decide whether the
//! focused element is an editable text input, read and select text inside it,
//! and move focus back to a remembered input. Windows needs no permission for
//! this, so the "trusted" checks always pass.

use std::cell::OnceCell;
use std::sync::atomic::{AtomicU32, Ordering};
use windows::core::Interface;
use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED};
use windows::Win32::UI::Accessibility::{
    CUIAutomation, CUIAutomation8, IUIAutomation, IUIAutomation2, IUIAutomationElement, IUIAutomationTextPattern,
    IUIAutomationValuePattern, TextPatternRangeEndpoint_End, TextPatternRangeEndpoint_Start, TextUnit_Character,
    TreeScope_Descendants, UIA_ControlTypePropertyId, UIA_DocumentControlTypeId, UIA_EditControlTypeId,
    UIA_PaneControlTypeId, UIA_TextPatternId, UIA_ValuePatternId, UIA_WindowControlTypeId,
};

/// UI Automation call timeout; a hung app must never stall a reply.
static TIMEOUT_MS: AtomicU32 = AtomicU32::new(2000);

thread_local! {
    static UIA: OnceCell<Option<IUIAutomation>> = const { OnceCell::new() };
}

/// Runs `f` with this thread's UI Automation client, setting up COM on first use.
pub(super) fn with_uia<T>(f: impl FnOnce(&IUIAutomation) -> Option<T>) -> Option<T> {
    UIA.with(|cell| {
        let uia = cell.get_or_init(|| unsafe {
            // Already initialized (S_FALSE, or a different apartment) is fine.
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
            let uia: IUIAutomation = CoCreateInstance(&CUIAutomation8, None, CLSCTX_INPROC_SERVER)
                .or_else(|_| CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER))
                .ok()?;
            if let Ok(uia2) = uia.cast::<IUIAutomation2>() {
                let ms = TIMEOUT_MS.load(Ordering::Relaxed);
                let _ = uia2.SetConnectionTimeout(ms);
                let _ = uia2.SetTransactionTimeout(ms);
            }
            Some(uia)
        });
        f(uia.as_ref()?)
    })
}

/// No permission to grant on Windows.
pub fn is_trusted() -> bool {
    true
}

pub fn prompt_for_trust() -> bool {
    true
}

pub fn set_global_timeout(seconds: f32) {
    TIMEOUT_MS.store((seconds * 1000.0) as u32, Ordering::Relaxed);
}

/// A UI Automation element in another app.
#[derive(Clone)]
pub struct Element(IUIAutomationElement);

// Elements come from multithreaded-apartment clients, which COM lets any thread use.
unsafe impl Send for Element {}
unsafe impl Sync for Element {}

impl Element {
    fn text_pattern(&self) -> Option<IUIAutomationTextPattern> {
        unsafe { self.0.GetCurrentPatternAs(UIA_TextPatternId).ok() }
    }

    fn value_pattern(&self) -> Option<IUIAutomationValuePattern> {
        unsafe { self.0.GetCurrentPatternAs(UIA_ValuePatternId).ok() }
    }

    fn process_id(&self) -> Option<i32> {
        unsafe { self.0.CurrentProcessId().ok() }
    }
}

/// The focused UI element, if it belongs to the given app.
pub fn focused_element(pid: i32) -> Option<Element> {
    let el = with_uia(|uia| unsafe { uia.GetFocusedElement().ok() }).map(Element)?;
    (el.process_id()? == pid).then_some(el)
}

/// Selected text of the focused element, if the app exposes it.
/// When it doesn't, callers fall back to Ctrl+C.
pub fn selected_text(pid: i32) -> Option<String> {
    selected_text_in(&focused_element(pid)?).filter(|t| !t.trim().is_empty())
}

/// Selected text inside a specific element.
pub fn selected_text_in(el: &Element) -> Option<String> {
    unsafe {
        let ranges = el.text_pattern()?.GetSelection().ok()?;
        let mut text = String::new();
        for i in 0..ranges.Length().ok()? {
            text.push_str(&ranges.GetElement(i).ok()?.GetText(-1).ok()?.to_string());
        }
        Some(text)
    }
}

/// Whether the element still exists (e.g. the page hasn't removed the input).
pub fn is_alive(el: &Element) -> bool {
    unsafe { el.0.CurrentControlType().is_ok() }
}

/// Heuristic: can the user type into this element?
pub fn is_editable(el: &Element) -> bool {
    unsafe {
        if el.0.CurrentIsPassword().is_ok_and(|p| p.as_bool()) {
            return false; // never paste replies into password fields
        }
        let Ok(control) = el.0.CurrentControlType() else { return false };
        if control == UIA_WindowControlTypeId || control == UIA_PaneControlTypeId {
            return false;
        }
        match el.value_pattern() {
            Some(value) => value.CurrentIsReadOnly().is_ok_and(|r| !r.as_bool()),
            // Some rich editors only offer the Text pattern.
            None => control == UIA_EditControlTypeId && el.text_pattern().is_some(),
        }
    }
}

/// Move keyboard focus to the given element.
pub fn focus(el: &Element) -> bool {
    unsafe { el.0.SetFocus().is_ok() }
}

/// Title of the app's main window (in browsers: the tab title plus the browser name).
pub fn window_title(pid: i32) -> Option<String> {
    super::apps::window_title(super::apps::main_window(pid)?).filter(|t| !t.is_empty())
}

/// URL of the web page with focus, if the app is a browser.
pub fn page_url(pid: i32) -> Option<String> {
    document_url(pid).or_else(|| address_bar_url(pid))
}

/// Browsers report the page URL as the value of the web document.
fn document_url(pid: i32) -> Option<String> {
    let mut el = focused_element(pid)?;
    with_uia(|uia| unsafe {
        let walker = uia.ControlViewWalker().ok()?;
        for _ in 0..40 {
            if el.0.CurrentControlType().ok()? == UIA_DocumentControlTypeId {
                return value(&el).filter(|v| v.contains("://") && !v.contains(char::is_whitespace));
            }
            el = Element(walker.GetParentElement(&el.0).ok()?);
        }
        None
    })
}

/// Otherwise: the browser's address bar, the first text field in its window.
fn address_bar_url(pid: i32) -> Option<String> {
    let hwnd = super::apps::main_window(pid)?;
    let text = with_uia(|uia| unsafe {
        let window = uia.ElementFromHandle(hwnd).ok()?;
        let condition = uia.CreatePropertyCondition(UIA_ControlTypePropertyId, &UIA_EditControlTypeId.0.into()).ok()?;
        value(&Element(window.FindFirst(TreeScope_Descendants, &condition).ok()?))
    })?;
    let text = text.trim();
    // Chrome and Edge leave out "https://" while the page has focus.
    let looks_like_url = !text.is_empty() && !text.contains(char::is_whitespace) && (text.contains('.') || text.starts_with("localhost"));
    if text.contains("://") {
        Some(text.to_string())
    } else if looks_like_url {
        Some(format!("https://{text}"))
    } else {
        None
    }
}

/// Text content of an editable element.
pub fn value(el: &Element) -> Option<String> {
    unsafe {
        if let Some(value) = el.value_pattern() {
            return value.CurrentValue().ok().map(|v| v.to_string());
        }
        Some(el.text_pattern()?.DocumentRange().ok()?.GetText(-1).ok()?.to_string())
    }
}

/// Selects `length` characters starting at `location` inside a text element.
pub fn select_range(el: &Element, location: usize, length: usize) -> bool {
    let Some(pattern) = el.text_pattern() else { return false };
    unsafe {
        let Ok(range) = pattern.DocumentRange() else { return false };
        // Collapse to the start of the text, then move both ends into place.
        range.MoveEndpointByRange(TextPatternRangeEndpoint_End, &range, TextPatternRangeEndpoint_Start).is_ok()
            && range.MoveEndpointByUnit(TextPatternRangeEndpoint_End, TextUnit_Character, (location + length) as i32).is_ok()
            && range.MoveEndpointByUnit(TextPatternRangeEndpoint_Start, TextUnit_Character, location as i32).is_ok()
            && range.Select().is_ok()
    }
}
