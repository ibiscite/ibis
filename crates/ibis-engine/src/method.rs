//! The method registry shared by typed engine calls and headless front ends.
use ibis_core::contract::ErrorCode;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Method {
    Libraries,
    Search,
    Show,
    Cite,
    Records,
    Project,
    Entry,
    Styles,
    Outputs,
    Schema,
    Call,
}
impl Method {
    pub const ALL: [Self; 11] = [
        Self::Libraries,
        Self::Search,
        Self::Show,
        Self::Cite,
        Self::Records,
        Self::Project,
        Self::Entry,
        Self::Styles,
        Self::Outputs,
        Self::Schema,
        Self::Call,
    ];
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Libraries => "libraries",
            Self::Search => "search",
            Self::Show => "show",
            Self::Cite => "cite",
            Self::Records => "records",
            Self::Project => "project",
            Self::Entry => "entry",
            Self::Styles => "styles",
            Self::Outputs => "outputs",
            Self::Schema => "schema",
            Self::Call => "call",
        }
    }
    pub fn parse(name: &str) -> Result<Self, ErrorCode> {
        Self::ALL
            .into_iter()
            .find(|method| method.as_str() == name)
            .ok_or(ErrorCode::Usage)
    }
}
