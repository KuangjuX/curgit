mod cli;
mod git;
mod llm;
mod prompt;
mod session;
mod split;

use anyhow::Result;
use clap::{Parser, ValueEnum};

#[derive(Parser)]
#[command(
    name = "curgit",
    version,
    about = "AI-powered Git commit message generator"
)]
struct Args {
    /// Language for the commit message (en, zh)
    #[arg(short, long, default_value = "en")]
    lang: String,

    /// LLM provider: cursor, ollama, openai, claude, kimi, deepseek, custom
    #[arg(short, long)]
    provider: Option<String>,

    /// LLM model to use (overrides provider default)
    #[arg(short, long)]
    model: Option<String>,

    /// API base URL (overrides provider default)
    #[arg(long)]
    api_base: Option<String>,

    /// Dry run — generate message without committing
    #[arg(long)]
    dry_run: bool,

    /// Force semantic auto-split mode (prompt-based)
    #[arg(long, conflicts_with = "no_split")]
    split: bool,

    /// Disable semantic auto-split; generate a single commit message
    #[arg(long, conflicts_with = "split")]
    no_split: bool,

    /// Split strategy: auto (default), always, never
    #[arg(long, value_enum)]
    split_mode: Option<SplitModeArg>,

    /// Auto-split threshold: changed file count
    #[arg(long)]
    split_files_threshold: Option<usize>,

    /// Auto-split threshold: hunk count
    #[arg(long)]
    split_hunks_threshold: Option<usize>,

    /// Auto-split threshold: formatted diff size (chars)
    #[arg(long)]
    split_chars_threshold: Option<usize>,

    /// Show current config and exit
    #[arg(long)]
    show_config: bool,

    /// Force start a new Cursor Agent session (discard the cached one)
    #[arg(long)]
    new_session: bool,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum SplitModeArg {
    Auto,
    Always,
    Never,
}

#[derive(Clone, Copy, Debug)]
enum SplitMode {
    Auto,
    Always,
    Never,
}

#[derive(Clone, Copy, Debug)]
struct SplitThresholds {
    files: usize,
    hunks: usize,
    chars: usize,
}

const DEFAULT_SPLIT_FILES_THRESHOLD: usize = 8;
const DEFAULT_SPLIT_HUNKS_THRESHOLD: usize = 20;
const DEFAULT_SPLIT_CHARS_THRESHOLD: usize = 20_000;

#[tokio::main]
async fn main() {
    if let Err(e) = run().await {
        cli::print_error(&format!("{e:#}"));
        std::process::exit(1);
    }
}

async fn run() -> Result<()> {
    let args = Args::parse();

    if args.show_config {
        return show_config(&args);
    }

    let file_config = llm::ConfigFile::load()?;
    let author = llm::AuthorConfig::resolve(&file_config);

    let diff = git::get_staged_diff(None)?;

    if diff.is_empty() {
        cli::print_warning("No staged changes found. Use `git add` to stage files first.");
        return Ok(());
    }

    cli::print_info(&format!("Analyzing staged changes: {}", diff.summary()));

    let formatted_diff = git::format_diff_for_prompt(&diff);
    let cursorrules = git::read_cursorrules();
    let language = prompt::Language::from_str(&args.lang);

    let config = llm::LlmConfig::resolve(
        args.provider.as_deref(),
        args.model.as_deref(),
        args.api_base.as_deref(),
    )?;

    cli::print_info(&format!(
        "Using {} (model: {})",
        config.provider, config.model
    ));

    let split_mode = resolve_split_mode(&args);
    let split_thresholds = resolve_split_thresholds(&args);
    let use_split = should_use_split(split_mode, &diff, &formatted_diff, split_thresholds);

    if matches!(split_mode, SplitMode::Auto) {
        cli::print_info(&format!(
            "Split mode: auto (thresholds: {} files / {} hunks / {} chars)",
            split_thresholds.files, split_thresholds.hunks, split_thresholds.chars
        ));
    }

    let mut sess = session::SessionState::load();
    if args.new_session {
        sess.invalidate();
    }

    if use_split {
        let result =
            run_split_flow(&args, &config, &diff, &formatted_diff, &author, &mut sess).await;
        let _ = sess.save();
        return result;
    }

    let system_prompt = prompt::build_system_prompt(language, cursorrules.as_deref());
    let user_prompt = prompt::build_user_prompt(&diff, &formatted_diff);

    loop {
        let spinner = cli::create_spinner("Generating commit message...");
        let active_sid = sess.is_valid().then(|| sess.session_id.clone()).flatten();
        let (message, json_resp) = llm::generate_with_session(
            &config,
            &system_prompt,
            &user_prompt,
            active_sid.as_deref(),
        )
        .await?;
        spinner.finish_and_clear();

        update_session(&mut sess, &json_resp);

        if args.dry_run {
            cli::display_commit_message(&message);
            cli::print_info("Dry run — no commit created.");
            let _ = sess.save();
            return Ok(());
        }

        match cli::prompt_commit_flow(&message)? {
            Some(final_message) => {
                let status = git::git_with_author(&author)
                    .args(["commit", "-m", &final_message])
                    .status()?;

                if status.success() {
                    cli::print_success("Commit created successfully!");
                } else {
                    cli::print_error("git commit failed.");
                    std::process::exit(1);
                }
                let _ = sess.save();
                return Ok(());
            }
            None => {
                cli::print_info("Regenerating commit message...");
                continue;
            }
        }
    }
}

fn update_session(
    sess: &mut session::SessionState,
    json_resp: &Option<session::CursorJsonResponse>,
) {
    if let Some(resp) = json_resp {
        if let (Some(sid), Some(usage)) = (&resp.session_id, &resp.usage) {
            sess.update_from_response(sid, usage);
            if !sess.is_valid() {
                sess.invalidate();
            }
        }
    }
}

async fn run_split_flow(
    args: &Args,
    config: &llm::LlmConfig,
    diff: &git::StagedDiff,
    formatted_diff: &str,
    author: &llm::AuthorConfig,
    sess: &mut session::SessionState,
) -> Result<()> {
    let staged_patch = split::parse_staged_patch()?;
    cli::print_info(&format!(
        "Generating semantic split plan from {} files / {} hunks...",
        diff.files.len(),
        staged_patch.hunks.len()
    ));

    let spinner = cli::create_spinner("Analyzing diff and generating split plan...");
    let active_sid = sess.is_valid().then(|| sess.session_id.clone()).flatten();
    let (mut groups, json_resp) = split::generate_split_plan_with_session(
        config,
        diff,
        formatted_diff,
        &staged_patch,
        &args.lang,
        active_sid.as_deref(),
    )
    .await?;
    spinner.finish_and_clear();

    update_session(sess, &json_resp);

    let warnings = split::validate_split_plan(&groups, diff, &staged_patch);
    for w in &warnings {
        cli::print_warning(w);
    }

    if args.dry_run {
        split::display_split_plan(&groups);
        cli::print_info("Dry run — no commits created.");
        return Ok(());
    }

    match cli::prompt_split_flow(&mut groups)? {
        cli::SplitAction::Proceed => {
            split::execute_split_plan(&groups, &staged_patch, author)?;
            cli::print_success(&format!(
                "All {} commits created successfully!",
                groups.len()
            ));
        }
        cli::SplitAction::Cancel => {
            cli::print_info("Split cancelled. All files remain staged.");
        }
    }

    Ok(())
}

fn show_config(args: &Args) -> Result<()> {
    let config = llm::LlmConfig::resolve(
        args.provider.as_deref(),
        args.model.as_deref(),
        args.api_base.as_deref(),
    )?;
    let file_config = llm::ConfigFile::load()?;
    let author = llm::AuthorConfig::resolve(&file_config);

    println!("curgit configuration:");
    println!("  provider:  {}", config.provider);
    println!("  model:     {}", config.model);
    println!("  api_base:  {}", config.api_base);
    println!(
        "  api_key:   {}",
        config
            .api_key
            .as_ref()
            .map(|k| format!(
                "{}...{}",
                &k[..4.min(k.len())],
                &k[k.len().saturating_sub(4)..]
            ))
            .unwrap_or_else(|| "(not set)".to_string())
    );
    if let Some(path) = llm::LlmConfig::config_file_path() {
        println!("  config:    {}", path.display());
    }
    let split_mode = resolve_split_mode(args);
    let split_thresholds = resolve_split_thresholds(args);
    let split_mode_str = match split_mode {
        SplitMode::Auto => "auto",
        SplitMode::Always => "always",
        SplitMode::Never => "never",
    };
    println!("  split:     {}", split_mode_str);
    println!(
        "  thresholds: files={} hunks={} chars={}",
        split_thresholds.files, split_thresholds.hunks, split_thresholds.chars
    );
    if author.has_any() {
        println!(
            "  author:    name={} email={}",
            author
                .name
                .as_deref()
                .unwrap_or("(git default)"),
            author
                .email
                .as_deref()
                .unwrap_or("(git default)")
        );
    } else {
        println!("  author:    (use Git user.name / user.email)");
    }

    let sess = session::SessionState::load();
    if let Some(sid) = &sess.session_id {
        println!("  session:   {}…{}", &sid[..8.min(sid.len())], &sid[sid.len().saturating_sub(4)..]);
        println!("  turns:     {}", sess.turn_count);
        println!("  ctx_tokens: {}", sess.context_tokens);
        println!("  valid:     {}", sess.is_valid());
    } else {
        println!("  session:   (none)");
    }

    Ok(())
}

fn resolve_split_mode(args: &Args) -> SplitMode {
    if args.split {
        return SplitMode::Always;
    }
    if args.no_split {
        return SplitMode::Never;
    }
    match args.split_mode.unwrap_or(SplitModeArg::Auto) {
        SplitModeArg::Auto => SplitMode::Auto,
        SplitModeArg::Always => SplitMode::Always,
        SplitModeArg::Never => SplitMode::Never,
    }
}

fn resolve_split_thresholds(args: &Args) -> SplitThresholds {
    SplitThresholds {
        files: args
            .split_files_threshold
            .or_else(|| parse_env_usize("CURGIT_SPLIT_FILES_THRESHOLD"))
            .unwrap_or(DEFAULT_SPLIT_FILES_THRESHOLD),
        hunks: args
            .split_hunks_threshold
            .or_else(|| parse_env_usize("CURGIT_SPLIT_HUNKS_THRESHOLD"))
            .unwrap_or(DEFAULT_SPLIT_HUNKS_THRESHOLD),
        chars: args
            .split_chars_threshold
            .or_else(|| parse_env_usize("CURGIT_SPLIT_CHARS_THRESHOLD"))
            .unwrap_or(DEFAULT_SPLIT_CHARS_THRESHOLD),
    }
}

fn parse_env_usize(key: &str) -> Option<usize> {
    std::env::var(key)
        .ok()
        .and_then(|v| v.trim().parse::<usize>().ok())
}

fn should_use_split(
    split_mode: SplitMode,
    diff: &git::StagedDiff,
    formatted_diff: &str,
    thresholds: SplitThresholds,
) -> bool {
    match split_mode {
        SplitMode::Always => true,
        SplitMode::Never => false,
        SplitMode::Auto => {
            let file_count = diff.files.len();
            let hunk_count = diff.total_hunks();
            let diff_chars = formatted_diff.len();
            file_count >= thresholds.files
                || hunk_count >= thresholds.hunks
                || diff_chars >= thresholds.chars
        }
    }
}
