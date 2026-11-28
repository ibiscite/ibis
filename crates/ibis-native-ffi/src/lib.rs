//! In-process AppKit effects. No daemon or citation engine code is linked here.
#![cfg(target_os = "macos")]

mod clipboard;
pub use clipboard::{macos_html, publish_macos};

#[allow(unsafe_code)]
mod bridge;
