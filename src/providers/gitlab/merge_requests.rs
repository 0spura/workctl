use serde_json::Value;

use crate::domain::{AppError, PullRequest, PullRequestState, PullRequestSummary};

pub struct GitLabMergeRequests {
    repo_url: String,
    host: String,
}

impl GitLabMergeRequests {
    pub fn new(repo_url: String) -> Result<Self, AppError> {
        let url = reqwest::Url::parse(&repo_url)
            .map_err(|_| AppError::invalid_input("GitLab repository URL is invalid"))?;
        let host = url
            .host_str()
            .ok_or_else(|| AppError::invalid_input("GitLab repository URL is invalid"))?;
        let host = match url.port() {
            Some(port) => format!("{host}:{port}"),
            None => host.to_owned(),
        };
        Ok(Self { repo_url, host })
    }

    pub fn authenticate(&self) -> Result<(), AppError> {
        super::authenticate(&self.host)
    }

    pub fn create(
        &self,
        options: &[String],
        description: Option<&str>,
    ) -> Result<PullRequest, AppError> {
        let mut args = vec![
            "mr".to_owned(),
            "create".to_owned(),
            "--repo".to_owned(),
            self.repo_url.clone(),
        ];
        args.extend_from_slice(options);
        let output = super::run_glab_mutation(&args, None)?;
        let number = self.created_number(&output)?;
        if let Some(description) = description {
            self.set_description(number, description)?;
        }
        self.view(number)
            .map_err(|_| AppError::gitlab_write_uncertain())
    }
    pub fn update(
        &self,
        number: u64,
        options: &[String],
        description: Option<&str>,
    ) -> Result<PullRequest, AppError> {
        if !options.is_empty() {
            let mut args = vec![
                "mr".to_owned(),
                "update".to_owned(),
                number.to_string(),
                "--repo".to_owned(),
                self.repo_url.clone(),
            ];
            args.extend_from_slice(options);
            super::run_glab_mutation(&args, None)?;
        }
        if let Some(description) = description {
            self.set_description(number, description)?;
        }
        self.view(number)
            .map_err(|_| AppError::gitlab_write_uncertain())
    }

    pub fn list(&self, args: &[String]) -> Result<Vec<PullRequestSummary>, AppError> {
        let mut command = vec![
            "mr".to_owned(),
            "list".to_owned(),
            "--output".to_owned(),
            "json".to_owned(),
            "--repo".to_owned(),
            self.repo_url.clone(),
        ];
        command.extend_from_slice(args);
        let output = super::run_glab(&command)?;
        let values: Vec<Value> =
            serde_json::from_slice(&output).map_err(|_| AppError::provider_response())?;
        values.iter().map(map_summary).collect()
    }

    pub fn view(&self, number: u64) -> Result<PullRequest, AppError> {
        let args = ["mr", "view", "--output", "json", "--repo"]
            .into_iter()
            .map(str::to_owned)
            .chain([self.repo_url.clone(), number.to_string()])
            .collect::<Vec<_>>();
        let output = super::run_glab(&args)?;
        let value: Value =
            serde_json::from_slice(&output).map_err(|_| AppError::provider_response())?;
        map_view(&value)
    }
    pub fn diff(&self, number: u64, raw: bool) -> Result<String, AppError> {
        let mut args = vec![
            "mr".to_owned(),
            "diff".to_owned(),
            number.to_string(),
            "--repo".to_owned(),
            self.repo_url.clone(),
            "--color=never".to_owned(),
        ];
        if raw {
            args.push("--raw".to_owned());
        }
        String::from_utf8(super::run_glab(&args)?).map_err(|_| AppError::provider_response())
    }
    pub fn checkout(&self, number: u64) -> Result<(), AppError> {
        let args = [
            "mr".to_owned(),
            "checkout".to_owned(),
            number.to_string(),
            "--repo".to_owned(),
            self.repo_url.clone(),
        ];
        super::run_glab(&args)?;
        Ok(())
    }
    pub fn set_state(&self, number: u64, closed: bool) -> Result<(), AppError> {
        let operation = if closed { "close" } else { "reopen" };
        let args = [
            "mr".to_owned(),
            operation.to_owned(),
            number.to_string(),
            "--repo".to_owned(),
            self.repo_url.clone(),
        ];
        super::run_glab_mutation(&args, None)?;
        Ok(())
    }

    pub fn action(&self, operation: &str, number: u64) -> Result<(), AppError> {
        if !matches!(
            operation,
            "approve" | "revoke" | "rebase" | "subscribe" | "unsubscribe" | "todo"
        ) {
            return Err(AppError::invalid_input(
                "unsupported GitLab merge request action",
            ));
        }
        let args = [
            "mr".to_owned(),
            operation.to_owned(),
            number.to_string(),
            "--repo".to_owned(),
            self.repo_url.clone(),
        ];
        super::run_glab_mutation(&args, None)?;
        Ok(())
    }

    pub fn related_data(&self, operation: &str, number: u64) -> Result<Value, AppError> {
        if !matches!(operation, "approvers" | "issues") {
            return Err(AppError::invalid_input(
                "unsupported GitLab merge request query",
            ));
        }
        let args = [
            "mr".to_owned(),
            operation.to_owned(),
            number.to_string(),
            "--repo".to_owned(),
            self.repo_url.clone(),
            "--output".to_owned(),
            "json".to_owned(),
        ];
        let output = super::run_glab(&args)?;
        serde_json::from_slice(&output).map_err(|_| AppError::provider_response())
    }

    pub fn list_notes(&self, number: u64, filters: &[String]) -> Result<Value, AppError> {
        let mut args = vec![
            "mr".to_owned(),
            "note".to_owned(),
            "list".to_owned(),
            number.to_string(),
            "--repo".to_owned(),
            self.repo_url.clone(),
            "--output".to_owned(),
            "json".to_owned(),
        ];
        args.extend_from_slice(filters);
        let output = super::run_glab(&args)?;
        serde_json::from_slice(&output).map_err(|_| AppError::provider_response())
    }

    pub fn update_note(&self, number: u64, note_id: u64, body: &str) -> Result<(), AppError> {
        let args = [
            "mr".to_owned(),
            "note".to_owned(),
            "update".to_owned(),
            number.to_string(),
            note_id.to_string(),
            "--repo".to_owned(),
            self.repo_url.clone(),
        ];
        super::run_glab_mutation(&args, Some(body.as_bytes().to_vec()))?;
        Ok(())
    }

    pub fn merge(&self, options: &[String], number: u64) -> Result<(), AppError> {
        let mut args = vec![
            "mr".to_owned(),
            "merge".to_owned(),
            number.to_string(),
            "--repo".to_owned(),
            self.repo_url.clone(),
        ];
        args.extend_from_slice(options);
        super::run_glab_mutation(&args, None)?;
        Ok(())
    }

    pub fn create_note(
        &self,
        number: u64,
        message: &str,
        options: &[String],
    ) -> Result<(), AppError> {
        let mut args = vec![
            "mr".to_owned(),
            "note".to_owned(),
            "create".to_owned(),
            number.to_string(),
            "--repo".to_owned(),
            self.repo_url.clone(),
        ];
        args.extend_from_slice(options);
        super::run_glab_mutation(&args, Some(message.as_bytes().to_vec()))?;
        Ok(())
    }

    pub fn note_discussion_action(
        &self,
        operation: &str,
        discussion_id: &str,
        number: u64,
    ) -> Result<(), AppError> {
        if !matches!(operation, "resolve" | "reopen") {
            return Err(AppError::invalid_input(
                "unsupported GitLab merge request discussion action",
            ));
        }
        let args = [
            "mr".to_owned(),
            "note".to_owned(),
            operation.to_owned(),
            discussion_id.to_owned(),
            number.to_string(),
            "--repo".to_owned(),
            self.repo_url.clone(),
        ];
        super::run_glab_mutation(&args, None)?;
        Ok(())
    }
    fn created_number(&self, output: &[u8]) -> Result<u64, AppError> {
        let output = std::str::from_utf8(output).map_err(|_| AppError::gitlab_write_uncertain())?;
        let prefix = format!("{}/-/merge_requests/", self.repo_url.trim_end_matches('/'));
        output
            .trim()
            .strip_prefix(&prefix)
            .filter(|suffix| !suffix.is_empty() && suffix.bytes().all(|byte| byte.is_ascii_digit()))
            .and_then(|suffix| suffix.parse::<u64>().ok())
            .filter(|number| *number > 0)
            .ok_or_else(AppError::gitlab_write_uncertain)
    }

    fn set_description(&self, number: u64, description: &str) -> Result<(), AppError> {
        let project = self.project_api_path()?;
        let args = [
            "api".to_owned(),
            "--method".to_owned(),
            "PUT".to_owned(),
            format!("projects/{project}/merge_requests/{number}"),
            "--hostname".to_owned(),
            self.host.clone(),
            "--input".to_owned(),
            "-".to_owned(),
        ];
        let body = serde_json::to_vec(&serde_json::json!({ "description": description }))
            .map_err(|_| AppError::provider_response())?;
        super::run_glab_mutation(&args, Some(body))?;
        Ok(())
    }

    fn project_api_path(&self) -> Result<String, AppError> {
        let path = self
            .repo_url
            .strip_prefix("https://")
            .and_then(|url| url.split_once('/').map(|(_, path)| path))
            .filter(|path| !path.is_empty())
            .ok_or_else(|| AppError::invalid_input("GitLab repository URL is invalid"))?;
        Ok(path.trim_end_matches(".git").replace('/', "%2F"))
    }
}

fn map_summary(value: &Value) -> Result<PullRequestSummary, AppError> {
    let state = state(value)?;
    Ok(PullRequestSummary {
        number: number(value)?,
        title: string(value, "title")?,
        state,
        draft: boolean(value, "draft"),
        url: string(value, "web_url")?,
        base_ref: string(value, "target_branch")?,
        head_ref: string(value, "source_branch")?,
        updated_at: string(value, "updated_at")?,
    })
}

fn map_view(value: &Value) -> Result<PullRequest, AppError> {
    let state = state(value)?;
    Ok(PullRequest {
        number: number(value)?,
        title: string(value, "title")?,
        body: value
            .get("description")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned(),
        state,
        draft: boolean(value, "draft"),
        url: string(value, "web_url")?,
        base_ref: string(value, "target_branch")?,
        head_ref: string(value, "source_branch")?,
        author: value
            .pointer("/author/username")
            .and_then(Value::as_str)
            .ok_or_else(AppError::provider_response)?
            .to_owned(),
        created_at: string(value, "created_at")?,
        updated_at: string(value, "updated_at")?,
        merged_at: value
            .get("merged_at")
            .and_then(Value::as_str)
            .map(str::to_owned),
        mergeable: None,
        review_decision: None,
        labels: string_list(value.get("labels"))?,
        assignees: value
            .get("assignees")
            .and_then(Value::as_array)
            .ok_or_else(AppError::provider_response)?
            .iter()
            .map(|user| {
                user.get("username")
                    .and_then(Value::as_str)
                    .map(str::to_owned)
                    .ok_or_else(AppError::provider_response)
            })
            .collect::<Result<_, _>>()?,
    })
}

fn state(value: &Value) -> Result<PullRequestState, AppError> {
    match value.get("state").and_then(Value::as_str) {
        Some("opened") => Ok(PullRequestState::Open),
        Some("closed") => Ok(PullRequestState::Closed),
        Some("merged") => Ok(PullRequestState::Merged),
        _ => Err(AppError::provider_response()),
    }
}

fn number(value: &Value) -> Result<u64, AppError> {
    value
        .get("iid")
        .and_then(Value::as_u64)
        .filter(|number| *number > 0)
        .ok_or_else(AppError::provider_response)
}

fn string(value: &Value, key: &str) -> Result<String, AppError> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or_else(AppError::provider_response)
}

fn boolean(value: &Value, key: &str) -> bool {
    value.get(key).and_then(Value::as_bool).unwrap_or(false)
}

fn string_list(value: Option<&Value>) -> Result<Vec<String>, AppError> {
    value
        .and_then(Value::as_array)
        .ok_or_else(AppError::provider_response)?
        .iter()
        .map(|item| {
            item.as_str()
                .map(str::to_owned)
                .ok_or_else(AppError::provider_response)
        })
        .collect()
}
