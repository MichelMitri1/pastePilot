//! The native OS layer. The rest of the app only talks to these modules, which
//! offer the same functions on macOS (`macos/`) and Windows (`win/`).

#[cfg(target_os = "macos")]
pub use crate::macos::{action_bar, apps, ax, focus_tracker, hud, image, keys, pasteboard};
#[cfg(windows)]
pub use crate::win::{action_bar, apps, ax, focus_tracker, hud, image, keys, pasteboard};

/// Key names for messages, in each platform's own notation: "⌥R" / "Alt+R".
pub mod kbd {
    #[cfg(target_os = "macos")]
    mod names {
        pub const ALT: &str = "⌥";
        pub const CMD: &str = "⌘";
        pub const SHIFT: &str = "⇧";
        pub const ENTER: &str = "↩";
    }
    #[cfg(windows)]
    mod names {
        pub const ALT: &str = "Alt+";
        pub const CMD: &str = "Ctrl+";
        pub const SHIFT: &str = "Shift+";
        pub const ENTER: &str = "Enter";
    }
    pub use names::*;
}

/// Opens a URL in the default browser.
pub fn open_url(url: &str) -> std::io::Result<()> {
    #[cfg(target_os = "macos")]
    std::process::Command::new("open").arg(url).spawn()?;
    // Not `cmd /c start`, which would treat & in the URL as a command separator.
    #[cfg(windows)]
    std::process::Command::new("rundll32").args(["url.dll,FileProtocolHandler", url]).spawn()?;
    Ok(())
}
