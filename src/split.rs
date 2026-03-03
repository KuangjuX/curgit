use anyhow::{bail, Context, Result};
use serde::Deserialize;
use std::process::Command;

use crate::git::{FileChange, StagedDiff};
use crate::llm::LlmConfig;

const SPLIT_THRESHOLD_FILES: usize = 8;
const SPLIT_THRESHOLD_HUNKS: usize = 20;

#[derive(Debug, Clone, Deserialize)]
pub struct CommitGroup {
    pub files: Vec<String>,
    pub message: String,
}

pub fn should_suggest_split(diff: &StagedDiff) -> bool {
    let total_hunks: usize = diff.files.iter().map(|f| f.hunks.len()).sum();
    diff.files.len() >= SPLIT_THRESHOLD_FILES || total_hunks >= SPLIT_THRESHOLD_HUNKS
}

pub fn build_split_prompt(diff: &StagedDiff, formatted_diff: &str, language: &str) -> String {
    let file_list: Vec<String> = diff
        .files
        .iter()
        .map(|f| format!("- {} ({})", f.path, f.status))
        .collect();

    let lang_note = if language == "zh" {
        "Write the commit messages in Chinese (中文), but keep type and scope in English."
    } else {
        "Write the commit messages in English."
    };

    format!(
r#"You are a Git commit splitting expert. Analyze the following staged diff and split it into logical, atomic commits.

## Changed files
{file_list}

## Full diff
{formatted_diff}

## Instructions

1. Group the files by logical change (e.g., feature A files together, refactoring files together, config changes together).
2. Each group should be ONE atomic commit that makes sense on its own.
3. If all changes are closely related, return a single group — do NOT split unnecessarily.
4. For each group, provide a Conventional Commits message with a body using bullet points.
5. {lang_note}

## Output Format

You MUST output ONLY valid JSON — no markdown fences, no explanation, no extra text.
The JSON must be an array of objects, each with "files" (array of file paths) and "message" (the commit message string).

Example:
[
  {{
    "files": ["src/auth.rs", "src/middleware.rs"],
    "message": "feat(auth): add OAuth2 login support\n\n- Implement Google OAuth2 flow\n- Add token refresh middleware"
  }},
  {{
    "files": ["README.md"],
    "message": "docs: update README with auth setup instructions\n\n- Add OAuth2 configuration section\n- Document environment variables"
  }}
]
"#,
        file_list = file_list.join("\n"),
        formatted_diff = formatted_diff,
        lang_note = lang_note,
    )
}

pub fn parse_split_response(response: &str) -> Result<Vec<CommitGroup>> {
    let trimmed = response.trim();

    // Strip markdown code fences if present
    let json_str = if trimmed.starts_with("```") {
        let inner = trimmed
            .strip_prefix("```")
            .unwrap_or(trimmed)
            .trim_start_matches(|c: char| c.is_alphabetic() || c == '\n');
        inner.strip_suffix("```").unwrap_or(inner).trim()
    } else {
        trimmed
    };

    // Try to find JSON array in the response
    let json_str = if let Some(start) = json_str.find('[') {
        if let Some(end) = json_str.rfind(']') {
            &json_str[start..=end]
        } else {
            json_str
        }
    } else {
        json_str
    };

    let groups: Vec<CommitGroup> =
        serde_json::from_str(json_str).context("Failed to parse split plan from LLM response")?;

    if groups.is_empty() {
        bail!("LLM returned an empty split plan");
    }

    Ok(groups)
}

pub async fn generate_split_plan(
    config: &LlmConfig,
    diff: &StagedDiff,
    formatted_diff: &str,
    language: &str,
) -> Result<Vec<CommitGroup>> {
    let system_prompt =
        "You are a Git commit splitting assistant. Output ONLY valid JSON. No markdown, no explanation.";
    let user_prompt = build_split_prompt(diff, formatted_diff, language);

    let response =
        crate::llm::generate_commit_message(config, system_prompt, &user_prompt).await?;

    parse_split_response(&response)
}

pub fn display_split_plan(groups: &[CommitGroup]) {
    use colored::Colorize;

    println!();
    println!("{}", "─".repeat(60).dimmed());
    println!(
        "{}",
        format!("  Split plan: {} commits", groups.len()).bold()
    );
    println!("{}", "─".repeat(60).dimmed());

    for (i, group) in groups.iter().enumerate() {
        println!();
        println!(
            "  {}",
            format!("Commit {}/{}", i + 1, groups.len())
                .cyan()
                .bold()
        );

        let first_line = group.message.lines().next().unwrap_or(&group.message);
        println!("  {}", first_line.green().bold());

        for line in group.message.lines().skip(1) {
            if line.starts_with("- ") || line.starts_with("* ") {
                println!("  {}", line.white());
            } else if !line.is_empty() {
                println!("  {}", line.dimmed());
            }
        }

        println!("  {}", "Files:".dimmed());
        for file in &group.files {
            println!("    {}", format!("• {file}").dimmed());
        }
    }

    println!();
    println!("{}", "─".repeat(60).dimmed());
}

pub fn validate_split_plan(groups: &[CommitGroup], diff: &StagedDiff) -> Vec<String> {
    let mut warnings = Vec::new();

    let all_diff_files: Vec<&str> = diff.files.iter().map(|f| f.path.as_str()).collect();
    let all_plan_files: Vec<&str> = groups.iter().flat_map(|g| g.files.iter().map(|f| f.as_str())).collect();

    for f in &all_diff_files {
        if !all_plan_files.contains(f) {
            warnings.push(format!("File '{f}' is staged but not included in any commit group"));
        }
    }

    for f in &all_plan_files {
        if !all_diff_files.contains(f) {
            warnings.push(format!("File '{f}' in split plan but not in staged changes"));
        }
    }

    warnings
}

/// Execute the split plan: for each group, selectively stage files and commit.
/// Before starting, all staged files are unstaged, then re-staged per group.
pub fn execute_split_plan(groups: &[CommitGroup], all_files: &[FileChange]) -> Result<()> {
    // Unstage everything first
    let all_paths: Vec<&str> = all_files.iter().map(|f| f.path.as_str()).collect();
    unstage_files(&all_paths)?;

    for (i, group) in groups.iter().enumerate() {
        let file_refs: Vec<&str> = group.files.iter().map(|s| s.as_str()).collect();

        stage_files(&file_refs)?;

        let status = Command::new("git")
            .args(["commit", "-m", &group.message])
            .status()
            .with_context(|| format!("Failed to run git commit for group {}", i + 1))?;

        if !status.success() {
            // Re-stage remaining files so user doesn't lose work
            let remaining: Vec<&str> = groups[i + 1..]
                .iter()
                .flat_map(|g| g.files.iter().map(|s| s.as_str()))
                .collect();
            if !remaining.is_empty() {
                let _ = stage_files(&remaining);
            }
            bail!("git commit failed for commit {}/{}", i + 1, groups.len());
        }

        crate::cli::print_success(&format!(
            "Commit {}/{} created",
            i + 1,
            groups.len()
        ));
    }

    Ok(())
}

fn unstage_files(files: &[&str]) -> Result<()> {
    if files.is_empty() {
        return Ok(());
    }
    let mut cmd = Command::new("git");
    cmd.arg("reset").arg("HEAD").arg("--");
    for f in files {
        cmd.arg(f);
    }
    let status = cmd.status().context("Failed to unstage files")?;
    if !status.success() {
        bail!("git reset HEAD failed");
    }
    Ok(())
}

fn stage_files(files: &[&str]) -> Result<()> {
    if files.is_empty() {
        return Ok(());
    }
    let mut cmd = Command::new("git");
    cmd.arg("add").arg("--");
    for f in files {
        cmd.arg(f);
    }
    let status = cmd.status().context("Failed to stage files")?;
    if !status.success() {
        bail!("git add failed");
    }
    Ok(())
}
