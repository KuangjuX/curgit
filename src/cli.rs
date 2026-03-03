use anyhow::Result;
use colored::Colorize;
use dialoguer::{theme::ColorfulTheme, Editor, Select};
use indicatif::{ProgressBar, ProgressStyle};
use std::time::Duration;

pub fn create_spinner(msg: &str) -> ProgressBar {
    let pb = ProgressBar::new_spinner();
    pb.set_style(
        ProgressStyle::with_template("{spinner:.cyan} {msg}")
            .unwrap()
            .tick_strings(&["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"]),
    );
    pb.set_message(msg.to_string());
    pb.enable_steady_tick(Duration::from_millis(80));
    pb
}

pub enum UserAction {
    Commit,
    Edit(String),
    Cancel,
}

pub enum SplitAction {
    Proceed,
    Cancel,
}

pub fn prompt_split_action() -> Result<SplitAction> {
    let items = vec![
        "✅ Proceed — execute all commits in order",
        "❌ Cancel  — abort, keep all files staged",
    ];

    let selection = Select::with_theme(&ColorfulTheme::default())
        .with_prompt("Execute this split plan?")
        .items(&items)
        .default(0)
        .interact()?;

    match selection {
        0 => Ok(SplitAction::Proceed),
        _ => Ok(SplitAction::Cancel),
    }
}

pub fn display_commit_message(message: &str) {
    println!();
    println!("{}", "─".repeat(60).dimmed());
    println!("{}", "  Generated commit message:".bold());
    println!("{}", "─".repeat(60).dimmed());
    println!();

    for (i, line) in message.lines().enumerate() {
        if i == 0 {
            println!("  {}", line.green().bold());
        } else if line.starts_with("- ") || line.starts_with("* ") {
            println!("  {}", line.white());
        } else {
            println!("  {}", line.dimmed());
        }
    }

    println!();
    println!("{}", "─".repeat(60).dimmed());
}

pub fn prompt_user_action(message: &str) -> Result<UserAction> {
    let items = vec![
        "✅ Commit — use this message",
        "✏️  Edit  — modify before committing",
        "❌ Cancel — abort",
    ];

    let selection = Select::with_theme(&ColorfulTheme::default())
        .with_prompt("What would you like to do?")
        .items(&items)
        .default(0)
        .interact()?;

    match selection {
        0 => Ok(UserAction::Commit),
        1 => {
            let edited = Editor::new()
                .edit(message)?
                .unwrap_or_else(|| message.to_string());
            Ok(UserAction::Edit(edited))
        }
        _ => Ok(UserAction::Cancel),
    }
}

pub fn print_success(message: &str) {
    println!(
        "\n{}  {}",
        "✔".green().bold(),
        message.green()
    );
}

pub fn print_error(message: &str) {
    eprintln!(
        "\n{}  {}",
        "✖".red().bold(),
        message.red()
    );
}

pub fn print_warning(message: &str) {
    println!(
        "\n{}  {}",
        "⚠".yellow().bold(),
        message.yellow()
    );
}

pub fn print_info(message: &str) {
    println!(
        "\n{}  {}",
        "ℹ".cyan().bold(),
        message.cyan()
    );
}
