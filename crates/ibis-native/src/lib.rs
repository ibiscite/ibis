//! Ibis's native calls, outside any window toolkit, so the desktop app, a
//! launcher or the command line can share them (decision 0029). Every
//! `unsafe` block of the paste proofs lives here, in modules that allow it
//! one by one; the host supplies windows, settings and the keyboard layout.

pub mod paste;
