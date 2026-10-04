mod glide_adapter;
mod gliner_decide_adapter;
mod jev_adapter;
mod laya_adapter;
mod llm_decision_adapter;
mod system_one;

use crate::domain::{AppError, LabelSuggestion, RepositoryLabel};

const MAX_LABELS: usize = 1_000;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Adapter {
    Jev,
    Laya,
    GlinerDecide,
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
    pub labels: &'a [RepositoryLabel],
}

struct AdapterConfig<'a> {
    api_key: Option<&'a str>,
    model_name: Option<&'a str>,
}

trait DecisionAdapter {
    fn suggest(
        config: AdapterConfig<'_>,
        input: DecisionInput<'_>,
    ) -> Result<Vec<LabelSuggestion>, AppError>;
}

impl DecisionAdapter for jev_adapter::JevAdapter {
    fn suggest(
        config: AdapterConfig<'_>,
        input: DecisionInput<'_>,
    ) -> Result<Vec<LabelSuggestion>, AppError> {
        jev_adapter::JevAdapter::suggest(api_key_required(config.api_key)?, input)
    }
}

impl DecisionAdapter for laya_adapter::LayaAdapter {
    fn suggest(
        config: AdapterConfig<'_>,
        input: DecisionInput<'_>,
    ) -> Result<Vec<LabelSuggestion>, AppError> {
        laya_adapter::LayaAdapter::suggest(api_key_required(config.api_key)?, input)
    }
}

impl DecisionAdapter for gliner_decide_adapter::GLiNERDecideAdapter {
    fn suggest(
        _config: AdapterConfig<'_>,
        input: DecisionInput<'_>,
    ) -> Result<Vec<LabelSuggestion>, AppError> {
        gliner_decide_adapter::GLiNERDecideAdapter::suggest(input)
    }
}

impl DecisionAdapter for glide_adapter::GLiDeRAdapter {
    fn suggest(
        config: AdapterConfig<'_>,
        input: DecisionInput<'_>,
    ) -> Result<Vec<LabelSuggestion>, AppError> {
        glide_adapter::GLiDeRAdapter::suggest(api_key_required(config.api_key)?, input)
    }
}

impl DecisionAdapter for llm_decision_adapter::LLMDecisionAdapter {
    fn suggest(
        config: AdapterConfig<'_>,
        input: DecisionInput<'_>,
    ) -> Result<Vec<LabelSuggestion>, AppError> {
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
            "fastino/GLiNER2.5-Decide" => (Adapter::GlinerDecide, false, None),
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

    pub fn suggest(&self, input: DecisionInput<'_>) -> Result<Vec<LabelSuggestion>, AppError> {
        if input.labels.is_empty() {
            return Ok(Vec::new());
        }
        if input.labels.len() > MAX_LABELS {
            return Err(AppError::decision_input_limit());
        }
        let config = AdapterConfig {
            api_key: self.api_key.as_deref(),
            model_name: self.model_name.as_deref(),
        };
        match self.adapter {
            Adapter::Jev => <jev_adapter::JevAdapter as DecisionAdapter>::suggest(config, input),
            Adapter::Laya => <laya_adapter::LayaAdapter as DecisionAdapter>::suggest(config, input),
            Adapter::GlinerDecide => {
                <gliner_decide_adapter::GLiNERDecideAdapter as DecisionAdapter>::suggest(
                    config, input,
                )
            }
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
    scores: Vec<LabelSuggestion>,
    labels: &[RepositoryLabel],
) -> Result<Vec<LabelSuggestion>, AppError> {
    if scores.len() != labels.len() {
        return Err(AppError::decision_response());
    }
    let mut seen = std::collections::HashSet::with_capacity(scores.len());
    for score in &scores {
        if !labels.iter().any(|label| label.name == score.label)
            || !seen.insert(score.label.as_str())
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
    use crate::domain::{LabelSuggestion, RepositoryLabel};

    fn labels() -> Vec<RepositoryLabel> {
        vec![
            RepositoryLabel {
                name: "bug".to_owned(),
                description: None,
            },
            RepositoryLabel {
                name: "docs".to_owned(),
                description: None,
            },
        ]
    }

    #[test]
    fn score_contract_requires_one_unique_finite_score_per_candidate() {
        let labels = labels();
        let valid = vec![
            LabelSuggestion {
                label: "bug".to_owned(),
                probability: 0.8,
            },
            LabelSuggestion {
                label: "docs".to_owned(),
                probability: 0.0,
            },
        ];
        assert_eq!(
            ensure_scores(valid, &labels).expect("valid scores").len(),
            2
        );

        for invalid in [
            vec![LabelSuggestion {
                label: "bug".to_owned(),
                probability: 0.8,
            }],
            vec![
                LabelSuggestion {
                    label: "bug".to_owned(),
                    probability: 0.8,
                },
                LabelSuggestion {
                    label: "bug".to_owned(),
                    probability: 0.7,
                },
            ],
            vec![
                LabelSuggestion {
                    label: "bug".to_owned(),
                    probability: 0.8,
                },
                LabelSuggestion {
                    label: "unknown".to_owned(),
                    probability: 0.7,
                },
            ],
            vec![
                LabelSuggestion {
                    label: "bug".to_owned(),
                    probability: f64::NAN,
                },
                LabelSuggestion {
                    label: "docs".to_owned(),
                    probability: 0.7,
                },
            ],
        ] {
            assert_eq!(
                ensure_scores(invalid, &labels)
                    .expect_err("invalid scores")
                    .code,
                "decision_response"
            );
        }
    }
}
