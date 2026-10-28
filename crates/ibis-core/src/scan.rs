//! A shared linear work allowance for pasted-input lookahead and nested readers.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Stopped {
    Cancelled,
    Limit,
}

pub(crate) struct Work<'a> {
    left: usize,
    used: usize,
    until_poll: usize,
    stop: &'a dyn Fn() -> bool,
    stopped: Option<Stopped>,
}

impl<'a> Work<'a> {
    pub(crate) fn new(bytes: usize, stop: &'a dyn Fn() -> bool) -> Self {
        Self {
            left: bytes.saturating_mul(32),
            used: 0,
            until_poll: 256,
            stop,
            stopped: stop().then_some(Stopped::Cancelled),
        }
    }

    pub(crate) fn check(&self) -> Result<(), Stopped> {
        self.stopped.map_or(Ok(()), Err)
    }

    /// Charge scans before opaque operations, or one character at a time in
    /// readers. No nested reader gets a fresh allowance or cancellation clock.
    pub(crate) fn spend(&mut self, mut bytes: usize) -> bool {
        if self.stopped.is_some() {
            return false;
        }
        if bytes > self.left {
            self.stopped = Some(Stopped::Limit);
            return false;
        }
        while bytes > 0 {
            let step = bytes.min(self.until_poll);
            self.left -= step;
            self.used += step;
            self.until_poll -= step;
            bytes -= step;
            if self.until_poll == 0 {
                self.until_poll = 256;
                if (self.stop)() {
                    self.stopped = Some(Stopped::Cancelled);
                    return false;
                }
            }
        }
        true
    }

    pub(crate) fn find(&mut self, text: &str, predicate: impl Fn(char) -> bool) -> Option<usize> {
        for (i, c) in text.char_indices() {
            if !self.spend(c.len_utf8()) {
                return None;
            }
            if predicate(c) {
                return Some(i);
            }
        }
        None
    }

    pub(crate) fn find_str(&mut self, text: &str, needle: &str) -> Option<usize> {
        for (i, c) in text.char_indices() {
            if !self.spend(c.len_utf8()) {
                return None;
            }
            if text[i..].starts_with(needle) {
                return Some(i);
            }
        }
        None
    }

    pub(crate) fn trim_start<'s>(&mut self, text: &'s str) -> &'s str {
        let i = self.find(text, |c| !c.is_whitespace()).unwrap_or(text.len());
        &text[i..]
    }

    #[cfg(test)]
    pub(crate) fn used(&self) -> usize {
        self.used
    }
}
