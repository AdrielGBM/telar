//! Windows' answers: the client-area animation switch, the high-contrast flag and the user's UI languages, plus a listener for the broadcast that says one of them changed.

use std::sync::{Arc, Mutex, OnceLock, PoisonError};

use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::Globalization::{GetUserPreferredUILanguages, MUI_LANGUAGE_NAME};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::Accessibility::{HCF_HIGHCONTRASTON, HIGHCONTRASTW};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DispatchMessageW, GetMessageW, MSG, RegisterClassW,
    SPI_GETCLIENTAREAANIMATION, SPI_GETHIGHCONTRAST, SPI_SETCLIENTAREAANIMATION,
    SPI_SETHIGHCONTRAST, SystemParametersInfoW, TranslateMessage, WM_SETTINGCHANGE, WNDCLASSW,
    WS_OVERLAPPED,
};

use super::wide::{names_locale_settings, split_multi_sz};

/// "Show animations in Windows" switched off is the user asking for less motion.
pub fn reduced_motion() -> Option<bool> {
    let mut animations_on: windows_sys::core::BOOL = 0;
    let ok = unsafe {
        SystemParametersInfoW(
            SPI_GETCLIENTAREAANIMATION,
            0,
            (&mut animations_on as *mut windows_sys::core::BOOL).cast(),
            0,
        )
    };
    (ok != 0).then_some(animations_on == 0)
}

pub fn high_contrast() -> Option<bool> {
    let mut contrast = HIGHCONTRASTW {
        cbSize: std::mem::size_of::<HIGHCONTRASTW>() as u32,
        ..HIGHCONTRASTW::default()
    };
    let ok = unsafe {
        SystemParametersInfoW(
            SPI_GETHIGHCONTRAST,
            contrast.cbSize,
            (&mut contrast as *mut HIGHCONTRASTW).cast(),
            0,
        )
    };
    (ok != 0).then_some(contrast.dwFlags & HCF_HIGHCONTRASTON != 0)
}

/// The display languages in the user's order, as BCP 47 names (`es-CL`). Empty where the call fails.
pub fn locales() -> Vec<String> {
    let mut count = 0u32;
    let mut len = 0u32;
    let sized = unsafe {
        GetUserPreferredUILanguages(
            MUI_LANGUAGE_NAME,
            &mut count,
            std::ptr::null_mut(),
            &mut len,
        )
    };
    if sized == 0 || len == 0 {
        return Vec::new();
    }
    let mut buffer = vec![0u16; len as usize];
    let filled = unsafe {
        GetUserPreferredUILanguages(MUI_LANGUAGE_NAME, &mut count, buffer.as_mut_ptr(), &mut len)
    };
    if filled == 0 {
        return Vec::new();
    }
    split_multi_sz(&buffer)
}

static ON_CHANGE: Mutex<Option<Arc<dyn Fn() + Send + Sync>>> = Mutex::new(None);
static WATCHER: OnceLock<()> = OnceLock::new();

/// Calls `on_change` whenever Windows broadcasts a setting that can move a preference, until the next call replaces it.
///
/// winit's `with_msg_hook` cannot do this: it sees only messages `PeekMessageW` returns, which are the posted ones, while `WM_SETTINGCHANGE` is broadcast with `SendMessageTimeout` and handed straight to each top-level window procedure inside that call. So the listener is a window of its own: hidden, top-level (a message-only window is left out of broadcasts), and pumped by a thread of its own so it depends on nothing winit does.
pub fn watch(on_change: impl Fn() + Send + Sync + 'static) {
    *ON_CHANGE.lock().unwrap_or_else(PoisonError::into_inner) = Some(Arc::new(on_change));
    WATCHER.get_or_init(|| {
        let _ = std::thread::Builder::new()
            .name("telar-system-preferences".to_string())
            .spawn(|| unsafe { pump_listener() });
    });
}

unsafe fn pump_listener() {
    let class_name: Vec<u16> = "TelarSystemPreferences\0".encode_utf16().collect();
    let instance = unsafe { GetModuleHandleW(std::ptr::null()) };
    let class = WNDCLASSW {
        lpfnWndProc: Some(listener_proc),
        hInstance: instance,
        lpszClassName: class_name.as_ptr(),
        ..unsafe { std::mem::zeroed() }
    };
    if unsafe { RegisterClassW(&class) } == 0 {
        return;
    }
    let window = unsafe {
        CreateWindowExW(
            0,
            class_name.as_ptr(),
            std::ptr::null(),
            WS_OVERLAPPED,
            0,
            0,
            0,
            0,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            instance,
            std::ptr::null(),
        )
    };
    if window.is_null() {
        return;
    }
    let mut message: MSG = unsafe { std::mem::zeroed() };
    while unsafe { GetMessageW(&mut message, std::ptr::null_mut(), 0, 0) } > 0 {
        unsafe {
            TranslateMessage(&message);
            DispatchMessageW(&message);
        }
    }
}

unsafe extern "system" fn listener_proc(
    window: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    let relevant = match message {
        WM_SETTINGCHANGE => {
            matches!(
                wparam as u32,
                SPI_SETCLIENTAREAANIMATION | SPI_SETHIGHCONTRAST
            ) || unsafe { names_locale_settings(&area(lparam)) }
        }
        _ => false,
    };
    if relevant {
        let on_change = ON_CHANGE
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        if let Some(on_change) = on_change {
            on_change();
        }
    }
    unsafe { DefWindowProcW(window, message, wparam, lparam) }
}

// The area is a NUL-terminated wide string or null; the cap only guards against a sender that forgot the terminator.
unsafe fn area(lparam: LPARAM) -> Vec<u16> {
    let start = lparam as *const u16;
    if start.is_null() {
        return Vec::new();
    }
    (0..256)
        .map(|offset| unsafe { *start.add(offset) })
        .take_while(|&unit| unit != 0)
        .collect()
}
