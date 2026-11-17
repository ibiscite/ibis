//! Once-only initial presentation policy, independent of native window APIs.

pub(crate) struct InitialPresentation {
    pending: bool,
    focused: bool,
    panel_ready: bool,
    page_loaded: bool,
}

impl InitialPresentation {
    pub(crate) fn new(visible: bool, focused: bool) -> Self {
        Self {
            pending: visible,
            focused,
            panel_ready: false,
            page_loaded: false,
        }
    }

    pub(crate) fn allows_automatic_blur(&self) -> bool {
        !self.pending
    }

    pub(crate) fn cancel(&mut self) {
        self.pending = false;
    }

    pub(crate) fn panel_ready(&mut self) -> Option<bool> {
        self.panel_ready = true;
        self.take_ready()
    }

    pub(crate) fn page_loaded(&mut self) -> Option<bool> {
        self.page_loaded = true;
        self.take_ready()
    }

    fn take_ready(&mut self) -> Option<bool> {
        if self.pending && self.panel_ready && self.page_loaded {
            self.pending = false;
            Some(self.focused)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn either_readiness_order_presents_once_with_original_focus_intent() {
        for focused in [false, true] {
            let mut startup = InitialPresentation::new(true, focused);
            assert!(!startup.allows_automatic_blur());
            assert_eq!(startup.panel_ready(), None);
            assert!(!startup.allows_automatic_blur());
            assert_eq!(startup.page_loaded(), Some(focused));
            assert!(startup.allows_automatic_blur());
            assert_eq!(startup.page_loaded(), None);
            assert_eq!(startup.panel_ready(), None);

            let mut startup = InitialPresentation::new(true, focused);
            assert_eq!(startup.page_loaded(), None);
            assert_eq!(startup.page_loaded(), None);
            assert_eq!(startup.panel_ready(), Some(focused));
            assert_eq!(startup.panel_ready(), None);
            assert_eq!(startup.page_loaded(), None);
        }
    }

    #[test]
    fn originally_hidden_windows_never_present_automatically() {
        for focused in [false, true] {
            let mut startup = InitialPresentation::new(false, focused);
            assert!(startup.allows_automatic_blur());
            assert_eq!(startup.page_loaded(), None);
            assert_eq!(startup.panel_ready(), None);
        }
    }

    #[test]
    fn explicit_show_or_dismiss_cancels_pending_presentation_at_every_stage() {
        for first in [None, Some(true), Some(false)] {
            let mut startup = InitialPresentation::new(true, true);
            match first {
                Some(true) => assert_eq!(startup.panel_ready(), None),
                Some(false) => assert_eq!(startup.page_loaded(), None),
                None => {}
            }
            assert!(!startup.allows_automatic_blur());
            startup.cancel();
            assert!(startup.allows_automatic_blur());
            assert_eq!(startup.panel_ready(), None);
            assert_eq!(startup.page_loaded(), None);
            assert_eq!(startup.page_loaded(), None);
        }
    }
}
