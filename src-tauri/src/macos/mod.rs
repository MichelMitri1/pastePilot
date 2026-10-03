//! Native macOS functionality. Everything that talks to AppKit, Accessibility,
//! CoreGraphics events or the pasteboard lives under this module.

pub mod action_bar;
pub mod apps;
pub mod ax;
pub mod focus_tracker;
pub mod hud;
pub mod keys;
pub mod pasteboard;
