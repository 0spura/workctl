use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize)]
pub struct RepositoryLabel {
    pub name: String,
    pub description: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct LabelSuggestion {
    pub label: String,
    pub probability: f64,
}
