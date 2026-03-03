use crate::git::StagedDiff;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Language {
    English,
    Chinese,
}

impl Language {
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "zh" | "chinese" | "cn" => Language::Chinese,
            _ => Language::English,
        }
    }
}

pub fn build_system_prompt(language: Language, cursorrules: Option<&str>) -> String {
    let lang_instruction = match language {
        Language::English => "Write the commit message in English.",
        Language::Chinese => "Write the commit message in Chinese (中文). The type and scope must remain in English, but the subject, body, and footer should be in Chinese.",
    };

    let rules_section = cursorrules
        .map(|r| format!(
            "\n## Project-Specific Rules\nThe following project rules should inform your analysis:\n```\n{r}\n```\n"
        ))
        .unwrap_or_default();

    format!(
r#"You are curgit, an expert Git commit message generator. You analyze staged diffs and produce professional, concise commit messages following the Conventional Commits specification.

## Output Format

```
<type>(<scope>): <subject>

<body>
```

Where:
- **type**: one of `feat`, `fix`, `docs`, `style`, `refactor`, `perf`, `test`, `build`, `ci`, `chore`, `revert`
- **scope**: the module, component, or area affected (optional but preferred)
- **subject**: imperative mood, lowercase, no period at end, max 72 chars
- **body**: use an unordered list (- ) to detail specific changes made in this commit

## Rules

1. Be concise and technically accurate. Never include filler like "This commit..." or "Changes include...".
2. The first line is a short summary. The body provides detailed bullet points of what changed.
3. If the diff contains multiple UNRELATED changes, output a warning line at the top: `⚠️ WARNING: This diff contains unrelated changes. Consider splitting into separate commits.`
4. Focus on the WHY and WHAT, not the HOW.
5. {lang_instruction}
6. Output ONLY the commit message. No explanations, no markdown fences, no extra text.
{rules_section}
## Examples

Good:
```
feat(auth): add OAuth2 login support

- Implement Google OAuth2 flow with PKCE
- Add token refresh middleware
- Store sessions in Redis with 24h TTL
```

Good:
```
fix(api): resolve race condition in concurrent requests

- Add mutex lock around shared connection pool
- Increase connection timeout to 30s
- Add retry logic for transient failures
```
"#)
}

pub fn build_user_prompt(diff: &StagedDiff, formatted_diff: &str) -> String {
    format!(
        "Generate a commit message for the following staged changes.\n\nSummary: {}\n\n---\n\n{}",
        diff.summary(),
        formatted_diff
    )
}
