//! The only unsafe code in the project: the Win32 message loop the tray icon
//! needs, and a way to wake it from another thread.
#![allow(unsafe_code)]

use std::ops::ControlFlow;

use windows_sys::Win32::System::Threading::GetCurrentThreadId;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, GetMessageW, MSG, PostThreadMessageW, TranslateMessage, WM_APP,
};

#[derive(Clone, Copy)]
pub struct LoopHandle(u32);

impl LoopHandle {
    pub fn current() -> Self {
        // SAFETY: no arguments, cannot fail.
        Self(unsafe { GetCurrentThreadId() })
    }

    /// Makes the loop on the owning thread call its callback soon.
    pub fn wake(self) {
        // SAFETY: posting a message with no pointers to a thread id is always
        // sound; it fails harmlessly if the thread has already exited.
        unsafe { PostThreadMessageW(self.0, WM_APP, 0, 0) };
    }
}

/// Pumps messages until `after_each` returns `Break` or the queue reports quit.
pub fn run(mut after_each: impl FnMut() -> ControlFlow<()>) {
    // SAFETY: MSG is plain data and valid when zeroed.
    let mut msg: MSG = unsafe { std::mem::zeroed() };
    loop {
        // SAFETY: `msg` is a valid, writable MSG for the duration of each call.
        let got = unsafe { GetMessageW(&raw mut msg, std::ptr::null_mut(), 0, 0) };
        if got <= 0 {
            return;
        }
        // SAFETY: `msg` was just filled in by GetMessageW.
        unsafe {
            TranslateMessage(&raw const msg);
            DispatchMessageW(&raw const msg);
        }
        if after_each().is_break() {
            return;
        }
    }
}
