/** Key names in each platform's own notation: "⌥R" on macOS, "Alt+R" on Windows. */
export const isMac = navigator.userAgent.includes("Mac");
export const ALT = isMac ? "⌥" : "Alt+";
export const CMD = isMac ? "⌘" : "Ctrl+";
export const SHIFT = isMac ? "⇧" : "Shift+";
export const ENTER = isMac ? "↩" : "Enter";

/** ⌘ on macOS, Ctrl on Windows. */
export const modKey = (e: { metaKey: boolean; ctrlKey: boolean }) => (isMac ? e.metaKey : e.ctrlKey);

/** Where the app's menu lives. */
export const MENU = isMac ? "menu bar" : "tray menu";
