use super::{
    DecisionInput,
    system_one::{self, Auth},
};
use crate::domain::{AppError, LabelSuggestion};

const ENDPOINT: &str = "https://api.fastino.ai/v1/systemone";

pub struct GLiDeRAdapter;

impl GLiDeRAdapter {
    pub fn suggest(
        api_key: &str,
        input: DecisionInput<'_>,
    ) -> Result<Vec<LabelSuggestion>, AppError> {
        system_one::suggest(
            ENDPOINT,
            Some("fastino/GLiDE"),
            Auth::ApiKey,
            api_key,
            input,
            255,
        )
    }
}
