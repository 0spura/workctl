use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize)]
pub struct RepositoryLabel {
    pub name: String,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DecisionCandidate {
    pub name: String,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecisionScore {
    #[serde(rename = "label")]
    pub candidate: String,
    pub probability: f64,
}
