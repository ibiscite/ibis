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
    Styles,
    Schema,
    Call,
}
impl Method {
    pub const ALL: [Self; 7] = [
        Self::Libraries,
        Self::Search,
        Self::Show,
        Self::Cite,
        Self::Styles,
        Self::Schema,
        Self::Call,
    ];
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Libraries => "libraries",
            Self::Search => "search",
            Self::Show => "show",
            Self::Cite => "cite",
            Self::Styles => "styles",
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
