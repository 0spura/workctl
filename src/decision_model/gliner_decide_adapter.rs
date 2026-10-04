use std::{io::Read, time::Duration};

use serde::{Deserialize, Serialize};

use crate::domain::{AppError, LabelSuggestion};

use super::{DecisionInput, ensure_scores};

const DEFAULT_URL: &str = "http://127.0.0.1:8765";
const MAX_BODY: usize = 2 * 1024 * 1024;
const MAX_RESPONSE: usize = 1024 * 1024;

#[derive(Serialize)]
struct Request<'a> {
    title: &'a str,
    description: &'a str,
    labels: &'a [crate::domain::RepositoryLabel],
}

#[derive(Deserialize)]
struct Response {
    suggestions: Vec<LabelSuggestion>,
}

pub struct GLiNERDecideAdapter;

impl GLiNERDecideAdapter {
    pub fn suggest(input: DecisionInput<'_>) -> Result<Vec<LabelSuggestion>, AppError> {
        let url = local_url("/v1/labels", DEFAULT_URL)?;
        let body = serde_json::to_vec(&Request {
            title: input.title,
            description: input.description,
            labels: input.labels,
        })
        .map_err(|_| AppError::decision_model())?;
        if body.len() > MAX_BODY {
            return Err(AppError::decision_input_limit());
        }
        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(30))
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|_| AppError::decision_model())?;
        let response = client
            .post(url)
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .body(body)
            .send()
            .map_err(|_| AppError::decision_model())?;
        if !response.status().is_success() {
            return Err(AppError::decision_model());
        }
        let mut bytes = Vec::new();
        response
            .take((MAX_RESPONSE + 1) as u64)
            .read_to_end(&mut bytes)
            .map_err(|_| AppError::decision_model())?;
        if bytes.len() > MAX_RESPONSE {
            return Err(AppError::decision_response());
        }
        let parsed: Response =
            serde_json::from_slice(&bytes).map_err(|_| AppError::decision_response())?;
        ensure_scores(parsed.suggestions, input.labels)
    }
}

pub(super) fn local_url(path: &str, default: &str) -> Result<reqwest::Url, AppError> {
    let raw = std::env::var("DECISION_MODEL_BASE_URL").unwrap_or_else(|_| default.to_owned());
    local_url_from(&raw, path)
}

fn local_url_from(raw: &str, path: &str) -> Result<reqwest::Url, AppError> {
    let mut url = reqwest::Url::parse(raw).map_err(|_| AppError::decision_config())?;
    if url.scheme() != "http" && url.scheme() != "https" {
        return Err(AppError::decision_config());
    }
    let host = url.host_str().ok_or_else(AppError::decision_config)?;
    if !(host.eq_ignore_ascii_case("localhost")
        || host
            .parse::<std::net::IpAddr>()
            .is_ok_and(|ip| ip.is_loopback()))
    {
        return Err(AppError::decision_config());
    }
    let base = url.path().trim_end_matches('/');
    url.set_path(&format!("{base}{path}"));
    url.set_query(None);
    url.set_fragment(None);
    Ok(url)
}

#[cfg(test)]
mod tests {
    use super::local_url_from;

    #[test]
    fn local_endpoint_requires_loopback_and_preserves_api_prefix() {
        let url = local_url_from("http://127.0.0.1:11434/v1/", "/chat/completions")
            .expect("loopback endpoint");
        assert_eq!(url.as_str(), "http://127.0.0.1:11434/v1/chat/completions");
        assert_eq!(
            local_url_from("http://192.0.2.10:11434/v1", "/chat/completions")
                .expect_err("remote endpoint")
                .code,
            "decision_config"
        );
        assert_eq!(
            local_url_from("https://example.test", "/chat/completions")
                .expect_err("hosted endpoint")
                .code,
            "decision_config"
        );
    }
}
