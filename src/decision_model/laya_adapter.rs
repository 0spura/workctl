use super::{
    DecisionInput,
    system_one::{self, Auth},
};
use crate::domain::{AppError, LabelSuggestion};

const ENDPOINT: &str = "https://api.laya.studio/v1/systemone";

pub struct LayaAdapter;

impl LayaAdapter {
    pub fn suggest(
        api_key: &str,
        input: DecisionInput<'_>,
    ) -> Result<Vec<LabelSuggestion>, AppError> {
        system_one::suggest(ENDPOINT, None, Auth::Bearer, api_key, input, 32)
    }
}
