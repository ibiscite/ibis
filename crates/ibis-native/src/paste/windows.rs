//! The Windows window a paste returns to (#1133).

use std::sync::{Arc, Mutex};

use winsafe as w;

/// winsafe handles are Send but neither Clone nor Sync. Share an immutable
/// HWND by borrowing it under a lock; no raw copying or unsafe is needed.
#[derive(Clone)]
pub struct Window(pub Arc<Mutex<w::HWND>>);
