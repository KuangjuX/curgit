mod cli;
mod git;
mod llm;
mod prompt;
mod split;

use anyhow::Result;
use clap::Parser;
use std::process::Command;

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
    #[arg(long)]
    split: bool,

    /// Disable semantic auto-split; generate a single commit message
    #[arg(long)]
    no_split: bool,

    /// Show current config and exit
    #[arg(long)]
    show_config: bool,
}

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

    let use_split = args.split || !args.no_split;

    if use_split {
        return run_split_flow(&args, &config, &diff, &formatted_diff).await;
    }

    // Normal single-commit flow with regeneration support
    let system_prompt = prompt::build_system_prompt(language, cursorrules.as_deref());
    let user_prompt = prompt::build_user_prompt(&diff, &formatted_diff);

    loop {
        let spinner = cli::create_spinner("Generating commit message...");
        let message = llm::generate_commit_message(&config, &system_prompt, &user_prompt).await?;
        spinner.finish_and_clear();

        if args.dry_run {
            cli::display_commit_message(&message);
            cli::print_info("Dry run — no commit created.");
            return Ok(());
        }

        // prompt_commit_flow returns Some(msg) to commit, None to regenerate
        match cli::prompt_commit_flow(&message)? {
            Some(final_message) => {
                let status = Command::new("git")
                    .args(["commit", "-m", &final_message])
                    .status()?;

                if status.success() {
                    cli::print_success("Commit created successfully!");
                } else {
                    cli::print_error("git commit failed.");
                    std::process::exit(1);
                }
                return Ok(());
            }
            None => {
                cli::print_info("Regenerating commit message...");
                continue;
            }
        }
    }
}

async fn run_split_flow(
    args: &Args,
    config: &llm::LlmConfig,
    diff: &git::StagedDiff,
    formatted_diff: &str,
) -> Result<()> {
    let staged_patch = split::parse_staged_patch()?;
    cli::print_info(&format!(
        "Generating semantic split plan from {} files / {} hunks...",
        diff.files.len(),
        staged_patch.hunks.len()
    ));

    let spinner = cli::create_spinner("Analyzing diff and generating split plan...");
    let mut groups =
        split::generate_split_plan(config, diff, formatted_diff, &staged_patch, &args.lang).await?;
    spinner.finish_and_clear();

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
            split::execute_split_plan(&groups, &staged_patch)?;
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
    Ok(())
}
