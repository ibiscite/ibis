//! The only focused-field metadata a paste may request. Never values,
//! selections, titles or an accessibility-tree walk.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Attribute {
    Role,
    Subrole,
}

impl Attribute {
    pub(super) fn name(self) -> &'static str {
        match self {
            Self::Role => "AXRole",
            Self::Subrole => "AXSubrole",
        }
    }
}

pub(super) trait Accessibility {
    type Element;

    /// AXFocusedUIElement of this application, never the system-wide focus.
    fn focused(&self, pid: i32) -> Option<Self::Element>;
    fn attribute(&self, element: &Self::Element, attribute: Attribute) -> Option<String>;
}

/// A missing, mistyped or failed attribute cannot prove that a field is safe.
pub(super) fn secure_field(access: &impl Accessibility, pid: i32) -> Option<bool> {
    let field = access.focused(pid)?;
    for attribute in [Attribute::Role, Attribute::Subrole] {
        if access.attribute(&field, attribute)? == "AXSecureTextField" {
            return Some(true);
        }
    }
    Some(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    struct Fake {
        role: Option<&'static str>,
        subrole: Option<&'static str>,
        requests: RefCell<Vec<String>>,
    }

    impl Accessibility for Fake {
        type Element = ();

        fn focused(&self, pid: i32) -> Option<()> {
            assert_eq!(pid, 42);
            self.requests.borrow_mut().push("AXFocusedUIElement".into());
            Some(())
        }

        fn attribute(&self, _: &(), attribute: Attribute) -> Option<String> {
            self.requests.borrow_mut().push(attribute.name().into());
            match attribute {
                Attribute::Role => self.role,
                Attribute::Subrole => self.subrole,
            }
            .map(str::to_owned)
        }
    }

    #[test]
    fn only_the_captured_apps_focus_role_and_subrole_are_read() {
        for (role, subrole, expected, names) in [
            (
                Some("AXTextField"),
                Some("AXSecureTextField"),
                Some(true),
                vec!["AXFocusedUIElement", "AXRole", "AXSubrole"],
            ),
            (
                Some("AXSecureTextField"),
                None,
                Some(true),
                vec!["AXFocusedUIElement", "AXRole"],
            ),
            (
                Some("AXTextArea"),
                Some("AXUnknown"),
                Some(false),
                vec!["AXFocusedUIElement", "AXRole", "AXSubrole"],
            ),
            (None, Some("AXUnknown"), None, vec!["AXFocusedUIElement", "AXRole"]),
            (
                Some("AXTextField"),
                None,
                None,
                vec!["AXFocusedUIElement", "AXRole", "AXSubrole"],
            ),
        ] {
            let access = Fake {
                role,
                subrole,
                requests: RefCell::default(),
            };
            assert_eq!(secure_field(&access, 42), expected);
            assert_eq!(*access.requests.borrow(), names);
        }
    }
}
