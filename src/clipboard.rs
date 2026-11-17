//! Work around gpui-pre-windows 0.3.12 opening the write clipboard with no owner.
//! Windows requires a non-null owner after EmptyClipboard for SetClipboardData.
use gpui_kit::{App, Window};

#[cfg(windows)]
pub fn copy_text(window: &Window, _cx: &mut App, text: &str) -> anyhow::Result<()> {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use windows::Win32::{
        Foundation::{GlobalFree, HANDLE, HWND},
        System::{
            DataExchange::{CloseClipboard, EmptyClipboard, OpenClipboard, SetClipboardData},
            Memory::{GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalUnlock},
            Ole::CF_UNICODETEXT,
        },
    };
    let RawWindowHandle::Win32(handle) = HasWindowHandle::window_handle(window)?.as_raw() else {
        anyhow::bail!("Expected a Windows window handle");
    };
    let hwnd = HWND(handle.hwnd.get() as *mut _);
    struct ClipboardGuard;
    impl Drop for ClipboardGuard {
        fn drop(&mut self) {
            unsafe {
                let _ = CloseClipboard();
            }
        }
    }
    // SAFETY: the HWND is borrowed from the live GPUI window for this synchronous
    // call. The movable allocation is initialized with a null-terminated UTF-16
    // string. On success its ownership transfers to Windows; errors free it.
    unsafe {
        OpenClipboard(Some(hwnd))?;
        let _guard = ClipboardGuard;
        let wide: Vec<u16> = text
            .replace('\0', " ")
            .encode_utf16()
            .chain(Some(0))
            .collect();
        let allocation = GlobalAlloc(GMEM_MOVEABLE, wide.len() * 2)?;
        let pointer = GlobalLock(allocation);
        if pointer.is_null() {
            let _ = GlobalFree(Some(allocation));
            anyhow::bail!("Could not lock clipboard memory");
        }
        std::ptr::copy_nonoverlapping(wide.as_ptr(), pointer.cast(), wide.len());
        let _ = GlobalUnlock(allocation);
        if let Err(error) = EmptyClipboard() {
            let _ = GlobalFree(Some(allocation));
            return Err(error.into());
        }
        if let Err(error) = SetClipboardData(CF_UNICODETEXT.0 as u32, Some(HANDLE(allocation.0))) {
            let _ = GlobalFree(Some(allocation));
            return Err(error.into());
        }
    }
    Ok(())
}

#[cfg(not(windows))]
pub fn copy_text(_window: &Window, cx: &mut App, text: &str) -> anyhow::Result<()> {
    cx.write_to_clipboard(gpui_kit::ClipboardItem::new_string(text.to_string()));
    Ok(())
}
