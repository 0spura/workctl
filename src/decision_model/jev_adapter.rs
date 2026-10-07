use super::{
    DecisionInput,
    system_one::{self, Auth},
};
use crate::domain::{AppError, DecisionScore};

const ENDPOINT: &str = "https://thejevai.com/v1/systemone";

pub struct JevAdapter;

impl JevAdapter {
    pub fn suggest(
        api_key: &str,
        input: DecisionInput<'_>,
    ) -> Result<Vec<DecisionScore>, AppError> {
        system_one::suggest(
            ENDPOINT,
            Some("jev-latest"),
            Auth::Bearer,
            api_key,
            input,
            1_000,
        )
    }
}
