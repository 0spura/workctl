use crate::cli::GlobalArgs;
use crate::cli::gitlab::mr::{
    CheckoutArgs, CreateArgs, DiffArgs, ListArgs, MergeArgs, MergeRequestAction, MergeRequestArgs,
    NoteAction, NoteArgs, NoteCreateArgs, NoteDiscussionArgs, NoteListArgs, NoteUpdateArgs,
    UpdateArgs, ViewArgs,
};
use crate::commands::support;
use crate::domain::AppError;
use crate::output::{self, SuccessOutput};
use crate::providers::gitlab::merge_requests::GitLabMergeRequests;

pub(super) fn execute(globals: &GlobalArgs, args: MergeRequestArgs) -> Result<(), AppError> {
    let provider = GitLabMergeRequests::new(support::resolve_repo(
        globals.code_provider,
        globals.repo.as_deref(),
    )?)?;
    if let MergeRequestAction::Note(NoteArgs {
        action: NoteAction::Create(args),
    }) = &args.action
    {
        validate_note(args)?;
    }
    provider.authenticate()?;
    let result = match args.action {
        MergeRequestAction::Create(args) => create(&provider, &args)?,
        MergeRequestAction::List(args) => list(&provider, &args)?,
        MergeRequestAction::Update(args) => update(&provider, &args)?,
        MergeRequestAction::View(args) => view(&provider, &args)?,
        MergeRequestAction::Diff(args) => diff(&provider, &args)?,
        MergeRequestAction::Checkout(args) => checkout(&provider, &args)?,
        MergeRequestAction::Close(args) => set_state(&provider, args.number.0, true)?,
        MergeRequestAction::Reopen(args) => set_state(&provider, args.number.0, false)?,
        MergeRequestAction::Approve(args) => native_action(&provider, args.number.0, "approve")?,
        MergeRequestAction::Revoke(args) => native_action(&provider, args.number.0, "revoke")?,
        MergeRequestAction::Rebase(args) => native_action(&provider, args.number.0, "rebase")?,
        MergeRequestAction::Subscribe(args) => {
            native_action(&provider, args.number.0, "subscribe")?
        }
        MergeRequestAction::Unsubscribe(args) => {
            native_action(&provider, args.number.0, "unsubscribe")?
        }
        MergeRequestAction::Approvers(args) => {
            SuccessOutput::ProviderData(provider.related_data("approvers", args.number.0)?)
        }
        MergeRequestAction::Issues(args) => {
            SuccessOutput::ProviderData(provider.related_data("issues", args.number.0)?)
        }
        MergeRequestAction::Merge(args) => merge(&provider, &args)?,
        MergeRequestAction::Todo(args) => native_action(&provider, args.number.0, "todo")?,
        MergeRequestAction::Note(args) => note(&provider, args.action)?,
    };
    output::write(globals.format, &result)
}

fn create(provider: &GitLabMergeRequests, args: &CreateArgs) -> Result<SuccessOutput, AppError> {
    let description = support::optional_text(
        args.description.as_deref(),
        args.description_file.as_deref(),
        "use either --description or --description-file",
    )?;
    let mut options = vec![
        "--title".to_owned(),
        args.title.clone(),
        "--description=".to_owned(),
        "--yes".to_owned(),
        "--no-editor".to_owned(),
    ];
    for (flag, value) in [
        ("--source-branch", args.source_branch.as_deref()),
        ("--target-branch", args.target_branch.as_deref()),
        ("--head", args.head.as_deref()),
        ("--milestone", args.milestone.as_deref()),
        ("--template", args.template.as_deref()),
    ] {
        if let Some(value) = value {
            options.push(format!("{flag}={value}"));
        }
    }
    for (flag, values) in [
        ("--assignee", &args.assignees),
        ("--label", &args.labels),
        ("--reviewer", &args.reviewers),
    ] {
        for value in values {
            options.push(format!("{flag}={value}"));
        }
    }
    if args.draft {
        options.push("--draft".to_owned());
    }
    if args.wip {
        options.push("--wip".to_owned());
    }
    if let Some(head) = args.related_issue {
        options.push(format!("--related-issue={head}"));
    }
    if args.copy_issue_labels {
        options.push("--copy-issue-labels".to_owned());
    }
    if let Some(value) = args.allow_collaboration {
        options.push(format!("--allow-collaboration={value}"));
    }
    if args.auto_merge {
        options.push("--auto-merge".to_owned());
    }
    if args.create_source_branch {
        options.push("--create-source-branch".to_owned());
    }
    if let Some(value) = args.remove_source_branch {
        options.push(format!("--remove-source-branch={value}"));
    }
    if let Some(value) = args.squash_before_merge {
        options.push(format!("--squash-before-merge={value}"));
    }
    for (enabled, flag) in [
        (args.push, "--push"),
        (args.fill, "--fill"),
        (args.fill_commit_body, "--fill-commit-body"),
    ] {
        if enabled {
            options.push(flag.to_owned());
        }
    }
    if args.signoff {
        options.push("--signoff".to_owned());
    }
    Ok(SuccessOutput::PullRequest(
        provider.create(&options, description.as_deref())?,
    ))
}

fn update(provider: &GitLabMergeRequests, args: &UpdateArgs) -> Result<SuccessOutput, AppError> {
    let description = support::optional_text(
        args.description.as_deref(),
        args.description_file.as_deref(),
        "use either --description or --description-file",
    )?;
    if description.as_deref() == Some("") {
        return Err(AppError::invalid_input(
            "merge request description must not be blank",
        ));
    }
    if (args.ready && (args.draft || args.wip)) || (args.lock_discussion && args.unlock_discussion)
    {
        return Err(AppError::invalid_input(
            "conflicting merge request update options",
        ));
    }
    let has_options = args.title.is_some()
        || !args.assignees.is_empty()
        || args.unassign
        || !args.labels.is_empty()
        || !args.unlabels.is_empty()
        || args.milestone.is_some()
        || args.target_branch.is_some()
        || !args.reviewers.is_empty()
        || args.draft
        || args.ready
        || args.wip
        || args.lock_discussion
        || args.unlock_discussion
        || args.remove_source_branch.is_some()
        || args.squash_before_merge.is_some()
        || args.fill
        || args.fill_commit_body
        || args.yes;
    if !has_options && description.is_none() {
        return Err(AppError::invalid_input("update requires a field to change"));
    }
    if args.unassign && !args.assignees.is_empty() {
        return Err(AppError::invalid_input(
            "--assignee and --unassign cannot be combined",
        ));
    }
    let mut options = Vec::new();
    for (flag, value) in [
        ("--title", args.title.as_deref()),
        ("--milestone", args.milestone.as_deref()),
        ("--target-branch", args.target_branch.as_deref()),
    ] {
        if let Some(value) = value {
            options.push(format!("{flag}={value}"));
        }
    }
    for (flag, values) in [
        ("--assignee", &args.assignees),
        ("--label", &args.labels),
        ("--unlabel", &args.unlabels),
        ("--reviewer", &args.reviewers),
    ] {
        for value in values {
            options.push(format!("{flag}={value}"));
        }
    }
    for (enabled, flag) in [
        (args.unassign, "--unassign"),
        (args.draft, "--draft"),
        (args.ready, "--ready"),
        (args.wip, "--wip"),
        (args.lock_discussion, "--lock-discussion"),
        (args.unlock_discussion, "--unlock-discussion"),
        (args.fill, "--fill"),
        (args.fill_commit_body, "--fill-commit-body"),
        (args.yes, "--yes"),
    ] {
        if enabled {
            options.push(flag.to_owned());
        }
    }
    if let Some(value) = args.remove_source_branch {
        options.push(format!("--remove-source-branch={value}"));
    }
    if let Some(value) = args.squash_before_merge {
        options.push(format!("--squash-before-merge={value}"));
    }
    Ok(SuccessOutput::PullRequest(provider.update(
        args.number.0,
        &options,
        description.as_deref(),
    )?))
}

fn list(provider: &GitLabMergeRequests, args: &ListArgs) -> Result<SuccessOutput, AppError> {
    let mut filters = Vec::new();
    for (enabled, flag) in [
        (args.all, "--all"),
        (args.closed, "--closed"),
        (args.merged, "--merged"),
        (args.draft, "--draft"),
        (args.not_draft, "--not-draft"),
    ] {
        if enabled {
            filters.push(flag.to_owned());
        }
    }
    for (flag, labels) in [("--label", &args.labels), ("--not-label", &args.not_labels)] {
        for label in labels {
            filters.extend([flag.to_owned(), label.clone()]);
        }
    }
    for (flag, value) in [
        ("--assignee", &args.assignee),
        ("--author", &args.author),
        ("--reviewer", &args.reviewer),
        ("--milestone", &args.milestone),
        ("--source-branch", &args.source_branch),
        ("--target-branch", &args.target_branch),
        ("--search", &args.search),
        ("--created-after", &args.created_after),
        ("--created-before", &args.created_before),
        ("--deployed-after", &args.deployed_after),
        ("--deployed-before", &args.deployed_before),
        ("--environment", &args.environment),
        ("--order", &args.order),
        ("--sort", &args.sort),
    ] {
        if let Some(value) = value {
            filters.extend([flag.to_owned(), value.clone()]);
        }
    }
    filters.extend(["--page".to_owned(), args.page.to_string()]);
    filters.extend(["--per-page".to_owned(), args.per_page.to_string()]);
    Ok(SuccessOutput::PullRequests(provider.list(&filters)?))
}

fn view(provider: &GitLabMergeRequests, args: &ViewArgs) -> Result<SuccessOutput, AppError> {
    Ok(SuccessOutput::PullRequest(provider.view(args.number.0)?))
}

fn diff(provider: &GitLabMergeRequests, args: &DiffArgs) -> Result<SuccessOutput, AppError> {
    Ok(SuccessOutput::Diff {
        number: args.number.0,
        diff: provider.diff(args.number.0, args.raw)?,
    })
}

fn checkout(
    provider: &GitLabMergeRequests,
    args: &CheckoutArgs,
) -> Result<SuccessOutput, AppError> {
    provider.checkout(args.number.0)?;
    Ok(SuccessOutput::Checkout {
        number: args.number.0,
    })
}

fn set_state(
    provider: &GitLabMergeRequests,
    number: u64,
    closed: bool,
) -> Result<SuccessOutput, AppError> {
    provider.set_state(number, closed)?;
    Ok(SuccessOutput::State {
        number,
        state: if closed { "closed" } else { "open" }.to_owned(),
    })
}

fn native_action(
    provider: &GitLabMergeRequests,
    number: u64,
    action: &str,
) -> Result<SuccessOutput, AppError> {
    provider.action(action, number)?;
    Ok(SuccessOutput::GitLabAction {
        number,
        action: action.replace('-', " "),
    })
}

fn merge(provider: &GitLabMergeRequests, args: &MergeArgs) -> Result<SuccessOutput, AppError> {
    let mut options = Vec::new();
    if let Some(auto_merge) = args.auto_merge {
        options.push(format!("--auto-merge={auto_merge}"));
    }
    if let Some(message) = &args.message {
        options.extend(["--message".to_owned(), message.clone()]);
    }
    if args.rebase {
        options.push("--rebase".to_owned());
    }
    if let Some(remove_source_branch) = args.remove_source_branch {
        options.push(format!("--remove-source-branch={remove_source_branch}"));
    }
    if let Some(sha) = &args.sha {
        options.extend(["--sha".to_owned(), sha.clone()]);
    }
    if args.squash {
        options.push("--squash".to_owned());
    }
    if let Some(message) = &args.squash_message {
        options.extend(["--squash-message".to_owned(), message.clone()]);
    }
    if args.yes {
        options.push("--yes".to_owned());
    }
    provider.merge(&options, args.number.0)?;
    Ok(SuccessOutput::GitLabAction {
        number: args.number.0,
        action: if args.auto_merge == Some(true) {
            "merge queued".to_owned()
        } else {
            "merged".to_owned()
        },
    })
}

fn note(provider: &GitLabMergeRequests, action: NoteAction) -> Result<SuccessOutput, AppError> {
    match action {
        NoteAction::Create(args) => create_note(provider, args),
        NoteAction::List(args) => list_notes(provider, args),
        NoteAction::Update(args) => update_note(provider, args),
        NoteAction::Resolve(args) => discussion_action(provider, args, "resolve"),
        NoteAction::Reopen(args) => discussion_action(provider, args, "reopen"),
    }
}

fn list_notes(
    provider: &GitLabMergeRequests,
    args: NoteListArgs,
) -> Result<SuccessOutput, AppError> {
    let mut filters = Vec::new();
    if let Some(state) = args.state {
        filters.extend(["--state".to_owned(), state]);
    }
    if let Some(note_type) = args.note_type {
        filters.extend(["--type".to_owned(), note_type]);
    }
    if let Some(file) = args.file {
        filters.extend(["--file".to_owned(), file]);
    }
    Ok(SuccessOutput::ProviderData(
        provider.list_notes(args.number.0, &filters)?,
    ))
}

fn update_note(
    provider: &GitLabMergeRequests,
    args: NoteUpdateArgs,
) -> Result<SuccessOutput, AppError> {
    let body = support::optional_text(
        args.message.as_deref(),
        args.body_file.as_deref(),
        "use either --message or --body-file",
    )?
    .filter(|body| !body.trim().is_empty())
    .ok_or_else(|| {
        AppError::invalid_input("note update requires a nonblank --message or --body-file")
    })?;
    provider.update_note(args.number.0, args.note_id, &body)?;
    Ok(SuccessOutput::GitLabAction {
        number: args.number.0,
        action: "note updated".to_owned(),
    })
}

fn discussion_action(
    provider: &GitLabMergeRequests,
    args: NoteDiscussionArgs,
    action: &str,
) -> Result<SuccessOutput, AppError> {
    provider.note_discussion_action(action, &args.discussion_id, args.number.0)?;
    Ok(SuccessOutput::GitLabAction {
        number: args.number.0,
        action: format!(
            "discussion {}",
            if action == "resolve" {
                "resolved"
            } else {
                "reopened"
            }
        ),
    })
}

fn validate_note(args: &NoteCreateArgs) -> Result<(), AppError> {
    if args.resolvable == Some(false) && (args.file.is_some() || args.reply.is_some()) {
        return Err(AppError::invalid_input(
            "--resolvable=false cannot be combined with --file or --reply",
        ));
    }
    Ok(())
}
fn create_note(
    provider: &GitLabMergeRequests,
    args: NoteCreateArgs,
) -> Result<SuccessOutput, AppError> {
    validate_note(&args)?;
    let mut options = Vec::new();
    if let Some(file) = &args.file {
        options.extend(["--file".to_owned(), file.clone()]);
    }
    if let Some(line) = &args.line {
        options.extend(["--line".to_owned(), line.clone()]);
    }
    if let Some(old_line) = args.old_line {
        options.extend(["--old-line".to_owned(), old_line.to_string()]);
    }
    if let Some(reply) = &args.reply {
        options.extend(["--reply".to_owned(), reply.clone()]);
    }
    if let Some(resolvable) = args.resolvable {
        options.push(format!("--resolvable={resolvable}"));
    }
    if args.unique {
        options.push("--unique".to_owned());
    }
    provider.create_note(args.number.0, &args.message, &options)?;
    Ok(SuccessOutput::GitLabAction {
        number: args.number.0,
        action: "note added".to_owned(),
    })
}
