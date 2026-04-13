use crate::llm::AuthorConfig;
use anyhow::{Context, Result};
use git2::{Delta, DiffOptions, Repository};
use std::path::Path;
use std::process::Command;

/// Build a `git` command with optional `-c user.name` / `-c user.email` before subcommands like `commit`.
pub fn git_with_author(author: &AuthorConfig) -> Command {
    let mut cmd = Command::new("git");
    if let Some(ref n) = author.name {
        cmd.arg("-c").arg(format!("user.name={n}"));
    }
    if let Some(ref e) = author.email {
        cmd.arg("-c").arg(format!("user.email={e}"));
    }
    cmd
}

#[derive(Debug, Clone)]
pub struct FileChange {
    pub path: String,
    pub status: ChangeStatus,
    pub hunks: Vec<Hunk>,
}

#[derive(Debug, Clone)]
pub enum ChangeStatus {
    Added,
    Modified,
    Deleted,
    Renamed(String),
}

impl std::fmt::Display for ChangeStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ChangeStatus::Added => write!(f, "added"),
            ChangeStatus::Modified => write!(f, "modified"),
            ChangeStatus::Deleted => write!(f, "deleted"),
            ChangeStatus::Renamed(from) => write!(f, "renamed from {from}"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Hunk {
    pub header: String,
    pub added_lines: Vec<String>,
    pub removed_lines: Vec<String>,
}

#[derive(Debug)]
pub struct StagedDiff {
    pub files: Vec<FileChange>,
}

impl StagedDiff {
    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
    }

    pub fn summary(&self) -> String {
        let added = self
            .files
            .iter()
            .filter(|f| matches!(f.status, ChangeStatus::Added))
            .count();
        let modified = self
            .files
            .iter()
            .filter(|f| matches!(f.status, ChangeStatus::Modified))
            .count();
        let deleted = self
            .files
            .iter()
            .filter(|f| matches!(f.status, ChangeStatus::Deleted))
            .count();
        let renamed = self
            .files
            .iter()
            .filter(|f| matches!(f.status, ChangeStatus::Renamed(_)))
            .count();

        let mut parts = Vec::new();
        if added > 0 {
            parts.push(format!("{added} added"));
        }
        if modified > 0 {
            parts.push(format!("{modified} modified"));
        }
        if deleted > 0 {
            parts.push(format!("{deleted} deleted"));
        }
        if renamed > 0 {
            parts.push(format!("{renamed} renamed"));
        }
        parts.join(", ")
    }

    pub fn total_hunks(&self) -> usize {
        self.files.iter().map(|f| f.hunks.len()).sum()
    }
}

const IGNORED_EXTENSIONS: &[&str] = &[
    "lock", "sum", "min.js", "min.css", "map", "png", "jpg", "jpeg", "gif", "ico", "svg", "webp",
    "woff", "woff2", "ttf", "eot", "zip", "tar", "gz", "bz2", "exe", "dll", "so", "dylib", "pdf",
    "doc", "docx",
];

const IGNORED_FILES: &[&str] = &[
    "package-lock.json",
    "yarn.lock",
    "Cargo.lock",
    "pnpm-lock.yaml",
    "composer.lock",
    "Gemfile.lock",
    "poetry.lock",
    "Pipfile.lock",
];

const MAX_DIFF_CHARS: usize = 30_000;

fn should_ignore(path: &str) -> bool {
    let p = Path::new(path);

    if let Some(name) = p.file_name().and_then(|n| n.to_str()) {
        if IGNORED_FILES.contains(&name) {
            return true;
        }
    }

    if let Some(ext) = p.extension().and_then(|e| e.to_str()) {
        if IGNORED_EXTENSIONS.contains(&ext) {
            return true;
        }
    }

    false
}

pub fn get_staged_diff(repo_path: Option<&str>) -> Result<StagedDiff> {
    let repo = match repo_path {
        Some(p) => Repository::open(p).context("Failed to open git repository")?,
        None => Repository::discover(".").context("Not inside a git repository")?,
    };

    let head_tree = repo.head().ok().and_then(|h| h.peel_to_tree().ok());

    let mut opts = DiffOptions::new();
    opts.include_untracked(false);

    let diff = repo
        .diff_tree_to_index(head_tree.as_ref(), None, Some(&mut opts))
        .context("Failed to compute staged diff")?;

    let mut files: Vec<FileChange> = Vec::new();
    let mut total_chars = 0usize;

    diff.print(git2::DiffFormat::Patch, |delta, hunk, line| {
        let path = delta
            .new_file()
            .path()
            .or_else(|| delta.old_file().path())
            .and_then(|p| p.to_str())
            .unwrap_or("<unknown>");

        if should_ignore(path) {
            return true;
        }

        // Ensure we have a FileChange entry for this path
        if !files.iter().any(|f| f.path == path) {
            let status = match delta.status() {
                Delta::Added => ChangeStatus::Added,
                Delta::Deleted => ChangeStatus::Deleted,
                Delta::Modified => ChangeStatus::Modified,
                Delta::Renamed => {
                    let old = delta
                        .old_file()
                        .path()
                        .and_then(|p| p.to_str())
                        .unwrap_or("<unknown>")
                        .to_string();
                    ChangeStatus::Renamed(old)
                }
                _ => ChangeStatus::Modified,
            };
            files.push(FileChange {
                path: path.to_string(),
                status,
                hunks: Vec::new(),
            });
        }

        let file = files.iter_mut().find(|f| f.path == path).unwrap();

        match line.origin() {
            'H' => {
                // Hunk header
                if let Some(h) = hunk {
                    let header = String::from_utf8_lossy(h.header()).trim().to_string();
                    file.hunks.push(Hunk {
                        header,
                        added_lines: Vec::new(),
                        removed_lines: Vec::new(),
                    });
                }
            }
            '+' => {
                if total_chars > MAX_DIFF_CHARS {
                    return true;
                }
                let content = String::from_utf8_lossy(line.content()).to_string();
                total_chars += content.len();
                if let Some(hunk) = file.hunks.last_mut() {
                    hunk.added_lines.push(content);
                }
            }
            '-' => {
                if total_chars > MAX_DIFF_CHARS {
                    return true;
                }
                let content = String::from_utf8_lossy(line.content()).to_string();
                total_chars += content.len();
                if let Some(hunk) = file.hunks.last_mut() {
                    hunk.removed_lines.push(content);
                }
            }
            _ => {}
        }

        true
    })
    .context("Failed to iterate over diff")?;

    Ok(StagedDiff { files })
}

pub fn format_diff_for_prompt(diff: &StagedDiff) -> String {
    let mut output = String::new();

    for file in &diff.files {
        output.push_str(&format!("## {} ({})\n", file.path, file.status));

        for hunk in &file.hunks {
            if !hunk.header.is_empty() {
                output.push_str(&format!("### {}\n", hunk.header));
            }
            for line in &hunk.removed_lines {
                output.push_str(&format!("-{line}"));
                if !line.ends_with('\n') {
                    output.push('\n');
                }
            }
            for line in &hunk.added_lines {
                output.push_str(&format!("+{line}"));
                if !line.ends_with('\n') {
                    output.push('\n');
                }
            }
            output.push('\n');
        }
    }

    if output.len() > MAX_DIFF_CHARS {
        let mut end = MAX_DIFF_CHARS;
        while !output.is_char_boundary(end) {
            end -= 1;
        }
        output.truncate(end);
        output.push_str("\n\n[... diff truncated due to size limit ...]\n");
    }

    output
}

pub fn read_cursorrules() -> Option<String> {
    let paths = [".cursorrules", ".cursor/rules"];
    for p in &paths {
        if let Ok(content) = std::fs::read_to_string(p) {
            if !content.trim().is_empty() {
                return Some(content);
            }
        }
    }
    None
}
