use std::collections::{HashMap, HashSet, VecDeque};

use crate::domain::{AppError, GitHubIssueView, IssueState, RelatedIssue};
use crate::providers::github::issues::{GitHubIssues, mapping, read};

const RELATION_FIELDS: &str =
    "nodes{number title state url} totalCount pageInfo{hasNextPage endCursor}";
const MAX_RELATIONSHIPS_PER_DIRECTION: usize = 1_000;
const MAX_BLOCKER_NODES: usize = 20;
const MAX_BLOCKER_EDGES: usize = 500;
const MAX_BLOCKER_PATHS: usize = 20;
const MAX_BLOCKER_DEPTH: usize = 20;
const MAX_BLOCKER_REQUESTS: usize = 20;

pub(super) fn view(provider: &GitHubIssues, number: u64) -> Result<GitHubIssueView, AppError> {
    let issue = read::show(provider, number)?;
    let query = format!(
        "query($owner:String!,$name:String!,$number:Int!){{\
         repository(owner:$owner,name:$name){{issue(number:$number){{\
         issueType{{name}} parent{{number title state url}} \
         subIssues(first:100){{{RELATION_FIELDS}}}\
         }}}}}}"
    );
    let relationships = mapping::relationships(&relationship_query(
        provider,
        &provider.repo,
        number,
        query,
        None,
    )?)?;
    Ok(GitHubIssueView {
        issue,
        issue_type: relationships.issue_type,
        parent: relationships.parent,
        sub_issues: complete_relation(
            provider,
            &provider.repo,
            number,
            "subIssues",
            relationships.sub_issues,
        )?,
    })
}

pub(super) fn blocker_chains(
    provider: &GitHubIssues,
    number: u64,
) -> Result<Vec<String>, AppError> {
    let issue = read::show(provider, number)?;
    if issue.state == IssueState::Closed {
        return Ok(Vec::new());
    }

    let root = GraphNode {
        repo: provider.repo.clone(),
        number,
        state: issue.state,
        display: format!("#{number}"),
        blockers: Vec::new(),
    };
    let root_key = node_key(&root.repo, number);
    let mut graph = HashMap::from([(root_key.clone(), root)]);
    let mut queue = VecDeque::from([root_key.clone()]);
    let mut edge_count = 0;
    let mut request_count = 0;

    while let Some(key) = queue.pop_front() {
        let (repo, issue_number) = {
            let issue = graph.get(&key).ok_or_else(AppError::provider_response)?;
            (issue.repo.clone(), issue.number)
        };
        request_count += 1;
        if request_count > MAX_BLOCKER_REQUESTS {
            return Err(AppError::relationship_limit());
        }
        let blockers = load_blockers(provider, &repo, issue_number, &mut request_count)?;
        for related in blockers {
            edge_count += 1;
            if edge_count > MAX_BLOCKER_EDGES {
                return Err(AppError::relationship_limit());
            }
            let (related_repo, related_number) = issue_url_parts(&related.url, related.number)?;
            let related_key = node_key(&related_repo, related_number);
            if let Some(existing) = graph.get(&related_key) {
                if existing.state != related.state {
                    return Err(AppError::conflict());
                }
            } else {
                if graph.len() >= MAX_BLOCKER_NODES {
                    return Err(AppError::relationship_limit());
                }
                let node = GraphNode {
                    display: display_reference(&related_repo, related_number, &provider.repo),
                    repo: related_repo,
                    number: related_number,
                    state: related.state,
                    blockers: Vec::new(),
                };
                if node.state == IssueState::Open {
                    queue.push_back(related_key.clone());
                }
                graph.insert(related_key.clone(), node);
            }
            graph
                .get_mut(&key)
                .ok_or_else(AppError::provider_response)?
                .blockers
                .push(related_key);
        }
    }

    let mut chains = Vec::new();
    let mut reverse_path = vec![root_key.clone()];
    let mut path = HashSet::from([root_key.clone()]);
    collect_chains(
        &root_key,
        &root_key,
        &graph,
        &mut reverse_path,
        &mut path,
        &mut chains,
    )?;
    chains.sort();
    chains.dedup();
    Ok(chains)
}

fn collect_chains(
    current_key: &str,
    root_key: &str,
    graph: &HashMap<String, GraphNode>,
    reverse_path: &mut Vec<String>,
    path: &mut HashSet<String>,
    chains: &mut Vec<String>,
) -> Result<(), AppError> {
    let current = graph
        .get(current_key)
        .ok_or_else(AppError::provider_response)?;
    let open_blockers: Vec<_> = current
        .blockers
        .iter()
        .filter(|key| {
            graph
                .get(*key)
                .is_some_and(|node| node.state == IssueState::Open)
        })
        .collect();

    if open_blockers.is_empty() {
        if current_key != root_key {
            if chains.len() >= MAX_BLOCKER_PATHS {
                return Err(AppError::relationship_limit());
            }
            let mut chain = reverse_path
                .iter()
                .rev()
                .filter_map(|key| graph.get(key).map(|node| node.display.as_str()));
            let first = chain.next().ok_or_else(AppError::provider_response)?;
            let mut rendered = String::from(first);
            for issue in chain {
                rendered.push_str(" -> ");
                rendered.push_str(issue);
            }
            chains.push(rendered);
        }
        return Ok(());
    }

    if reverse_path.len() >= MAX_BLOCKER_DEPTH {
        return Err(AppError::relationship_limit());
    }
    for blocker_key in open_blockers {
        if !path.insert(blocker_key.clone()) {
            return Err(AppError::provider_response());
        }
        reverse_path.push(blocker_key.clone());
        collect_chains(blocker_key, root_key, graph, reverse_path, path, chains)?;
        reverse_path.pop();
        path.remove(blocker_key);
    }
    Ok(())
}

fn load_blockers(
    provider: &GitHubIssues,
    repo: &str,
    number: u64,
    request_count: &mut usize,
) -> Result<Vec<RelatedIssue>, AppError> {
    let query = format!(
        "query($owner:String!,$name:String!,$number:Int!){{\
         repository(owner:$owner,name:$name){{issue(number:$number){{\
         relations:blockedBy(first:100){{{RELATION_FIELDS}}}\
         }}}}}}"
    );
    let first = mapping::relation_page(&relationship_query(provider, repo, number, query, None)?)?;
    complete_relation_count(first.total_count)?;
    let mut nodes = first.nodes;
    let total = first.total_count;
    let mut next_cursor = first.next_cursor;
    let mut seen_cursors = HashSet::new();

    while let Some(cursor) = next_cursor.take() {
        if seen_cursors.len() >= 9 {
            return Err(AppError::relationship_limit());
        }
        if !seen_cursors.insert(cursor.clone()) || nodes.len() >= total {
            return Err(AppError::provider_response());
        }
        *request_count += 1;
        if *request_count > MAX_BLOCKER_REQUESTS {
            return Err(AppError::relationship_limit());
        }
        let query = format!(
            "query($owner:String!,$name:String!,$number:Int!,$cursor:String!){{\
             repository(owner:$owner,name:$name){{issue(number:$number){{\
             relations:blockedBy(first:100,after:$cursor){{{RELATION_FIELDS}}}\
             }}}}}}"
        );
        let page = mapping::relation_page(&relationship_query(
            provider,
            repo,
            number,
            query,
            Some(&cursor),
        )?)?;
        if page.total_count != total || nodes.len() + page.nodes.len() > total {
            return Err(AppError::provider_response());
        }
        nodes.extend(page.nodes);
        next_cursor = page.next_cursor;
    }
    let mut urls = HashSet::with_capacity(nodes.len());
    if nodes.len() != total || nodes.iter().any(|node| !urls.insert(node.url.as_str())) {
        return Err(AppError::provider_response());
    }
    Ok(nodes)
}

fn complete_relation(
    provider: &GitHubIssues,
    repo: &str,
    number: u64,
    field: &str,
    first: mapping::RelationPage,
) -> Result<Vec<RelatedIssue>, AppError> {
    complete_relation_count(first.total_count)?;
    let total = first.total_count;
    let mut nodes = first.nodes;
    let mut next_cursor = first.next_cursor;
    let mut cursors = HashSet::new();
    for page_index in 0..9 {
        let Some(cursor) = next_cursor.take() else {
            let mut urls = HashSet::with_capacity(nodes.len());
            if nodes.len() != total || nodes.iter().any(|node| !urls.insert(node.url.as_str())) {
                return Err(AppError::provider_response());
            }
            return Ok(nodes);
        };
        if page_index == 9 {
            return Err(AppError::relationship_limit());
        }
        if nodes.len() >= total || !cursors.insert(cursor.clone()) {
            return Err(AppError::provider_response());
        }
        let query = format!(
            "query($owner:String!,$name:String!,$number:Int!,$cursor:String!){{\
             repository(owner:$owner,name:$name){{issue(number:$number){{\
             relations:{field}(first:100,after:$cursor){{{RELATION_FIELDS}}}\
             }}}}}}"
        );
        let page = mapping::relation_page(&relationship_query(
            provider,
            repo,
            number,
            query,
            Some(&cursor),
        )?)?;
        if page.total_count != total || nodes.len() + page.nodes.len() > total {
            return Err(AppError::provider_response());
        }
        nodes.extend(page.nodes);
        next_cursor = page.next_cursor;
    }
    Err(AppError::relationship_limit())
}

fn complete_relation_count(count: usize) -> Result<(), AppError> {
    if count > MAX_RELATIONSHIPS_PER_DIRECTION {
        Err(AppError::relationship_limit())
    } else {
        Ok(())
    }
}

fn relationship_query(
    provider: &GitHubIssues,
    repo: &str,
    number: u64,
    query: String,
    cursor: Option<&str>,
) -> Result<Vec<u8>, AppError> {
    let (owner, name) = repo
        .split_once('/')
        .ok_or_else(AppError::provider_response)?;
    let mut args = vec![
        "api".to_owned(),
        "graphql".to_owned(),
        "-f".to_owned(),
        format!("query={query}"),
        "-f".to_owned(),
        format!("owner={owner}"),
        "-f".to_owned(),
        format!("name={name}"),
        "-F".to_owned(),
        format!("number={number}"),
    ];
    if let Some(cursor) = cursor {
        args.extend(["-f".to_owned(), format!("cursor={cursor}")]);
    }
    provider.run_gh(&args, None)
}

fn issue_url_parts(url: &str, expected_number: u64) -> Result<(String, u64), AppError> {
    let path = url
        .strip_prefix("https://github.com/")
        .ok_or_else(AppError::provider_response)?;
    let mut parts = path.split('/');
    let owner = parts.next().ok_or_else(AppError::provider_response)?;
    let repo = parts.next().ok_or_else(AppError::provider_response)?;
    let kind = parts.next().ok_or_else(AppError::provider_response)?;
    let number = parts
        .next()
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|number| *number > 0)
        .ok_or_else(AppError::provider_response)?;
    let valid_part = |part: &str| {
        !part.is_empty()
            && part != "."
            && part != ".."
            && part
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b"._-".contains(&byte))
    };
    if kind != "issues"
        || parts.next().is_some()
        || !valid_part(owner)
        || !valid_part(repo)
        || number != expected_number
    {
        return Err(AppError::provider_response());
    }
    Ok((format!("{owner}/{repo}"), number))
}

fn node_key(repo: &str, number: u64) -> String {
    format!("{}/{number}", repo.to_ascii_lowercase())
}

fn display_reference(repo: &str, number: u64, base_repo: &str) -> String {
    if repo.eq_ignore_ascii_case(base_repo) {
        format!("#{number}")
    } else {
        format!("{repo}#{number}")
    }
}

struct GraphNode {
    repo: String,
    number: u64,
    state: IssueState,
    display: String,
    blockers: Vec<String>,
}
