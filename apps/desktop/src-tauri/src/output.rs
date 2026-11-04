//! The engine's outputs as the app stores and shows them (#1260). Settings,
//! the per-app file and every command name an output by its engine id; this
//! module only looks ids up, so the app has no output vocabulary of its own.
use ibis_core::contract::OutputSpec;
use ibis_engine::outputs::{self, OutputInfo};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// One of the engine's outputs. It crosses IPC and files as its id, and only
/// an id the engine lists becomes one.
#[derive(Clone, Copy)]
pub(crate) struct OutputRef(&'static OutputInfo);

impl OutputRef {
    pub(crate) fn id(self) -> &'static str {
        &self.0.id
    }
    pub(crate) fn info(self) -> &'static OutputInfo {
        self.0
    }
    pub(crate) fn spec(self) -> &'static OutputSpec {
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
pub(crate) fn listed(id: &str) -> Option<OutputRef> {
    outputs::find(id).map(OutputRef)
}

/// The output a stored id means: an id this Ibis does not list, or none,
/// fails closed to the engine's default.
pub(crate) fn stored(id: Option<&str>) -> OutputRef {
    OutputRef(outputs::or_default(id))
}

/// The style a formatted app's citation is written in: the default output
/// when it is a style, else the engine's default style.
pub(crate) fn default_style(default: OutputRef) -> &'static str {
    match default.spec() {
        OutputSpec::Csl { style } => style,
        _ => outputs::DEFAULT_STYLE,
    }
}

/// A stored id read leniently: anything but a listed id, as a file from
/// another version may hold, is no choice at all.
pub(crate) fn lenient<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<OutputRef>, D::Error> {
    let value = serde_json::Value::deserialize(deserializer)?;
    Ok(value.as_str().and_then(listed))
}

/// One output as the web view shows it, from the engine's list at build time.
#[derive(Debug, Clone, Serialize, specta::Type)]
pub(crate) struct OutputChoice {
    id: String,
    /// The one name, in the header, footer, menus and Settings.
    name: String,
    /// A style's full title: the quiet detail beside the name.
    description: Option<String>,
    example: Option<String>,
    /// A citation style, written by the formatter, rather than a key syntax.
    style: bool,
    /// It writes reference entries, never in-text citations.
    entries: bool,
}

/// Every output the picker offers, in the engine's order.
pub(crate) fn choices() -> Vec<OutputChoice> {
    outputs::outputs()
        .iter()
        .map(|output| OutputChoice {
            id: output.id.clone(),
            name: output.name.clone(),
            description: output.description.clone(),
            example: output.example.clone(),
            style: matches!(output.output, OutputSpec::Csl { .. }),
            entries: output.entries,
        })
        .collect()
}

/// The output a listed id names, for tests that cite with it.
#[cfg(test)]
pub(crate) fn spec(id: &str) -> &'static OutputSpec {
    listed(id).unwrap_or_else(|| panic!("{id} is listed")).spec()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_stored_id_this_ibis_does_not_list_fails_closed_to_the_default() {
        for unknown in [None, Some(""), Some("no-such-output"), Some("Pandoc"), Some("style:apa")] {
            assert_eq!(stored(unknown).id(), outputs::DEFAULT, "{unknown:?}");
        }
        assert_eq!(stored(Some("typst")).id(), "typst");
        assert_eq!(stored(Some("ieee")).id(), "ieee");
    }

    #[test]
    fn only_a_listed_id_crosses_the_boundary() {
        let typst: OutputRef = serde_json::from_str(r#""typst""#).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(serde_json::to_string(&typst).unwrap_or_default(), r#""typst""#);
        for refused in [r#""no-such-output""#, r#"{"key":"typst"}"#, "7"] {
            assert!(serde_json::from_str::<OutputRef>(refused).is_err(), "{refused}");
        }
    }

    #[test]
    fn a_formatted_app_uses_the_default_style_or_the_engines() {
        assert_eq!(default_style(stored(Some("ieee"))), "ieee");
        assert_eq!(default_style(stored(Some("typst"))), outputs::DEFAULT_STYLE);
    }

    #[test]
    fn the_web_view_offers_exactly_the_engines_list() {
        let ids: Vec<_> = choices().into_iter().map(|choice| choice.id).collect();
        let listed: Vec<_> = outputs::outputs().iter().map(|output| output.id.clone()).collect();
        assert_eq!(ids, listed);
    }
}
