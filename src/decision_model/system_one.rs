use std::{collections::BTreeMap, io::Read, time::Duration};

use serde::{Deserialize, Serialize};

use crate::domain::{AppError, LabelSuggestion, RepositoryLabel};

use super::{DecisionInput, ensure_scores};

const MAX_REQUEST_BYTES: usize = 2 * 1024 * 1024;
const MAX_RESPONSE_BYTES: usize = 1024 * 1024;
const REMOTE_TIMEOUT: Duration = Duration::from_secs(300);

#[derive(Clone, Copy)]
pub enum Auth {
    Bearer,
    ApiKey,
}

#[derive(Serialize)]
struct Request<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    model: Option<&'static str>,
    state: State<'a>,
    questions: BTreeMap<String, Question>,
}

#[derive(Serialize)]
struct State<'a> {
    work_item: WorkItem<'a>,
    labels: &'a [RepositoryLabel],
}

#[derive(Serialize)]
struct WorkItem<'a> {
    title: &'a str,
    description: &'a str,
}

#[derive(Serialize)]
struct Question {
    #[serde(rename = "type")]
    kind: &'static str,
    instructions: String,
    criteria: BTreeMap<&'static str, &'static str>,
}

#[derive(Deserialize)]
struct Response {
    answers: BTreeMap<String, Answer>,
}

#[derive(Deserialize)]
struct Answer {
    #[serde(rename = "type")]
    kind: String,
    noul: f64,
}

pub fn suggest(
    endpoint: &str,
    model: Option<&'static str>,
    auth: Auth,
    api_key: &str,
    input: DecisionInput<'_>,
    max_questions: usize,
) -> Result<Vec<LabelSuggestion>, AppError> {
    if input.labels.len() > max_questions {
        return Err(AppError::decision_input_limit());
    }
    let questions = input.labels.iter().enumerate().map(|(index, label)| {
        let mut criteria = BTreeMap::new();
        criteria.insert("true", "The code work item clearly fits this existing label.");
        criteria.insert("false", "The code work item does not clearly fit this existing label.");
        (format!("label_{index}"), Question {
            kind: "noul",
            instructions: format!("Should this code work item receive the existing repository label {:?}? Use its description as the intended meaning. Recommend only a clear semantic match.", label.name),
            criteria,
        })
    }).collect();
    let request = Request {
        model,
        state: State {
            work_item: WorkItem {
                title: input.title,
                description: input.description,
            },
            labels: input.labels,
        },
        questions,
    };
    let body = serde_json::to_vec(&request).map_err(|_| AppError::decision_model())?;
    if body.len() > MAX_REQUEST_BYTES {
        return Err(AppError::decision_input_limit());
    }
    let client = reqwest::blocking::Client::builder()
        .timeout(REMOTE_TIMEOUT)
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|_| AppError::decision_model())?;
    let mut builder = client
        .post(endpoint)
        .header(reqwest::header::CONTENT_TYPE, "application/json");
    builder = match auth {
        Auth::Bearer => builder.bearer_auth(api_key),
        Auth::ApiKey => builder.header("X-API-Key", api_key),
    };
    let response = builder
        .body(body)
        .send()
        .map_err(|_| AppError::decision_model())?;
    if response.status() == reqwest::StatusCode::UNAUTHORIZED
        || response.status() == reqwest::StatusCode::FORBIDDEN
    {
        return Err(AppError::decision_authentication());
    }
    if !response.status().is_success() {
        return Err(AppError::decision_model());
    }
    let mut bytes = Vec::new();
    response
        .take((MAX_RESPONSE_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|_| AppError::decision_model())?;
    if bytes.len() > MAX_RESPONSE_BYTES {
        return Err(AppError::decision_response());
    }
    let response: Response =
        serde_json::from_slice(&bytes).map_err(|_| AppError::decision_response())?;
    let mut scores = Vec::with_capacity(input.labels.len());
    for (index, label) in input.labels.iter().enumerate() {
        let answer = response
            .answers
            .get(&format!("label_{index}"))
            .ok_or_else(AppError::decision_response)?;
        if answer.kind != "noul" {
            return Err(AppError::decision_response());
        }
        scores.push(LabelSuggestion {
            label: label.name.clone(),
            probability: answer.noul,
        });
    }
    ensure_scores(scores, input.labels)
}

#[cfg(test)]
mod tests {
    use std::{
        io::{BufRead, Read, Write},
        net::TcpListener,
        thread,
    };

    use super::{Auth, suggest};
    use crate::decision_model::DecisionInput;
    use crate::domain::RepositoryLabel;

    #[test]
    fn native_request_uses_selected_auth_and_validates_noul_scores() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind decision fixture");
        let address = listener.local_addr().expect("fixture address");
        let server = thread::spawn(move || {
            let (stream, _) = listener.accept().expect("accept request");
            let mut reader = std::io::BufReader::new(stream);
            let mut line = String::new();
            let mut length = 0;
            let mut has_key = false;
            loop {
                line.clear();
                reader.read_line(&mut line).expect("read header");
                if line == "\r\n" {
                    break;
                }
                let lower = line.to_ascii_lowercase();
                if lower.trim() == "x-api-key: fixture-key" {
                    has_key = true;
                }
                if let Some(value) = lower.strip_prefix("content-length:") {
                    length = value.trim().parse::<usize>().expect("content length");
                }
            }
            let mut request = vec![0; length];
            reader.read_exact(&mut request).expect("read request body");
            let request: serde_json::Value =
                serde_json::from_slice(&request).expect("request JSON");
            assert_eq!(request["model"], "fastino/GLiDE");
            assert_eq!(request["state"]["work_item"]["title"], "Build fails");
            assert_eq!(request["questions"]["label_0"]["type"], "noul");
            assert!(has_key);
            let response = r#"{"answers":{"label_0":{"type":"noul","noul":0.8}}}"#;
            write!(
                reader.into_inner(),
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                response.len(),
                response
            ).expect("write response");
        });
        let labels = [RepositoryLabel {
            name: "bug".to_owned(),
            description: Some("Build failures".to_owned()),
        }];
        let scores = suggest(
            &format!("http://{address}/v1/systemone"),
            Some("fastino/GLiDE"),
            Auth::ApiKey,
            "fixture-key",
            DecisionInput {
                title: "Build fails",
                description: "CI reports a compile error",
                labels: &labels,
            },
            255,
        )
        .expect("validated response");
        server.join().expect("fixture server");
        assert_eq!(scores[0].label, "bug");
        assert_eq!(scores[0].probability, 0.8);
    }
}
