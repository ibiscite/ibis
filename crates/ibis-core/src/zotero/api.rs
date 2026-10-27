//! Talking to Zotero's local API while Zotero runs.
//!
//! Sending the requests is left to a [`Transport`], so this crate never
//! touches the network. Zotero serves the API on
//! `http://127.0.0.1:23119/api/` once "Allow other applications on this
//! computer to communicate with Zotero" is on.

use thiserror::Error;

use super::Library;

/// A response from Zotero. Header names are matched without regard to case.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Response {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl Response {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.trim())
    }
}

/// Sends one GET request to the local API. `path` starts after `/api/`, for
/// example `users/0/items?limit=1`.
pub trait Transport {
    fn get(&mut self, path: &str) -> Result<Response, TransportError>;
}

/// Why a request could not be completed. Never carries payloads or
/// operating system messages.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum TransportError {
    #[error("Zotero is not running, or it is not accepting connections")]
    Unreachable,
    #[error("Zotero did not answer in time")]
    TimedOut,
    #[error("Zotero's answer could not be read")]
    Malformed,
    #[error("Zotero's answer was too large")]
    TooLarge,
}

/// Why a library could not be read. Messages are fixed text for users and
/// diagnostics; they never include library content.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum ZoteroError {
    #[error(transparent)]
    Transport(#[from] TransportError),
    #[error(
        "Zotero refused the connection. In Zotero, open Settings > Advanced and turn on \
         \"Allow other applications on this computer to communicate with Zotero\""
    )]
    NotAllowed,
    #[error("this Zotero group library was not found; check that you are still a member")]
    GroupNotFound,
    #[error("Zotero answered with an unexpected status ({0})")]
    Status(u16),
    #[error("this version of Zotero speaks a different API version; update Zotero or Ibis")]
    UnsupportedApi,
    #[error("Zotero's answer was incomplete")]
    Incomplete,
    #[error("the library kept changing while it was read; Ibis will try again")]
    Changing,
    #[error("reading Zotero exceeded Ibis's size limit")]
    TooLarge,
}

/// Whether Zotero is there to read from, found without reading any library.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Presence {
    /// Zotero is open and lets other applications read it.
    Available,
    /// Zotero is open, but "Allow other applications on this computer to
    /// communicate with Zotero" is off.
    AccessOff,
    /// Nothing answered as Zotero.
    Absent,
}

/// Ask Zotero's local API root, which answers with fixed text and reads no
/// library.
pub fn presence(transport: &mut impl Transport) -> Presence {
    match transport.get("") {
        Ok(response) if response.status == 200 => Presence::Available,
        Ok(response) if response.status == 403 => Presence::AccessOff,
        _ => Presence::Absent,
    }
}

pub(super) fn request(transport: &mut impl Transport, library: Library, path: &str) -> Result<Response, ZoteroError> {
    checked(transport.get(path)?, library)
}

fn checked(response: Response, library: Library) -> Result<Response, ZoteroError> {
    match response.status {
        200 => {}
        403 => return Err(ZoteroError::NotAllowed),
        404 if matches!(library, Library::Group(_)) => return Err(ZoteroError::GroupNotFound),
        status => return Err(ZoteroError::Status(status)),
    }
    if response.header("Zotero-API-Version") != Some("3") {
        return Err(ZoteroError::UnsupportedApi);
    }
    Ok(response)
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;

    use super::*;

    /// A scripted Zotero: answers requests in order and records them.
    #[derive(Default)]
    struct Fake {
        answers: VecDeque<Result<Response, TransportError>>,
        asked: Vec<String>,
    }

    impl Fake {
        fn answer(mut self, response: Response) -> Self {
            self.answers.push_back(Ok(response));
            self
        }
        fn fail(mut self, error: TransportError) -> Self {
            self.answers.push_back(Err(error));
            self
        }
    }

    impl Transport for Fake {
        fn get(&mut self, path: &str) -> Result<Response, TransportError> {
            self.asked.push(path.to_string());
            self.answers.pop_front().unwrap_or(Err(TransportError::Unreachable))
        }
    }

    #[test]
    fn presence_is_read_from_the_api_root_without_reading_a_library() {
        let answer = |status| Response {
            status,
            ..Response::default()
        };
        let mut open = Fake::default().answer(answer(200));
        assert_eq!(presence(&mut open), Presence::Available);
        assert_eq!(open.asked, [""]);
        assert_eq!(presence(&mut Fake::default().answer(answer(403))), Presence::AccessOff);
        assert_eq!(presence(&mut Fake::default().answer(answer(404))), Presence::Absent);
        assert_eq!(
            presence(&mut Fake::default().fail(TransportError::Unreachable)),
            Presence::Absent
        );
    }

    #[test]
    fn refused_missing_and_foreign_answers_are_explained() {
        let answer = |status: u16, version: &str| Response {
            status,
            headers: vec![("zotero-api-version".into(), version.into())],
            body: Vec::new(),
        };
        let ask = |response: Response, library: Library| {
            request(&mut Fake::default().answer(response), library, "users/0/items").map(|_| ())
        };
        assert_eq!(ask(answer(200, "3"), Library::User), Ok(()));
        assert_eq!(ask(answer(403, "3"), Library::User), Err(ZoteroError::NotAllowed));
        assert_eq!(
            ask(answer(404, "3"), Library::Group(5)),
            Err(ZoteroError::GroupNotFound)
        );
        assert_eq!(ask(answer(404, "3"), Library::User), Err(ZoteroError::Status(404)));
        assert_eq!(ask(answer(302, "3"), Library::User), Err(ZoteroError::Status(302)));
        assert_eq!(ask(answer(200, "2"), Library::User), Err(ZoteroError::UnsupportedApi));
        assert_eq!(
            request(
                &mut Fake::default().fail(TransportError::Unreachable),
                Library::User,
                ""
            )
            .map(|_| ()),
            Err(ZoteroError::Transport(TransportError::Unreachable))
        );
        assert!(ZoteroError::NotAllowed.to_string().contains("Allow other applications"));
    }
}
