//! Text for a complete, ordered document. No processor state escapes a request.
use ibis_core::contract::CitationItem;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize)]
pub struct DocumentRequest {
    pub style: String,
    pub locale: Option<String>,
    pub clusters: Vec<DocumentCluster>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DocumentCluster {
    /// Normal citations, including locators and author suppression. Other forms
    /// and affixes are refused in this text-only slice.
    pub items: Vec<CitationItem>,
    /// Zero for in-text citations; otherwise the actual note position, including
    /// notes without citations. Clusters are supplied in document order.
    pub note_index: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DocumentResult {
    /// Final text in input order, after all disambiguation and numbering updates.
    pub clusters: Vec<String>,
    /// Cited works once each, in style order, separated by newlines.
    pub bibliography: String,
}
