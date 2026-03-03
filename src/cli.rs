use anyhow::Result;
use colored::Colorize;
use dialoguer::{theme::ColorfulTheme, Editor, Input, Select};
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

pub enum SplitAction {
    Proceed,
    Cancel,
}

pub fn display_commit_message(message: &str) {
    println!();
    println!("{}", "─".repeat(60).dimmed());
    println!("{}", "  Generated commit message:".bold());
    println!("{}", "─".repeat(60).dimmed());
    println!();
    print_message_body(message);
    println!();
    println!("{}", "─".repeat(60).dimmed());
}

fn print_message_body(message: &str) {
    for (i, line) in message.lines().enumerate() {
        if i == 0 {
            println!("  {}", line.green().bold());
        } else if line.starts_with("- ") || line.starts_with("* ") {
            println!("  {}", line.white());
        } else {
            println!("  {}", line.dimmed());
        }
    }
}

/// Interactive prompt for single-commit flow.
/// Returns the final message to commit, or None if cancelled.
pub fn prompt_commit_flow(initial_message: &str) -> Result<Option<String>> {
    let mut message = initial_message.to_string();

    loop {
        display_commit_message(&message);

        let items = vec![
            "✅ Commit    — use this message",
            "✏️  Edit      — open in editor",
            "📝 Inline    — quick-edit the subject line",
            "🔄 Regenerate — generate a new message",
            "❌ Cancel    — abort",
        ];

        let selection = Select::with_theme(&ColorfulTheme::default())
            .with_prompt("What would you like to do?")
            .items(&items)
            .default(0)
            .interact()?;

        match selection {
            0 => return Ok(Some(message)),
            1 => {
                if let Some(edited) = Editor::new().edit(&message)? {
                    let edited = edited.trim().to_string();
                    if edited.is_empty() {
                        print_warning("Commit message cannot be empty.");
                        continue;
                    }
                    message = edited;
                }
            }
            2 => {
                message = inline_edit_message(&message)?;
            }
            3 => return Ok(None), // signal regenerate
            4 => {
                print_info("Commit cancelled.");
                std::process::exit(0);
            }
            _ => unreachable!(),
        }
    }
}

/// Quick inline edit: edit subject line and body separately in the terminal.
fn inline_edit_message(message: &str) -> Result<String> {
    let lines: Vec<&str> = message.lines().collect();
    let subject = lines.first().copied().unwrap_or("");

    let new_subject: String = Input::with_theme(&ColorfulTheme::default())
        .with_prompt("Subject")
        .default(subject.to_string())
        .interact_text()?;

    // Body is everything after the first blank line
    let body_start = lines
        .iter()
        .position(|l| l.is_empty())
        .map(|i| i + 1)
        .unwrap_or(lines.len());

    let old_body = if body_start < lines.len() {
        lines[body_start..].join("\n")
    } else {
        String::new()
    };

    println!(
        "\n{}",
        "  Current body (press Enter to keep, or type new body):".dimmed()
    );
    for line in old_body.lines() {
        println!("  {}", line.dimmed());
    }

    let new_body: String = Input::with_theme(&ColorfulTheme::default())
        .with_prompt("Body (Enter to keep)")
        .default(old_body.clone())
        .allow_empty(true)
        .interact_text()?;

    let result = if new_body.trim().is_empty() {
        new_subject
    } else {
        format!("{}\n\n{}", new_subject, new_body)
    };

    Ok(result)
}

/// Interactive prompt for split-commit flow.
/// Allows editing individual commit messages before executing.
pub fn prompt_split_flow(groups: &mut Vec<crate::split::CommitGroup>) -> Result<SplitAction> {
    loop {
        crate::split::display_split_plan(groups);

        let mut items = vec!["✅ Proceed — execute all commits in order".to_string()];
        for (i, group) in groups.iter().enumerate() {
            let subject = group.message.lines().next().unwrap_or("(empty)");
            items.push(format!("✏️  Edit commit {} — {}", i + 1, subject));
        }
        items.push("❌ Cancel  — abort, keep all files staged".to_string());

        let selection = Select::with_theme(&ColorfulTheme::default())
            .with_prompt("What would you like to do?")
            .items(&items)
            .default(0)
            .interact()?;

        if selection == 0 {
            return Ok(SplitAction::Proceed);
        } else if selection == items.len() - 1 {
            return Ok(SplitAction::Cancel);
        } else {
            let idx = selection - 1;
            let edited = edit_single_commit_message(&groups[idx].message)?;
            groups[idx].message = edited;
            // Loop back to show updated plan
        }
    }
}

/// Edit a single commit message with full editor or inline.
fn edit_single_commit_message(message: &str) -> Result<String> {
    let items = vec![
        "✏️  Open in editor",
        "📝 Inline quick-edit",
        "↩️  Keep as-is",
    ];

    let selection = Select::with_theme(&ColorfulTheme::default())
        .with_prompt("How would you like to edit?")
        .items(&items)
        .default(0)
        .interact()?;

    match selection {
        0 => {
            let edited = Editor::new()
                .edit(message)?
                .unwrap_or_else(|| message.to_string());
            let edited = edited.trim().to_string();
            if edited.is_empty() {
                print_warning("Message cannot be empty, keeping original.");
                return Ok(message.to_string());
            }
            Ok(edited)
        }
        1 => inline_edit_message(message),
        _ => Ok(message.to_string()),
    }
}

pub fn print_success(message: &str) {
    println!("\n{}  {}", "✔".green().bold(), message.green());
}

pub fn print_error(message: &str) {
    eprintln!("\n{}  {}", "✖".red().bold(), message.red());
}

pub fn print_warning(message: &str) {
    println!("\n{}  {}", "⚠".yellow().bold(), message.yellow());
}

pub fn print_info(message: &str) {
    println!("\n{}  {}", "ℹ".cyan().bold(), message.cyan());
}
