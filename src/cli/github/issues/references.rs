use crate::cli::common::IssueNumber;

/// Canonical references are parsed before provider access; no arbitrary URL reaches a mutation.
#[derive(Debug, Clone)]
pub struct IssueReference {
    pub number: u64,
    pub repo: Option<String>,
}

impl std::str::FromStr for IssueReference {
    type Err = &'static str;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        if let Ok(number) = value.parse::<IssueNumber>() {
            return Ok(Self {
                number: number.0,
                repo: None,
            });
        }
        let invalid = "expected a positive issue number or canonical https://github.com/OWNER/REPO/issues/NUMBER URL";
        let path = value.strip_prefix("https://github.com/").ok_or(invalid)?;
        let mut parts = path.split('/');
        let owner = parts.next().ok_or(invalid)?;
        let repo = parts.next().ok_or(invalid)?;
        let kind = parts.next().ok_or(invalid)?;
        let number = parts
            .next()
            .ok_or(invalid)?
            .parse::<IssueNumber>()
            .map_err(|_| invalid)?;
        let valid_part = |part: &str| {
            !part.is_empty()
                && part != "."
                && part != ".."
                && part
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || b"._-".contains(&byte))
        };
        if kind != "issues" || parts.next().is_some() || !valid_part(owner) || !valid_part(repo) {
            return Err(invalid);
        }
        Ok(Self {
            number: number.0,
            repo: Some(format!("{owner}/{repo}")),
        })
    }
}

impl IssueReference {
    pub fn into_argument(self) -> String {
        match self.repo {
            Some(repo) => format!("https://github.com/{repo}/issues/{}", self.number),
            None => self.number.to_string(),
        }
    }
}
