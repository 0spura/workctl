mod glide_adapter;
mod jev_adapter;
mod laya_adapter;
mod llm_decision_adapter;
mod system_one;

use crate::domain::{AppError, DecisionCandidate, DecisionScore};

const MAX_CANDIDATES: usize = 1_000;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Adapter {
    Jev,
    Laya,
    Glide,
    Llm,
}

pub struct DecisionModel {
    adapter: Adapter,
    api_key: Option<String>,
    model_name: Option<String>,
}
#[derive(Clone, Debug)]
pub struct DecisionInput<'a> {
    pub title: &'a str,
    pub description: &'a str,
    pub candidates: &'a [DecisionCandidate],
}

struct AdapterConfig<'a> {
    api_key: Option<&'a str>,
    model_name: Option<&'a str>,
}

trait DecisionAdapter {
    fn suggest(
        config: AdapterConfig<'_>,
        input: DecisionInput<'_>,
    ) -> Result<Vec<DecisionScore>, AppError>;
}

impl DecisionAdapter for jev_adapter::JevAdapter {
    fn suggest(
        config: AdapterConfig<'_>,
        input: DecisionInput<'_>,
    ) -> Result<Vec<DecisionScore>, AppError> {
        jev_adapter::JevAdapter::suggest(api_key_required(config.api_key)?, input)
    }
}

impl DecisionAdapter for laya_adapter::LayaAdapter {
    fn suggest(
        config: AdapterConfig<'_>,
        input: DecisionInput<'_>,
    ) -> Result<Vec<DecisionScore>, AppError> {
        laya_adapter::LayaAdapter::suggest(api_key_required(config.api_key)?, input)
    }
}


impl DecisionAdapter for glide_adapter::GLiDeRAdapter {
    fn suggest(
        config: AdapterConfig<'_>,
        input: DecisionInput<'_>,
    ) -> Result<Vec<DecisionScore>, AppError> {
        glide_adapter::GLiDeRAdapter::suggest(api_key_required(config.api_key)?, input)
    }
}

impl DecisionAdapter for llm_decision_adapter::LLMDecisionAdapter {
    fn suggest(
        config: AdapterConfig<'_>,
        input: DecisionInput<'_>,
    ) -> Result<Vec<DecisionScore>, AppError> {
        llm_decision_adapter::LLMDecisionAdapter::suggest(
            config.model_name.ok_or_else(AppError::decision_config)?,
            input,
        )
    }
}

impl DecisionModel {
    pub fn from_environment() -> Result<Self, AppError> {
        let selected = std::env::var("DECISION_MODEL").map_err(|_| AppError::decision_config())?;
        let (adapter, needs_key, model_name) = match selected.as_str() {
            "jev-latest" => (Adapter::Jev, true, None),
            "laya" => (Adapter::Laya, true, None),
            "fastino/GLiDE" => (Adapter::Glide, true, None),
            value if value.starts_with("local/") && value.len() > "local/".len() => (
                Adapter::Llm,
                false,
                Some(value["local/".len()..].to_owned()),
            ),
            _ => return Err(AppError::decision_config()),
        };
        let api_key = if needs_key {
            Some(
                std::env::var("DECISION_MODEL_API_KEY")
                    .ok()
                    .filter(|key| !key.trim().is_empty())
                    .ok_or_else(AppError::decision_authentication)?,
            )
        } else {
            None
        };
        Ok(Self {
            adapter,
            api_key,
            model_name,
        })
    }

    pub fn suggest(&self, input: DecisionInput<'_>) -> Result<Vec<DecisionScore>, AppError> {
        if input.candidates.is_empty() {
            return Ok(Vec::new());
        }
        if input.candidates.len() > MAX_CANDIDATES {
            return Err(AppError::decision_input_limit());
        }
        let mut names = std::collections::HashSet::with_capacity(input.candidates.len());
        if input
            .candidates
            .iter()
            .any(|candidate| candidate.name.trim().is_empty() || !names.insert(candidate.name.as_str()))
        {
            return Err(AppError::invalid_input(
                "decision candidates must have distinct nonblank names",
            ));
        }
        let config = AdapterConfig {
            api_key: self.api_key.as_deref(),
            model_name: self.model_name.as_deref(),
        };
        match self.adapter {
            Adapter::Jev => <jev_adapter::JevAdapter as DecisionAdapter>::suggest(config, input),
            Adapter::Laya => <laya_adapter::LayaAdapter as DecisionAdapter>::suggest(config, input),
            Adapter::Glide => {
                <glide_adapter::GLiDeRAdapter as DecisionAdapter>::suggest(config, input)
            }
            Adapter::Llm => <llm_decision_adapter::LLMDecisionAdapter as DecisionAdapter>::suggest(
                config, input,
            ),
        }
    }
}

fn api_key_required(key: Option<&str>) -> Result<&str, AppError> {
    key.ok_or_else(AppError::decision_authentication)
}

fn ensure_scores(
    scores: Vec<DecisionScore>,
    candidates: &[DecisionCandidate],
) -> Result<Vec<DecisionScore>, AppError> {
    if scores.len() != candidates.len() {
        return Err(AppError::decision_response());
    }
    let mut seen = std::collections::HashSet::with_capacity(scores.len());
    for score in &scores {
        if !candidates
            .iter()
            .any(|candidate| candidate.name == score.candidate)
            || !seen.insert(score.candidate.as_str())
            || !score.probability.is_finite()
            || !(0.0..=1.0).contains(&score.probability)
        {
            return Err(AppError::decision_response());
        }
    }
    Ok(scores)
}

#[cfg(test)]
mod tests {
    use super::ensure_scores;
    use crate::domain::{DecisionCandidate, DecisionScore};

    fn candidates() -> Vec<DecisionCandidate> {
        vec![
            DecisionCandidate {
                name: "bug".to_owned(),
                description: None,
            },
            DecisionCandidate {
                name: "docs".to_owned(),
                description: None,
            },
        ]
    }

    #[test]
    fn score_contract_requires_one_unique_finite_score_per_candidate() {
        let candidates = candidates();
        let valid = vec![
            DecisionScore {
                candidate: "bug".to_owned(),
                probability: 0.8,
            },
            DecisionScore {
                candidate: "docs".to_owned(),
                probability: 0.0,
            },
        ];
        assert_eq!(
            ensure_scores(valid, &candidates)
                .expect("valid scores")
                .len(),
            2
        );

        for invalid in [
            vec![DecisionScore {
                candidate: "bug".to_owned(),
                probability: 0.8,
            }],
            vec![
                DecisionScore {
                    candidate: "bug".to_owned(),
                    probability: 0.8,
                },
                DecisionScore {
                    candidate: "bug".to_owned(),
                    probability: 0.7,
                },
            ],
            vec![
                DecisionScore {
                    candidate: "bug".to_owned(),
                    probability: 0.8,
                },
                DecisionScore {
                    candidate: "unknown".to_owned(),
                    probability: 0.7,
                },
            ],
            vec![
                DecisionScore {
                    candidate: "bug".to_owned(),
                    probability: f64::NAN,
                },
                DecisionScore {
                    candidate: "docs".to_owned(),
                    probability: 0.7,
                },
            ],
        ] {
            assert_eq!(
                ensure_scores(invalid, &candidates)
                    .expect_err("invalid scores")
                    .code,
                "decision_response"
            );
        }
    }
}
