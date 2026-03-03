use anyhow::{bail, Context, Result};
use serde::Deserialize;
use std::collections::{HashMap, HashSet};
use std::io::Write;
use std::process::{Command, Stdio};

use crate::git::StagedDiff;
use crate::llm::LlmConfig;

#[derive(Debug, Clone, Deserialize)]
pub struct CommitGroup {
    #[serde(default)]
    pub files: Vec<String>,
    #[serde(default)]
    pub hunks: Vec<String>,
    pub message: String,
}

#[derive(Debug, Clone)]
pub struct ParsedStagedPatch {
    files: Vec<PatchFile>,
    pub hunks: Vec<StagedHunk>,
    pub files_without_hunks: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct StagedHunk {
    pub id: String,
    pub file: String,
    pub patch: String,
}

#[derive(Debug, Clone)]
struct PatchFile {
    path: String,
    prelude: Vec<String>,
    hunks: Vec<PatchHunk>,
}

#[derive(Debug, Clone)]
struct PatchHunk {
    id: String,
    lines: Vec<String>,
}

pub fn parse_staged_patch() -> Result<ParsedStagedPatch> {
    let output = Command::new("git")
        .args(["diff", "--cached", "--patch", "--no-color", "--no-ext-diff"])
        .output()
        .context("Failed to read staged patch via git diff --cached")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("git diff --cached failed: {stderr}");
    }

    let raw_patch = String::from_utf8_lossy(&output.stdout).to_string();
    Ok(parse_patch_text(&raw_patch))
}

fn parse_patch_text(raw_patch: &str) -> ParsedStagedPatch {
    let mut files: Vec<PatchFile> = Vec::new();
    let mut current_file: Option<PatchFile> = None;
    let mut current_hunk: Option<PatchHunk> = None;

    for line in raw_patch.lines() {
        if line.starts_with("diff --git ") {
            if let Some(hunk) = current_hunk.take() {
                if let Some(file) = current_file.as_mut() {
                    file.hunks.push(hunk);
                }
            }
            if let Some(file) = current_file.take() {
                files.push(file);
            }

            current_file = Some(PatchFile {
                path: parse_path_from_diff_header(line),
                prelude: vec![line.to_string()],
                hunks: Vec::new(),
            });
            continue;
        }

        let Some(file) = current_file.as_mut() else {
            continue;
        };

        if line.starts_with("@@ ") {
            if let Some(hunk) = current_hunk.take() {
                file.hunks.push(hunk);
            }
            current_hunk = Some(PatchHunk {
                id: String::new(),
                lines: vec![line.to_string()],
            });
            continue;
        }

        if let Some(hunk) = current_hunk.as_mut() {
            hunk.lines.push(line.to_string());
        } else {
            file.prelude.push(line.to_string());
        }
    }

    if let Some(hunk) = current_hunk {
        if let Some(file) = current_file.as_mut() {
            file.hunks.push(hunk);
        }
    }
    if let Some(file) = current_file {
        files.push(file);
    }

    let mut hunks: Vec<StagedHunk> = Vec::new();
    let mut next_id = 1usize;
    for file in &mut files {
        for hunk in &mut file.hunks {
            let id = format!("H{next_id}");
            next_id += 1;
            hunk.id = id.clone();
            hunks.push(StagedHunk {
                id,
                file: file.path.clone(),
                patch: hunk.lines.join("\n"),
            });
        }
    }

    let files_without_hunks = files
        .iter()
        .filter(|f| f.hunks.is_empty())
        .map(|f| f.path.clone())
        .collect();

    ParsedStagedPatch {
        files,
        hunks,
        files_without_hunks,
    }
}

fn parse_path_from_diff_header(line: &str) -> String {
    let parts: Vec<&str> = line.split_whitespace().collect();
    if parts.len() >= 4 {
        if let Some(path) = parts[3].strip_prefix("b/") {
            return path.to_string();
        }
        if let Some(path) = parts[2].strip_prefix("a/") {
            return path.to_string();
        }
    }
    "<unknown>".to_string()
}

pub fn build_split_prompt(
    diff: &StagedDiff,
    formatted_diff: &str,
    staged_patch: &ParsedStagedPatch,
    language: &str,
) -> String {
    let file_list: Vec<String> = diff
        .files
        .iter()
        .map(|f| format!("- {} ({})", f.path, f.status))
        .collect();

    let hunk_list = if staged_patch.hunks.is_empty() {
        "No textual hunks were found.".to_string()
    } else {
        staged_patch
            .hunks
            .iter()
            .map(|h| format!("### [{}] {}\n{}", h.id, h.file, h.patch))
            .collect::<Vec<_>>()
            .join("\n\n")
    };

    let files_without_hunks = if staged_patch.files_without_hunks.is_empty() {
        "None".to_string()
    } else {
        staged_patch
            .files_without_hunks
            .iter()
            .map(|f| format!("- {f}"))
            .collect::<Vec<_>>()
            .join("\n")
    };

    let lang_note = if language == "zh" {
        "Write the commit messages in Chinese (中文), but keep type and scope in English."
    } else {
        "Write the commit messages in English."
    };

    format!(
        r#"You are a Git commit splitting expert. Analyze the staged diff and split it into logical, atomic commits by semantic intent.

## Changed files
{file_list}

## Full diff (for context)
{formatted_diff}

## Hunk inventory (for assignment)
{hunk_list}

## Files without textual hunks (e.g., binary/metadata-only changes)
{files_without_hunks}

## Instructions
1. Split by feature/intent, not by size.
2. Different functionalities or concerns SHOULD be in different commits.
3. If all changes are truly one concern, return exactly one commit group.
4. Assign each textual hunk to exactly one group via `hunks`.
5. Assign files without textual hunks via `files`.
6. For each group, provide a Conventional Commits message with bullet points in body.
7. {lang_note}

## Output format
Output ONLY valid JSON array. No markdown, no explanation.
Each item must contain:
- `hunks`: array of hunk IDs (e.g. ["H1","H3"])
- `files`: array of whole-file paths that should be staged as-is (usually only for files without textual hunks)
- `message`: commit message string

Example:
[
  {{
    "hunks": ["H1", "H2"],
    "files": [],
    "message": "feat(auth): add OAuth2 login support\n\n- Implement Google OAuth2 flow\n- Add token refresh middleware"
  }},
  {{
    "hunks": [],
    "files": ["assets/logo.png"],
    "message": "chore(assets): add new logo asset\n\n- Add high-resolution brand logo"
  }}
]
"#,
        file_list = file_list.join("\n"),
        formatted_diff = formatted_diff,
        hunk_list = hunk_list,
        files_without_hunks = files_without_hunks,
        lang_note = lang_note,
    )
}

pub fn parse_split_response(response: &str) -> Result<Vec<CommitGroup>> {
    let trimmed = response.trim();

    let json_str = if trimmed.starts_with("```") {
        let inner = trimmed
            .strip_prefix("```")
            .unwrap_or(trimmed)
            .trim_start_matches(|c: char| c.is_alphabetic() || c == '\n');
        inner.strip_suffix("```").unwrap_or(inner).trim()
    } else {
        trimmed
    };

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
    staged_patch: &ParsedStagedPatch,
    language: &str,
) -> Result<Vec<CommitGroup>> {
    let system_prompt =
        "You are a Git commit splitting assistant. Output ONLY valid JSON. No markdown, no explanation.";
    let user_prompt = build_split_prompt(diff, formatted_diff, staged_patch, language);

    let response = crate::llm::generate_commit_message(config, system_prompt, &user_prompt).await?;

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
            format!("Commit {}/{}", i + 1, groups.len()).cyan().bold()
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

        if !group.hunks.is_empty() {
            println!(
                "  {} {}",
                "Hunks:".dimmed(),
                format!("({})", group.hunks.len()).dimmed()
            );
            for h in &group.hunks {
                println!("    {}", format!("• {h}").dimmed());
            }
        }

        if !group.files.is_empty() {
            println!("  {}", "Files:".dimmed());
            for file in &group.files {
                println!("    {}", format!("• {file}").dimmed());
            }
        }
    }

    println!();
    println!("{}", "─".repeat(60).dimmed());
}

pub fn validate_split_plan(
    groups: &[CommitGroup],
    diff: &StagedDiff,
    staged_patch: &ParsedStagedPatch,
) -> Vec<String> {
    let mut warnings = Vec::new();

    let known_hunks: HashSet<&str> = staged_patch.hunks.iter().map(|h| h.id.as_str()).collect();
    let mut assigned_hunk_count: HashMap<&str, usize> = HashMap::new();

    for group in groups {
        for h in &group.hunks {
            let h = h.as_str();
            if !known_hunks.contains(h) {
                warnings.push(format!(
                    "Hunk '{h}' in split plan but not in staged changes"
                ));
                continue;
            }
            *assigned_hunk_count.entry(h).or_insert(0) += 1;
        }
    }

    for h in &staged_patch.hunks {
        match assigned_hunk_count.get(h.id.as_str()).copied().unwrap_or(0) {
            0 => warnings.push(format!(
                "Hunk '{}' ({}) is staged but not included in any commit group",
                h.id, h.file
            )),
            1 => {}
            n => warnings.push(format!(
                "Hunk '{}' is assigned to {} commit groups",
                h.id, n
            )),
        }
    }

    let diff_files: HashSet<&str> = diff.files.iter().map(|f| f.path.as_str()).collect();
    let files_without_hunks: HashSet<&str> = staged_patch
        .files_without_hunks
        .iter()
        .map(String::as_str)
        .collect();
    let mut assigned_file_count: HashMap<&str, usize> = HashMap::new();

    for group in groups {
        for f in &group.files {
            let f = f.as_str();
            if !diff_files.contains(f) {
                warnings.push(format!(
                    "File '{f}' in split plan but not in staged changes"
                ));
                continue;
            }
            *assigned_file_count.entry(f).or_insert(0) += 1;
            if !files_without_hunks.contains(f) {
                warnings.push(format!(
                    "File '{f}' was assigned as whole-file staging, but it also has textual hunks"
                ));
            }
        }
    }

    for f in files_without_hunks {
        match assigned_file_count.get(f).copied().unwrap_or(0) {
            0 => warnings.push(format!(
                "File '{f}' has no textual hunks and is not included in any commit group"
            )),
            1 => {}
            n => warnings.push(format!("File '{f}' is assigned to {} commit groups", n)),
        }
    }

    warnings
}

/// Execute the split plan: re-stage and commit by hunk/file groups.
pub fn execute_split_plan(groups: &[CommitGroup], staged_patch: &ParsedStagedPatch) -> Result<()> {
    let all_paths: Vec<&str> = staged_patch.files.iter().map(|f| f.path.as_str()).collect();
    unstage_files(&all_paths)?;

    for (i, group) in groups.iter().enumerate() {
        if !group.hunks.is_empty() {
            let patch = build_group_patch(staged_patch, &group.hunks)?;
            if patch.trim().is_empty() {
                bail!("Split plan referenced hunks but produced an empty patch");
            }
            apply_patch_to_index(&patch)?;
        }

        if !group.files.is_empty() {
            let file_refs: Vec<&str> = group.files.iter().map(|s| s.as_str()).collect();
            stage_files(&file_refs)?;
        }

        if !has_staged_changes()? {
            bail!(
                "Commit {}/{} has no staged changes. Check split plan assignments.",
                i + 1,
                groups.len()
            );
        }

        let status = Command::new("git")
            .args(["commit", "-m", &group.message])
            .status()
            .with_context(|| format!("Failed to run git commit for group {}", i + 1))?;

        if !status.success() {
            bail!("git commit failed for commit {}/{}", i + 1, groups.len());
        }

        crate::cli::print_success(&format!("Commit {}/{} created", i + 1, groups.len()));
    }

    Ok(())
}

fn build_group_patch(staged_patch: &ParsedStagedPatch, hunk_ids: &[String]) -> Result<String> {
    let selected: HashSet<&str> = hunk_ids.iter().map(String::as_str).collect();
    let known: HashSet<&str> = staged_patch.hunks.iter().map(|h| h.id.as_str()).collect();
    for id in &selected {
        if !known.contains(id) {
            bail!("Unknown hunk ID '{id}' in split plan");
        }
    }

    let mut patch = String::new();
    for file in &staged_patch.files {
        let selected_hunks: Vec<&PatchHunk> = file
            .hunks
            .iter()
            .filter(|h| selected.contains(h.id.as_str()))
            .collect();
        if selected_hunks.is_empty() {
            continue;
        }

        for line in &file.prelude {
            patch.push_str(line);
            patch.push('\n');
        }
        for hunk in selected_hunks {
            for line in &hunk.lines {
                patch.push_str(line);
                patch.push('\n');
            }
        }
    }

    Ok(patch)
}

fn apply_patch_to_index(patch: &str) -> Result<()> {
    let mut child = Command::new("git")
        .args(["apply", "--cached", "--recount", "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .context("Failed to launch git apply --cached")?;

    {
        let stdin = child
            .stdin
            .as_mut()
            .context("Failed to open stdin for git apply")?;
        stdin
            .write_all(patch.as_bytes())
            .context("Failed to write patch to git apply stdin")?;
    }

    let output = child
        .wait_with_output()
        .context("Failed to wait for git apply")?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("git apply --cached failed: {stderr}");
    }
    Ok(())
}

fn has_staged_changes() -> Result<bool> {
    let status = Command::new("git")
        .args(["diff", "--cached", "--quiet"])
        .status()
        .context("Failed to check staged changes")?;
    Ok(!status.success())
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
