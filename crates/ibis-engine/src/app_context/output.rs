//! Listed output identities, shared by context and native delivery.
use crate::outputs::{self, OutputInfo};
use ibis_core::contract::OutputSpec;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
/// One of the engine's outputs. It crosses IPC and files as its id, and only
/// an id the engine lists becomes one.
#[derive(Clone, Copy)]
pub struct OutputRef(&'static OutputInfo);

impl OutputRef {
    pub fn id(self) -> &'static str {
        &self.0.id
    }
    pub fn spec(self) -> &'static OutputSpec {
        &self.0.output
    }
}

impl PartialEq for OutputRef {
    fn eq(&self, other: &Self) -> bool {
        self.id() == other.id()
    }
}
impl Eq for OutputRef {}

impl std::fmt::Debug for OutputRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.id())
    }
}

impl Serialize for OutputRef {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.id())
    }
}

impl<'de> Deserialize<'de> for OutputRef {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let id = String::deserialize(deserializer)?;
        listed(&id).ok_or_else(|| serde::de::Error::custom("not an output Ibis lists"))
    }
}

/// The listed output `id` names, if any.
pub fn listed(id: &str) -> Option<OutputRef> {
    outputs::find(id).map(OutputRef)
}

/// The output a stored id means: an id this Ibis does not list, or none,
/// fails closed to the engine's default.
pub fn stored(id: Option<&str>) -> OutputRef {
    OutputRef(outputs::or_default(id))
}

/// The style a formatted app's citation is written in: the default output
/// when it is a style, else the engine's default style.
pub fn default_style(default: OutputRef) -> &'static str {
    match default.spec() {
        OutputSpec::Csl { style } => style,
        _ => outputs::DEFAULT_STYLE,
    }
}

/// The listed output a writing project's stored output writes. A project
/// made in the command line can name a style the picker does not list; that is
/// refused, never replaced by a plausible wrong output.
pub fn of_writing(output: &OutputSpec) -> Result<OutputRef, String> {
    outputs::named(output).map(OutputRef).ok_or_else(|| {
        "This project's output is available in the command line. Choose a listed output there before using it in the picker."
            .into()
    })
}
