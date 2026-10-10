mod glide_adapter;
mod jev_adapter;
mod laya_adapter;
mod llm_decision_adapter;
mod system_one;

use crate::config::DecisionModelSettings;
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
    base_url: Option<String>,
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
    base_url: Option<&'a str>,
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
            config.base_url,
            input,
        )
    }
}

impl DecisionModel {
    /// Resolves the model for an opted-in automatic operation. Nonblank environment variables
    /// (`DECISION_MODEL`, `DECISION_MODEL_API_KEY`, `DECISION_MODEL_BASE_URL`) override the
    /// matching configured value key by key.
    pub fn from_settings(settings: &DecisionModelSettings) -> Result<Self, AppError> {
        Self::resolve(
            nonblank_env("DECISION_MODEL"),
            nonblank_env("DECISION_MODEL_API_KEY"),
            nonblank_env("DECISION_MODEL_BASE_URL"),
            settings,
        )
    }

    /// Builds the model from already-resolved overrides, keeping environment lookup separate from
    /// the precedence rules so both are directly testable.
    fn resolve(
        model_override: Option<String>,
        api_key_override: Option<String>,
        base_url_override: Option<String>,
        settings: &DecisionModelSettings,
    ) -> Result<Self, AppError> {
        let selected = model_override
            .or_else(|| nonblank(settings.model.as_deref()).map(str::to_owned))
            .ok_or_else(AppError::decision_config)?;
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
                api_key_override
                    .or_else(|| nonblank(settings.api_key.as_deref()).map(str::to_owned))
                    .ok_or_else(AppError::decision_authentication)?,
            )
        } else {
            None
        };
        let base_url =
            base_url_override.or_else(|| nonblank(settings.base_url.as_deref()).map(str::to_owned));
        Ok(Self {
            adapter,
            api_key,
            model_name,
            base_url,
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
        if input.candidates.iter().any(|candidate| {
            candidate.name.trim().is_empty() || !names.insert(candidate.name.as_str())
        }) {
            return Err(AppError::invalid_input(
                "decision candidates must have distinct nonblank names",
            ));
        }
        let config = AdapterConfig {
            api_key: self.api_key.as_deref(),
            model_name: self.model_name.as_deref(),
            base_url: self.base_url.as_deref(),
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

/// A nonblank environment override; blank variables are treated as unset.
fn nonblank_env(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .filter(|value| !value.trim().is_empty())
}

fn nonblank(value: Option<&str>) -> Option<&str> {
    value.filter(|value| !value.trim().is_empty())
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
    use super::{Adapter, DecisionModel, ensure_scores};
    use crate::config::DecisionModelSettings;
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

    #[test]
    fn configured_settings_select_the_local_adapter_and_base_url() {
        let settings = DecisionModelSettings {
            model: Some("local/test-model".to_owned()),
            api_key: None,
            base_url: Some("http://127.0.0.1:11434/v1".to_owned()),
        };

        let model = DecisionModel::resolve(None, None, None, &settings).expect("configured model");
        assert_eq!(model.adapter, Adapter::Llm);
        assert_eq!(model.model_name.as_deref(), Some("test-model"));
        assert_eq!(model.base_url.as_deref(), Some("http://127.0.0.1:11434/v1"));
        assert!(model.api_key.is_none());
    }

    #[test]
    fn environment_overrides_configured_decision_model_per_key() {
        let settings = DecisionModelSettings {
            model: Some("laya".to_owned()),
            api_key: Some("configured-key".to_owned()),
            base_url: Some("http://127.0.0.1:11434/v1".to_owned()),
        };

        let overridden = DecisionModel::resolve(
            Some("local/env-model".to_owned()),
            Some("environment-key".to_owned()),
            None,
            &settings,
        )
        .expect("environment override");
        assert_eq!(overridden.adapter, Adapter::Llm);
        assert_eq!(overridden.model_name.as_deref(), Some("env-model"));
        // A local adapter never receives hosted credentials, even when both sources configure one.
        assert!(overridden.api_key.is_none());
        assert_eq!(
            overridden.base_url.as_deref(),
            Some("http://127.0.0.1:11434/v1")
        );

        let configured =
            DecisionModel::resolve(None, None, None, &settings).expect("configured key");
        assert_eq!(configured.adapter, Adapter::Laya);
        assert_eq!(configured.api_key.as_deref(), Some("configured-key"));
    }

    #[test]
    fn missing_selection_and_missing_credentials_fail_closed() {
        let hosted = DecisionModelSettings {
            model: Some("laya".to_owned()),
            ..DecisionModelSettings::default()
        };
        assert_eq!(
            DecisionModel::resolve(None, None, None, &hosted)
                .err()
                .expect("missing hosted key")
                .code,
            "decision_authentication"
        );

        for settings in [
            DecisionModelSettings::default(),
            DecisionModelSettings {
                model: Some("  ".to_owned()),
                ..DecisionModelSettings::default()
            },
            DecisionModelSettings {
                model: Some("unsupported/model".to_owned()),
                ..DecisionModelSettings::default()
            },
            DecisionModelSettings {
                model: Some("local/".to_owned()),
                ..DecisionModelSettings::default()
            },
        ] {
            assert_eq!(
                DecisionModel::resolve(None, None, None, &settings)
                    .err()
                    .expect("invalid selection")
                    .code,
                "decision_config"
            );
        }
    }
}
