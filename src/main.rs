mod cli;
mod git;
mod llm;
mod prompt;

use anyhow::Result;
use clap::Parser;
use std::process::Command;

#[derive(Parser)]
#[command(name = "curgit", version, about = "AI-powered Git commit message generator")]
struct Args {
    /// Language for the commit message (en, zh)
    #[arg(short, long, default_value = "en")]
    lang: String,

    /// LLM provider: ollama, openai, claude, kimi, deepseek, custom
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

    let system_prompt = prompt::build_system_prompt(language, cursorrules.as_deref());
    let user_prompt = prompt::build_user_prompt(&diff, &formatted_diff);

    let config = llm::LlmConfig::resolve(
        args.provider.as_deref(),
        args.model.as_deref(),
        args.api_base.as_deref(),
    )?;

    cli::print_info(&format!(
        "Using {} (model: {})",
        config.provider, config.model
    ));

    let spinner = cli::create_spinner("Generating commit message...");
    let message = llm::generate_commit_message(&config, &system_prompt, &user_prompt).await?;
    spinner.finish_and_clear();

    cli::display_commit_message(&message);

    if args.dry_run {
        cli::print_info("Dry run — no commit created.");
        return Ok(());
    }

    let final_message = loop {
        match cli::prompt_user_action(&message)? {
            cli::UserAction::Commit => break message.clone(),
            cli::UserAction::Edit(edited) => {
                if edited.trim().is_empty() {
                    cli::print_warning("Commit message cannot be empty.");
                    continue;
                }
                break edited;
            }
            cli::UserAction::Cancel => {
                cli::print_info("Commit cancelled.");
                return Ok(());
            }
        }
    };

    let status = Command::new("git")
        .args(["commit", "-m", &final_message])
        .status()?;

    if status.success() {
        cli::print_success("Commit created successfully!");
    } else {
        cli::print_error("git commit failed.");
        std::process::exit(1);
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
            .map(|k| format!("{}...{}", &k[..4.min(k.len())], &k[k.len().saturating_sub(4)..]))
            .unwrap_or_else(|| "(not set)".to_string())
    );
    if let Some(path) = llm::LlmConfig::config_file_path() {
        println!("  config:    {}", path.display());
    }
    Ok(())
}
