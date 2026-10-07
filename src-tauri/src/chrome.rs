//! Native caption tinting. Windows paints the title bar itself, so keeping it the
//! same colour as the UI means pushing the theme's palette through DWM.

use tauri::WebviewWindow;

#[cfg(windows)]
#[link(name = "dwmapi")]
extern "system" {
    fn DwmSetWindowAttribute(hwnd: *mut core::ffi::c_void, attr: u32, value: *const u32, len: u32) -> i32;
}

#[cfg(windows)]
const USE_DARK_MODE: u32 = 20;
#[cfg(windows)]
const BORDER_COLOR: u32 = 34;
#[cfg(windows)]
const CAPTION_COLOR: u32 = 35;
#[cfg(windows)]
const TEXT_COLOR: u32 = 36;

/// Colours are COLORREF (`0x00BBGGRR`), supplied by the front end from the live CSS
/// tokens so the two can never drift. 34/35/36 need Windows 11 — older builds reject
/// them and keep their own frame, which is why every call is best-effort.
pub fn apply(window: &WebviewWindow, dark: bool, caption: u32, text: u32, border: u32) {
    #[cfg(windows)]
    {
        let Ok(hwnd) = window.hwnd() else { return };
        let hwnd = hwnd.0;
        let dark = u32::from(dark);
        unsafe {
            DwmSetWindowAttribute(hwnd, USE_DARK_MODE, &dark, 4);
            for (attr, value) in [(CAPTION_COLOR, caption), (TEXT_COLOR, text), (BORDER_COLOR, border)] {
                DwmSetWindowAttribute(hwnd, attr, &value, 4);
            }
        }
    }
    #[cfg(not(windows))]
    let _ = (window, dark, caption, text, border);
}

/// Mirror the breadcrumb into the caption so the product name is not shown twice.
pub fn title(window: &WebviewWindow, text: &str) {
    let _ = window.set_title(text);
}
