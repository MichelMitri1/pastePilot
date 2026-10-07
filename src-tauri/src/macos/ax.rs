//! Minimal safe wrapper over the macOS Accessibility (AX) C API.
//!
//! Used to: check/request the Accessibility permission, read the selected
//! text of the focused element, decide whether the focused element is an
//! editable text input, and move focus back to a remembered input.

use core_foundation::base::{CFGetTypeID, CFRange, CFRelease, CFRetain, CFType, CFTypeID, CFTypeRef, TCFType};
use core_foundation::boolean::CFBoolean;
use core_foundation::dictionary::{CFDictionary, CFDictionaryRef};
use core_foundation::string::{CFString, CFStringRef};
use core_foundation::url::CFURL;
use std::ffi::c_void;

pub type AXUIElementRef = CFTypeRef;
pub type AXObserverRef = CFTypeRef;
pub type AXError = i32;
pub type AXObserverCallback =
    extern "C" fn(observer: AXObserverRef, element: AXUIElementRef, notification: CFStringRef, refcon: *mut c_void);

const AX_SUCCESS: AXError = 0;

#[link(name = "ApplicationServices", kind = "framework")]
extern "C" {
    static kAXTrustedCheckOptionPrompt: CFStringRef;
    fn AXIsProcessTrusted() -> bool;
    fn AXIsProcessTrustedWithOptions(options: CFDictionaryRef) -> bool;
    fn AXUIElementGetTypeID() -> CFTypeID;
    fn AXUIElementCreateSystemWide() -> AXUIElementRef;
    fn AXUIElementCreateApplication(pid: i32) -> AXUIElementRef;
    fn AXUIElementCopyAttributeValue(el: AXUIElementRef, attr: CFStringRef, value: *mut CFTypeRef) -> AXError;
    fn AXUIElementSetAttributeValue(el: AXUIElementRef, attr: CFStringRef, value: CFTypeRef) -> AXError;
    fn AXUIElementIsAttributeSettable(el: AXUIElementRef, attr: CFStringRef, settable: *mut u8) -> AXError;
    fn AXUIElementSetMessagingTimeout(el: AXUIElementRef, seconds: f32) -> AXError;
    pub fn AXObserverCreate(pid: i32, callback: AXObserverCallback, out: *mut AXObserverRef) -> AXError;
    pub fn AXObserverAddNotification(
        observer: AXObserverRef,
        el: AXUIElementRef,
        notification: CFStringRef,
        refcon: *mut c_void,
    ) -> AXError;
    pub fn AXObserverGetRunLoopSource(observer: AXObserverRef) -> CFTypeRef;
    fn AXValueCreate(value_type: u32, value: *const c_void) -> CFTypeRef;
}

const AX_VALUE_TYPE_CFRANGE: u32 = 4;

/// Is this process allowed to use Accessibility (and post synthetic key events)?
pub fn is_trusted() -> bool {
    unsafe { AXIsProcessTrusted() }
}

/// Like `is_trusted`, but makes macOS show its "allow accessibility" prompt
/// (and adds the app to the list in System Settings) when not yet trusted.
pub fn prompt_for_trust() -> bool {
    unsafe {
        let key = CFString::wrap_under_get_rule(kAXTrustedCheckOptionPrompt);
        let opts = CFDictionary::from_CFType_pairs(&[(key.as_CFType(), CFBoolean::true_value().as_CFType())]);
        AXIsProcessTrustedWithOptions(opts.as_concrete_TypeRef())
    }
}

/// Default AX IPC timeout is 6s; a hung app must never stall a reply that long.
pub fn set_global_timeout(seconds: f32) {
    let sys = Element::system_wide();
    unsafe { AXUIElementSetMessagingTimeout(sys.0, seconds) };
}

/// An owned (+1 retained) AXUIElementRef.
pub struct Element(AXUIElementRef);

// AX elements are opaque CF handles to another process; the AX API is safe to
// call from any thread.
unsafe impl Send for Element {}
unsafe impl Sync for Element {}

impl Drop for Element {
    fn drop(&mut self) {
        unsafe { CFRelease(self.0) }
    }
}

impl Clone for Element {
    fn clone(&self) -> Self {
        unsafe { CFRetain(self.0) };
        Element(self.0)
    }
}

impl Element {
    pub fn system_wide() -> Self {
        Element(unsafe { AXUIElementCreateSystemWide() })
    }

    pub fn application(pid: i32) -> Self {
        Element(unsafe { AXUIElementCreateApplication(pid) })
    }

    /// Takes a +0 reference (e.g. from an observer callback) and retains it.
    pub unsafe fn retain_from(raw: AXUIElementRef) -> Option<Self> {
        if raw.is_null() {
            return None;
        }
        CFRetain(raw);
        Some(Element(raw))
    }

    pub fn raw(&self) -> AXUIElementRef {
        self.0
    }

    fn attr(&self, name: &str) -> Option<CFType> {
        let name = CFString::new(name);
        let mut value: CFTypeRef = std::ptr::null();
        let err = unsafe { AXUIElementCopyAttributeValue(self.0, name.as_concrete_TypeRef(), &mut value) };
        if err != AX_SUCCESS || value.is_null() {
            return None;
        }
        Some(unsafe { CFType::wrap_under_create_rule(value) })
    }

    pub fn attr_string(&self, name: &str) -> Option<String> {
        self.attr(name)?.downcast::<CFString>().map(|s| s.to_string())
    }

    pub fn attr_element(&self, name: &str) -> Option<Element> {
        let value = self.attr(name)?;
        let raw = value.as_CFTypeRef();
        if unsafe { CFGetTypeID(raw) != AXUIElementGetTypeID() } {
            return None;
        }
        unsafe { Element::retain_from(raw) }
    }

    /// String attribute that may be a CFString or a CFURL (e.g. AXURL, AXDocument).
    pub fn attr_url_or_string(&self, name: &str) -> Option<String> {
        let value = self.attr(name)?;
        if let Some(url) = value.downcast::<CFURL>() {
            return Some(url.get_string().to_string());
        }
        value.downcast::<CFString>().map(|s| s.to_string())
    }

    pub fn has_attr(&self, name: &str) -> bool {
        self.attr(name).is_some()
    }

    pub fn is_settable(&self, name: &str) -> bool {
        let name = CFString::new(name);
        let mut settable: u8 = 0;
        let err = unsafe { AXUIElementIsAttributeSettable(self.0, name.as_concrete_TypeRef(), &mut settable) };
        err == AX_SUCCESS && settable != 0
    }

    pub fn set_bool(&self, name: &str, value: bool) -> bool {
        let name = CFString::new(name);
        let v = if value { CFBoolean::true_value() } else { CFBoolean::false_value() };
        unsafe { AXUIElementSetAttributeValue(self.0, name.as_concrete_TypeRef(), v.as_CFTypeRef()) == AX_SUCCESS }
    }
}

/// The focused UI element inside the given application.
pub fn focused_element(pid: i32) -> Option<Element> {
    Element::application(pid).attr_element("AXFocusedUIElement")
}

/// Selected text of the focused element, if the app exposes it through AX.
/// Browsers often don't for page (non-input) selections; callers fall back to Cmd+C.
pub fn selected_text(pid: i32) -> Option<String> {
    let text = focused_element(pid)?.attr_string("AXSelectedText")?;
    if text.trim().is_empty() {
        None
    } else {
        Some(text)
    }
}

const TEXT_ROLES: &[&str] = &["AXTextField", "AXTextArea", "AXComboBox", "AXSearchField"];

/// Heuristic: can the user type into this element?
pub fn is_editable(el: &Element) -> bool {
    if el.attr_string("AXSubrole").as_deref() == Some("AXSecureTextField") {
        return false; // never paste replies into password fields
    }
    if let Some(role) = el.attr_string("AXRole") {
        if TEXT_ROLES.contains(&role.as_str()) {
            return true;
        }
        if role == "AXWebArea" || role == "AXWindow" || role == "AXApplication" {
            return false;
        }
    }
    // Rich-text editors in web apps (contenteditable) usually expose one of these.
    el.is_settable("AXValue") || el.has_attr("AXEditableAncestor")
}

/// Chromium and Electron only build their web accessibility tree when an
/// assistive app asks for it. Without this, browser text inputs are invisible to AX.
pub fn enable_web_accessibility(pid: i32) {
    let app = Element::application(pid);
    app.set_bool("AXManualAccessibility", true);
}

/// Bring an app to the front through AX (works even under cooperative activation).
pub fn raise_app(pid: i32) -> bool {
    Element::application(pid).set_bool("AXFrontmost", true)
}

/// Move keyboard focus to the given element.
pub fn focus(el: &Element) -> bool {
    el.set_bool("AXFocused", true)
}

/// Title of the app's focused window (in browsers: the tab title).
pub fn window_title(pid: i32) -> Option<String> {
    Element::application(pid).attr_element("AXFocusedWindow")?.attr_string("AXTitle").filter(|t| !t.is_empty())
}

/// URL of the web page with focus, if the app is a browser that exposes it.
pub fn page_url(pid: i32) -> Option<String> {
    let app = Element::application(pid);
    // Walk up from the focused element to the enclosing web area.
    if let Some(mut el) = app.attr_element("AXFocusedUIElement") {
        for _ in 0..40 {
            if el.attr_string("AXRole").as_deref() == Some("AXWebArea") {
                if let Some(url) = el.attr_url_or_string("AXURL") {
                    return Some(url);
                }
                break;
            }
            match el.attr_element("AXParent") {
                Some(parent) => el = parent,
                None => break,
            }
        }
    }
    // Some browsers expose the page URL on the window instead.
    app.attr_element("AXFocusedWindow")?.attr_url_or_string("AXDocument").filter(|u| u.contains("://"))
}

/// Selected text inside a specific element.
pub fn selected_text_in(el: &Element) -> Option<String> {
    el.attr_string("AXSelectedText")
}

/// Whether the element still exists (e.g. the page hasn't removed the input).
pub fn is_alive(el: &Element) -> bool {
    el.attr_string("AXRole").is_some()
}

/// Text content of an editable element.
pub fn value(el: &Element) -> Option<String> {
    el.attr_string("AXValue")
}

/// Selects `length` UTF-16 units starting at `location` inside a text element.
pub fn select_range(el: &Element, location: usize, length: usize) -> bool {
    let range = CFRange { location: location as isize, length: length as isize };
    unsafe {
        let value = AXValueCreate(AX_VALUE_TYPE_CFRANGE, &range as *const CFRange as *const c_void);
        if value.is_null() {
            return false;
        }
        let name = CFString::new("AXSelectedTextRange");
        let ok = AXUIElementSetAttributeValue(el.0, name.as_concrete_TypeRef(), value) == AX_SUCCESS;
        CFRelease(value);
        ok
    }
}
