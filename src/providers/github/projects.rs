use serde::Deserialize;

use crate::config::GithubProjectDefaults;
use crate::domain::AppError;

const PROJECT_QUERY: &str = r#"query($owner: String!, $number: Int!) {
  repositoryOwner(login: $owner) {
    ... on Organization { projectV2(number: $number) { id title fields(first: 100) { nodes {
      __typename
      ... on ProjectV2Field { id name dataType }
      ... on ProjectV2SingleSelectField { id name dataType options { id name } }
      ... on ProjectV2IterationField { id name dataType configuration { iterations { id title } } }
    } pageInfo { hasNextPage } } } }
    ... on User { projectV2(number: $number) { id title fields(first: 100) { nodes {
      __typename
      ... on ProjectV2Field { id name dataType }
      ... on ProjectV2SingleSelectField { id name dataType options { id name } }
      ... on ProjectV2IterationField { id name dataType configuration { iterations { id title } } }
    } pageInfo { hasNextPage } } } }
  }
}"#;

#[derive(Debug)]
pub(super) struct ProjectFieldAssignment {
    pub name: String,
    pub field_id: String,
    pub kind: FieldKind,
    pub input: FieldInput,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) enum FieldKind {
    Text,
    Number,
    Date,
    SingleSelect,
    Iteration,
}

#[derive(Debug)]
pub(super) enum FieldInput {
    Text(String),
    Number(String),
    Date(String),
    Id(String),
    /// Removes the current value of any supported field type.
    Clear,
}

/// One option of a Project field that the configured decision model may choose.
#[derive(Debug)]
pub(super) struct FieldOption {
    /// Decision-model candidate name; unique across the auto-selected fields.
    pub candidate: String,
    /// Option title as the Project spells it.
    pub title: String,
    id: String,
}

#[derive(Debug)]
pub(super) struct ProjectFieldChoice {
    pub name: String,
    field_id: String,
    kind: FieldKind,
    pub options: Vec<FieldOption>,
}

#[derive(Debug)]
pub(super) struct ProjectPlan {
    pub url: String,
    pub project_id: String,
    pub owner: String,
    pub number: u64,
    pub assignments: Vec<ProjectFieldAssignment>,
    pub choices: Vec<ProjectFieldChoice>,
}

impl ProjectPlan {
    /// One decision candidate per option of every auto-selected field.
    pub(super) fn option_candidates(&self) -> Vec<crate::domain::DecisionCandidate> {
        self.choices
            .iter()
            .flat_map(|choice| {
                choice
                    .options
                    .iter()
                    .map(|option| crate::domain::DecisionCandidate {
                        name: option.candidate.clone(),
                        description: Some(format!(
                            "GitHub Project field {:?} set to the option {:?}",
                            choice.name, option.title
                        )),
                    })
            })
            .collect()
    }

    /// Records the model's choice for every auto-selected field.
    ///
    /// Only scores at or above `threshold` compete. An exact tie for a field's best score fails
    /// instead of guessing which option the model meant, and a field whose best score falls below
    /// the threshold stays unset.
    pub(super) fn choose_from_scores(
        &mut self,
        scores: &[crate::domain::DecisionScore],
        threshold: f64,
    ) -> Result<(), AppError> {
        let mut chosen = Vec::with_capacity(self.choices.len());
        for choice in &self.choices {
            let mut best: Option<(&FieldOption, f64)> = None;
            let mut tied = false;
            for option in &choice.options {
                let Some(score) = scores
                    .iter()
                    .find(|score| score.candidate == option.candidate)
                else {
                    continue;
                };
                if score.probability < threshold {
                    continue;
                }
                match best {
                    Some((_, probability)) if score.probability > probability => {
                        best = Some((option, score.probability));
                        tied = false;
                    }
                    Some((_, probability)) if score.probability == probability => {
                        tied = true;
                    }
                    None => best = Some((option, score.probability)),
                    _ => {}
                }
            }
            if tied {
                return Err(AppError::invalid_input(
                    "the decision model tied on a GitHub Project field option",
                ));
            }
            if let Some((option, _)) = best {
                chosen.push(option.candidate.clone());
            }
        }
        for candidate in chosen {
            self.choose(&candidate)?;
        }
        Ok(())
    }

    /// Records the model's choice as a validated field write.
    pub(super) fn choose(&mut self, candidate: &str) -> Result<(), AppError> {
        let Some(choice) = self.choices.iter().find(|choice| choice.has_option(candidate)) else {
            return Err(AppError::provider_response());
        };
        let option = choice
            .options
            .iter()
            .find(|option| option.candidate == candidate)
            .ok_or_else(AppError::provider_response)?;
        let assignment = ProjectFieldAssignment {
            name: choice.name.clone(),
            field_id: choice.field_id.clone(),
            kind: choice.kind,
            input: FieldInput::Id(option.id.clone()),
        };
        self.assignments.push(assignment);
        Ok(())
    }
}

impl ProjectFieldChoice {
    fn has_option(&self, candidate: &str) -> bool {
        self.options
            .iter()
            .any(|option| option.candidate == candidate)
    }
}

#[derive(Deserialize)]
struct GraphResponse {
    data: Option<GraphData>,
    errors: Option<Vec<serde_json::Value>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GraphData {
    repository_owner: Option<RepositoryOwner>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RepositoryOwner {
    project_v2: Option<Project>,
}

#[derive(Deserialize)]
struct Project {
    id: String,
    title: String,
    fields: FieldConnection,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct FieldConnection {
    nodes: Vec<RawField>,
    page_info: PageInfo,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PageInfo {
    has_next_page: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawField {
    #[serde(rename = "__typename")]
    typename: String,
    id: String,
    name: String,
    data_type: String,
    #[serde(default)]
    options: Vec<NamedId>,
    configuration: Option<IterationConfig>,
}

#[derive(Deserialize)]
struct NamedId {
    id: String,
    name: String,
}

#[derive(Deserialize)]
struct IterationConfig {
    iterations: Vec<Iteration>,
}

#[derive(Deserialize)]
struct Iteration {
    id: String,
    title: String,
}

pub(super) fn prepare(
    profile: &GithubProjectDefaults,
    requested: &[(String, String)],
    expected_title: Option<&str>,
) -> Result<ProjectPlan, AppError> {
    let (owner, number) = parse_project_url(&profile.url)?;
    let selectable = profile.auto_select_fields.as_slice();
    if requested.is_empty() && selectable.is_empty() && expected_title.is_none() {
        return Ok(ProjectPlan {
            url: profile.url.clone(),
            project_id: String::new(),
            owner,
            number,
            assignments: Vec::new(),
            choices: Vec::new(),
        });
    }
    let project = discover(&owner, number)?;
    if expected_title.is_some_and(|expected| expected != project.title) {
        return Err(AppError::invalid_input(
            "--project does not match the configured Project profile",
        ));
    }
    let assignments = resolve_assignments(&project.fields, requested)?;
    let choices = resolve_choices(&project.fields, selectable, requested)?;
    Ok(ProjectPlan {
        url: profile.url.clone(),
        project_id: project.id,
        owner,
        number,
        assignments,
        choices,
    })
}

/// Discovers the configured Project and validates the fields an edit explicitly sets or clears.
///
/// An edit applies only what the caller named: `autoSelectFields` is never consulted, so a field
/// edit can neither reapply a configured value nor trigger model selection. `expected_title`,
/// when set, is the explicit `--add-project` title and must match the discovered Project.
pub(super) fn prepare_edit(
    profile: &GithubProjectDefaults,
    sets: &[(String, String)],
    clears: &[String],
    expected_title: Option<&str>,
) -> Result<ProjectPlan, AppError> {
    let (owner, number) = parse_project_url(&profile.url)?;
    let project = discover(&owner, number)?;
    if expected_title.is_some_and(|expected| expected != project.title) {
        return Err(AppError::invalid_input(
            "--add-project does not match the configured Project profile",
        ));
    }
    let mut assignments = resolve_assignments(&project.fields, sets)?;
    assignments.extend(resolve_clears(&project.fields, clears)?);
    Ok(ProjectPlan {
        url: profile.url.clone(),
        project_id: project.id,
        owner,
        number,
        assignments,
        choices: Vec::new(),
    })
}

/// The Project identity and typed fields discovered for one profile.
struct DiscoveredProject {
    id: String,
    title: String,
    fields: Vec<RawField>,
}

/// Queries the Project schema through authenticated GraphQL.
fn discover(owner: &str, number: u64) -> Result<DiscoveredProject, AppError> {
    let args = vec![
        "api".to_owned(),
        "graphql".to_owned(),
        "-f".to_owned(),
        format!("query={PROJECT_QUERY}"),
        "-F".to_owned(),
        format!("owner={owner}"),
        "-F".to_owned(),
        format!("number={number}"),
    ];
    let output = super::run_gh(&args, None)?;
    let response: GraphResponse =
        serde_json::from_slice(&output).map_err(|_| AppError::provider_response())?;
    if response
        .errors
        .as_ref()
        .is_some_and(|errors| !errors.is_empty())
    {
        return Err(AppError::github_cli());
    }
    let project = response
        .data
        .and_then(|data| data.repository_owner)
        .and_then(|owner| owner.project_v2)
        .ok_or(AppError::invalid_input(
            "the configured GitHub Project was not found",
        ))?;
    if project.fields.page_info.has_next_page {
        return Err(AppError::invalid_input(
            "the GitHub Project has more than 100 fields; field assignment is unsupported",
        ));
    }
    Ok(DiscoveredProject {
        id: project.id,
        title: project.title,
        fields: project.fields.nodes,
    })
}

/// Resolves the allowlisted fields into model-choosable options.
///
/// A field that already carries a configured or explicit value is claimed and is never offered to
/// the model, so an explicit `--project-field` always wins over automatic selection.
fn resolve_choices(
    fields: &[RawField],
    selectable: &[String],
    claimed: &[(String, String)],
) -> Result<Vec<ProjectFieldChoice>, AppError> {
    let mut choices = Vec::with_capacity(selectable.len());
    for name in selectable {
        let field = fields
            .iter()
            .find(|field| field.name == *name)
            .ok_or(AppError::invalid_input(
                "a configured automatically selected GitHub Project field does not exist",
            ))?;
        let (kind, options) = match (field.typename.as_str(), field.data_type.as_str()) {
            ("ProjectV2SingleSelectField", "SINGLE_SELECT") => (
                FieldKind::SingleSelect,
                field
                    .options
                    .iter()
                    .map(|option| FieldOption {
                        candidate: format!("{name}={}", option.name),
                        title: option.name.clone(),
                        id: option.id.clone(),
                    })
                    .collect::<Vec<_>>(),
            ),
            ("ProjectV2IterationField", "ITERATION") => (
                FieldKind::Iteration,
                field
                    .configuration
                    .as_ref()
                    .map(|config| {
                        config
                            .iterations
                            .iter()
                            .map(|iteration| FieldOption {
                                candidate: format!("{name}={}", iteration.title),
                                title: iteration.title.clone(),
                                id: iteration.id.clone(),
                            })
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default(),
            ),
            _ => {
                return Err(AppError::invalid_input(
                    "an automatically selected GitHub Project field must be a single-select or iteration field",
                ));
            }
        };
        if options.is_empty() && !claimed.iter().any(|(claimed, _)| claimed == name) {
            return Err(AppError::invalid_input(
                "an automatically selected GitHub Project field has no available options",
            ));
        }
        if !claimed.iter().any(|(claimed, _)| claimed == name) {
            choices.push(ProjectFieldChoice {
                name: name.clone(),
                field_id: field.id.clone(),
                kind,
                options,
            });
        }
    }
    Ok(choices)
}

/// Finds one requested field and classifies its type.
///
/// Only the types that map to a documented `gh project item-edit` input are supported; anything
/// else is rejected before any write.
fn resolve_field<'a>(
    fields: &'a [RawField],
    name: &str,
) -> Result<(&'a RawField, FieldKind), AppError> {
    let field = fields
        .iter()
        .find(|field| field.name == name)
        .ok_or(AppError::invalid_input(
            "a requested GitHub Project field does not exist",
        ))?;
    let kind = match (field.typename.as_str(), field.data_type.as_str()) {
        ("ProjectV2Field", "TEXT") => FieldKind::Text,
        ("ProjectV2Field", "NUMBER") => FieldKind::Number,
        ("ProjectV2Field", "DATE") => FieldKind::Date,
        ("ProjectV2SingleSelectField", "SINGLE_SELECT") => FieldKind::SingleSelect,
        ("ProjectV2IterationField", "ITERATION") => FieldKind::Iteration,
        _ => {
            return Err(AppError::invalid_input(
                "a requested GitHub Project field has an unsupported type",
            ));
        }
    };
    Ok((field, kind))
}

/// Validates one explicit field value and maps it to the input `gh project item-edit` expects.
///
/// Single-select options and iterations resolve to their discovered IDs, so an unknown value
/// fails here instead of reaching the provider.
fn field_input(field: &RawField, kind: FieldKind, value: &str) -> Result<FieldInput, AppError> {
    match kind {
        FieldKind::Text => Ok(FieldInput::Text(value.to_owned())),
        FieldKind::Number => {
            value
                .parse::<f64>()
                .ok()
                .filter(|number| number.is_finite())
                .ok_or(AppError::invalid_input(
                    "a GitHub Project number field has an invalid value",
                ))?;
            Ok(FieldInput::Number(value.to_owned()))
        }
        FieldKind::Date => {
            if !valid_date(value) {
                return Err(AppError::invalid_input(
                    "a GitHub Project date field must use YYYY-MM-DD",
                ));
            }
            Ok(FieldInput::Date(value.to_owned()))
        }
        FieldKind::SingleSelect => {
            let option = field
                .options
                .iter()
                .find(|option| option.name == value)
                .ok_or(AppError::invalid_input(
                    "a GitHub Project single-select value is not an available option",
                ))?;
            Ok(FieldInput::Id(option.id.clone()))
        }
        FieldKind::Iteration => {
            let iteration = field
                .configuration
                .as_ref()
                .and_then(|config| config.iterations.iter().find(|item| item.title == value))
                .ok_or(AppError::invalid_input(
                    "a GitHub Project iteration is not available",
                ))?;
            Ok(FieldInput::Id(iteration.id.clone()))
        }
    }
}

fn resolve_assignments(
    fields: &[RawField],
    requested: &[(String, String)],
) -> Result<Vec<ProjectFieldAssignment>, AppError> {
    let mut assignments = Vec::with_capacity(requested.len());
    for (name, value) in requested {
        let (field, kind) = resolve_field(fields, name)?;
        assignments.push(ProjectFieldAssignment {
            name: name.clone(),
            field_id: field.id.clone(),
            kind,
            input: field_input(field, kind, value)?,
        });
    }
    Ok(assignments)
}

/// Resolves named fields into clear writes.
///
/// A clear still validates the field and its type, so an unknown or unsupported field fails
/// before any write exactly like a set.
fn resolve_clears(
    fields: &[RawField],
    requested: &[String],
) -> Result<Vec<ProjectFieldAssignment>, AppError> {
    let mut assignments = Vec::with_capacity(requested.len());
    for name in requested {
        let (field, kind) = resolve_field(fields, name)?;
        assignments.push(ProjectFieldAssignment {
            name: name.clone(),
            field_id: field.id.clone(),
            kind,
            input: FieldInput::Clear,
        });
    }
    Ok(assignments)
}

pub(super) fn add_item(plan: &ProjectPlan, issue_url: &str) -> Result<(), AppError> {
    let args = vec![
        "project".to_owned(),
        "item-add".to_owned(),
        plan.number.to_string(),
        "--owner".to_owned(),
        plan.owner.clone(),
        "--url".to_owned(),
        issue_url.to_owned(),
    ];
    super::run_gh(&args, None).map(|_| ())
}

/// Applies every planned field assignment serially, recording each success in `applied`.
///
/// A failure leaves the already applied names in `applied`, so the caller can report exactly the
/// completed and pending operations.
pub(super) fn set_fields(
    plan: &ProjectPlan,
    issue_url: &str,
    applied: &mut Vec<String>,
) -> Result<(), AppError> {
    for assignment in &plan.assignments {
        let mut args = vec![
            "project".to_owned(),
            "item-edit".to_owned(),
            "--project-id".to_owned(),
            plan.project_id.clone(),
            "--url".to_owned(),
            issue_url.to_owned(),
            "--field-id".to_owned(),
            assignment.field_id.clone(),
        ];
        match (&assignment.kind, &assignment.input) {
            (_, FieldInput::Clear) => args.push("--clear".to_owned()),
            (FieldKind::Text, FieldInput::Text(value)) => {
                args.extend(["--text".to_owned(), value.clone()])
            }
            (FieldKind::Number, FieldInput::Number(value)) => {
                args.extend(["--number".to_owned(), value.clone()])
            }
            (FieldKind::Date, FieldInput::Date(value)) => {
                args.extend(["--date".to_owned(), value.clone()])
            }
            (FieldKind::SingleSelect, FieldInput::Id(value)) => {
                args.extend(["--single-select-option-id".to_owned(), value.clone()])
            }
            (FieldKind::Iteration, FieldInput::Id(value)) => {
                args.extend(["--iteration-id".to_owned(), value.clone()])
            }
            _ => return Err(AppError::provider_response()),
        }
        super::run_gh(&args, None)?;
        applied.push(assignment.name.clone());
    }
    Ok(())
}

pub(super) fn pending(plan: &ProjectPlan, applied_count: usize) -> Vec<String> {
    plan.assignments
        .iter()
        .skip(applied_count)
        .map(|field| field.name.clone())
        .collect()
}

fn parse_project_url(url: &str) -> Result<(String, u64), AppError> {
    let Some(path) = url.strip_prefix("https://github.com/") else {
        return Err(AppError::config(
            "the configured GitHub Project URL is invalid",
        ));
    };
    let parts = path.trim_end_matches('/').split('/').collect::<Vec<_>>();
    if parts.len() != 4
        || !matches!(parts[0], "orgs" | "users")
        || parts[1].is_empty()
        || !parts[1]
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-')
    {
        return Err(AppError::config(
            "the configured GitHub Project URL is invalid",
        ));
    }
    let number = parts[3]
        .parse::<u64>()
        .ok()
        .filter(|number| *number > 0)
        .ok_or(AppError::config(
            "the configured GitHub Project URL is invalid",
        ))?;
    if parts[2] != "projects" {
        return Err(AppError::config(
            "the configured GitHub Project URL is invalid",
        ));
    }
    Ok((parts[1].to_owned(), number))
}

fn valid_date(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.len() != 10 || bytes[4] != b'-' || bytes[7] != b'-' {
        return false;
    }
    let (Ok(year), Ok(month), Ok(day)) = (
        value[0..4].parse::<u32>(),
        value[5..7].parse::<u32>(),
        value[8..10].parse::<u32>(),
    ) else {
        return false;
    };
    let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let max = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap => 29,
        2 => 28,
        _ => return false,
    };
    day > 0 && day <= max
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::DecisionScore;

    // RF-WI.6: Only a tie at the final maximum is ambiguous, regardless of option order.
    #[test]
    fn unique_best_option_wins_over_lower_ties_in_every_order() {
        for values in [
            [0.8, 0.8, 0.9],
            [0.8, 0.9, 0.8],
            [0.9, 0.8, 0.8],
        ] {
            let mut plan = choice_plan();
            let scores = scores(values);
            plan.choose_from_scores(&scores, 0.8).unwrap();
            let expected = values.iter().position(|score| *score == 0.9).unwrap();
            assert_eq!(plan.assignments.len(), 1);
            match &plan.assignments[0].input {
                FieldInput::Id(id) => assert_eq!(id, &format!("option-{expected}")),
                _ => panic!("expected selected option ID"),
            }
        }
    }

    // RF-WI.6: A qualifying maximum tie fails without recording any assignment.
    #[test]
    fn tied_maximum_is_rejected_in_every_order() {
        for values in [
            [0.9, 0.9, 0.8],
            [0.9, 0.8, 0.9],
            [0.8, 0.9, 0.9],
        ] {
            let mut plan = choice_plan();
            assert!(plan.choose_from_scores(&scores(values), 0.8).is_err());
            assert!(plan.assignments.is_empty());
        }
    }

    fn scores(values: [f64; 3]) -> Vec<DecisionScore> {
        values.into_iter().enumerate().map(|(index, probability)| DecisionScore {
            candidate: format!("Priority={index}"),
            probability,
        }).collect()
    }

    fn choice_plan() -> ProjectPlan {
        ProjectPlan {
            url: "https://github.com/orgs/owner/projects/3".to_owned(),
            project_id: "project".to_owned(),
            owner: "owner".to_owned(),
            number: 3,
            assignments: Vec::new(),
            choices: vec![ProjectFieldChoice {
                name: "Priority".to_owned(),
                field_id: "priority".to_owned(),
                kind: FieldKind::SingleSelect,
                options: (0..3).map(|index| FieldOption {
                    candidate: format!("Priority={index}"),
                    title: index.to_string(),
                    id: format!("option-{index}"),
                }).collect(),
            }],
        }
    }
}
