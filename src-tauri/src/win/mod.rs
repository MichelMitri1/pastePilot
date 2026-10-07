//! Native Windows functionality: the counterpart of `macos/`, with the same
//! functions, built on UI Automation, Win32 input, the Win32 clipboard and
//! two small webview overlays for the status pill and the rewrite bar.

pub mod action_bar;
pub mod apps;
pub mod ax;
pub mod focus_tracker;
pub mod hud;
pub mod image;
pub mod keys;
pub mod overlay;
pub mod pasteboard;
