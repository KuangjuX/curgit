# curgit

A high-performance CLI tool written in Rust that acts as a standalone **Git Agent**. It analyzes staged changes in a git repository and generates professional, context-aware commit messages following the [Conventional Commits](https://www.conventionalcommits.org/) standard using LLM.

## Features

- **Cursor-Native** — Defaults to local Cursor Auto via `cursor agent`, zero config needed
- **Smart Diff Analysis** — Extracts staged changes, file names, hunks, and function signatures
- **Noise Filtering** — Automatically excludes lock files, binary files, and other noise
- **Conventional Commits** — Generates messages in `<type>(<scope>): <subject>` format with detailed body
- **Semantic Auto-Split** — Splits staged changes by intent/functionality into multiple atomic commits
- **Multi-Provider LLM** — Supports Cursor, Ollama, OpenAI, Claude, Kimi, DeepSeek, and any OpenAI-compatible API
- **Multi-Language** — Supports English and Chinese commit messages
- **Interactive UX** — Loading spinner, commit preview, and [Commit / Edit / Cancel] prompt
- **Project-Aware** — Reads `.cursorrules` to align with local project conventions
- **Config File** — Persistent configuration via `~/.config/curgit/config.toml`
- **One-Click Install** — `install.sh` builds and installs to `/usr/local/bin`

## Installation

### One-Click Install

```bash
# From the project directory
./install.sh

# Or install to a custom location
CURGIT_INSTALL_DIR=~/.local/bin ./install.sh
```

### Manual Install

```bash
cargo install --path .
```

Or build a release binary:

```bash
cargo build --release
# Binary at ./target/release/curgit
```

## Quick Start

If you have Cursor installed, curgit works out of the box with zero configuration:

```bash
git add .
curgit
```

That's it. curgit will use Cursor's built-in LLM via `cursor agent` by default.

## Semantic Auto-Split

By default, curgit uses semantic split mode and asks the LLM to group staged changes by intent/functionality (instead of splitting by diff size only).

```bash
git add .
curgit            # default: semantic split flow

curgit --split    # explicitly force semantic split flow
curgit --no-split # disable split and generate a single commit message
```

### How It Works

1. curgit parses staged patch hunks and assigns stable hunk IDs (e.g. `H1`, `H2`)
2. Sends full diff + hunk inventory to the LLM with a split-analysis prompt
3. LLM groups hunks/files by logical change and generates a commit message per group
4. Displays the full split plan for review
5. On confirmation, executes each commit in order:
   - Unstages all files
   - For each group: applies the group's selected hunks to index (`git apply --cached`) and stages whole-file items when needed
   - Commits with the generated message

### Example Output

```
  Split plan: 3 commits

  Commit 1/3
  feat(auth): add OAuth2 login support
  - Implement Google OAuth2 flow with PKCE
  - Add token refresh middleware
  Hunks: (2)
    • H1
    • H2
  Files:
    • src/auth.rs
    • src/middleware.rs

  Commit 2/3
  refactor(db): migrate to connection pooling
  - Replace single connection with r2d2 pool
  - Add health check endpoint
  Files:
    • src/db.rs
    • src/health.rs

  Commit 3/3
  docs: update README with new setup instructions
  - Add OAuth2 configuration section
  - Document database pool settings
  Files:
    • README.md
```

## LLM Providers

| Provider | Flag | Default Model | API Key | Description |
|---|---|---|---|---|
| **Cursor** (default) | `--provider cursor` | Cursor Auto | No | Uses local Cursor CLI (`cursor agent`) |
| **Ollama** | `--provider ollama` | `qwen2.5-coder:7b` | No | Local inference via Ollama |
| **OpenAI** | `--provider openai` | `gpt-4o-mini` | Yes | OpenAI GPT series |
| **Claude** | `--provider claude` | `claude-sonnet-4-20250514` | Yes | Anthropic Claude (native API) |
| **Kimi** | `--provider kimi` | `moonshot-v1-8k` | Yes | Moonshot AI Kimi |
| **DeepSeek** | `--provider deepseek` | `deepseek-chat` | Yes | DeepSeek AI |
| **Custom** | `--provider custom` | `gpt-4o-mini` | No | Any OpenAI-compatible endpoint |

### Using Cursor (Default)

curgit calls `cursor agent --trust` under the hood, piping the diff as a prompt and capturing the generated commit message. This uses whatever model Cursor Auto selects — no API key, no extra setup.

```bash
# Uses Cursor Auto by default
curgit

# Explicitly specify
curgit --provider cursor
```

Cursor CLI is detected at `/Applications/Cursor.app/Contents/Resources/app/bin/cursor` (macOS) or via `cursor` in PATH.

### Using Ollama (Local, Offline)

```bash
# Install Ollama: https://ollama.ai
ollama pull qwen2.5-coder:7b

curgit --provider ollama
```

### Using Cloud Providers

```bash
# OpenAI
export CURGIT_OPENAI_API_KEY="sk-..."
export CURGIT_OPENAI_MODEL="gpt-4o-mini"
curgit --provider openai

# Claude
export CURGIT_CLAUDE_API_KEY="sk-ant-..."
export CURGIT_CLAUDE_MODEL="claude-sonnet-4-20250514"
curgit --provider claude

# Kimi
export CURGIT_KIMI_API_KEY="sk-..."
curgit --provider kimi

# DeepSeek
export CURGIT_DEEPSEEK_API_KEY="sk-..."
curgit --provider deepseek
```

## Configuration

### Config File

Create `~/.config/curgit/config.toml` for persistent settings:

```toml
provider = "cursor"
# model = "gpt-4o-mini"
# api_key = "sk-..."
# api_base = "https://api.openai.com/v1"

# Provider-specific overrides (recommended when using multiple providers)
[providers.openai]
api_key = "sk-openai-..."
model = "gpt-4o-mini"

[providers.claude]
api_key = "sk-ant-..."
model = "claude-sonnet-4-20250514"
```

### Environment Variables

| Variable | Description |
|---|---|
| `CURGIT_PROVIDER` | Default provider (`cursor`, `ollama`, `openai`, `claude`, `kimi`, `deepseek`, `custom`) |
| `CURGIT_<PROVIDER>_API_KEY` | Provider-specific API key, e.g. `CURGIT_OPENAI_API_KEY` |
| `CURGIT_<PROVIDER>_API_BASE` | Provider-specific API base URL, e.g. `CURGIT_CUSTOM_API_BASE` |
| `CURGIT_<PROVIDER>_MODEL` | Provider-specific model, e.g. `CURGIT_CLAUDE_MODEL` |
| `CURGIT_API_KEY` or `OPENAI_API_KEY` | API key for cloud providers |
| `CURGIT_API_BASE` or `OPENAI_API_BASE` | Override API base URL |
| `CURGIT_MODEL` | Override model name |

### Priority

Configuration is resolved in this order (highest to lowest):

1. CLI arguments (`--provider`, `--model`, `--api-base`)
2. Provider-specific environment variables (`CURGIT_OPENAI_API_KEY`, etc.)
3. Global environment variables (`CURGIT_API_KEY`, etc.)
4. Config file (`~/.config/curgit/config.toml`, including `[providers.<name>]`)
5. Provider defaults

## Usage

```bash
# Stage your changes first
git add .

# Generate a commit message (uses Cursor Auto by default)
curgit

# Use a specific provider
curgit --provider openai

# Use Chinese language
curgit --lang zh

# Specify a different model
curgit --provider openai --model gpt-4o

# Split controls
curgit --split
curgit --no-split

# Dry run (preview only, no commit)
curgit --dry-run

# Show current configuration
curgit --show-config
```

### Workflow

1. `curgit` reads your staged diff (`git diff --cached`)
2. Filters out noise (lock files, binaries, etc.)
3. By default, performs semantic split planning with hunk-level grouping
4. Sends the diff to the configured LLM (Cursor Auto by default)
5. Displays the generated commit message(s)
6. You choose: **Commit**, **Edit**, or **Cancel** (single) / **Proceed** or **Cancel** (split)

## Architecture

```
src/
├── main.rs     # Entry point, CLI args, orchestration
├── git.rs      # Git observer — diff extraction and filtering
├── prompt.rs   # Prompt engineering — system/user prompt construction
├── llm.rs      # Multi-provider LLM client (Cursor, Ollama, OpenAI, Claude, Kimi, DeepSeek)
├── split.rs    # Auto-split engine — LLM-driven commit splitting
└── cli.rs      # CLI UX — spinner, display, interactive prompts
```

## License

MIT
