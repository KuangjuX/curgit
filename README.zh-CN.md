# curgit

[![Language-English](https://img.shields.io/badge/Language-English-2f80ed?style=for-the-badge)](./README.md)
[![语言-中文](https://img.shields.io/badge/%E8%AF%AD%E8%A8%80-%E4%B8%AD%E6%96%87-e11d48?style=for-the-badge)](./README.zh-CN.md)

一个由 Rust 编写的高性能命令行工具，可作为独立的 **Git Agent** 使用。它会分析 Git 仓库中已暂存的变更，并使用 LLM 按照 [Conventional Commits](https://www.conventionalcommits.org/) 规范生成专业、具上下文感知的提交信息。

## 功能特性

- **Cursor 原生** — 默认通过本地 `cursor agent` 调用 Cursor Auto，零配置可用
- **智能 Diff 分析** — 提取已暂存变更、文件名、hunk 和函数签名
- **噪音过滤** — 自动排除 lock 文件、二进制文件等噪音内容
- **Conventional Commits** — 生成 `<type>(<scope>): <subject>` 格式及详细正文
- **语义自动拆分** — 按意图/功能将变更拆分为多个原子提交
- **多 Provider LLM** — 支持 Cursor、Ollama、OpenAI、Claude、Kimi、DeepSeek 及任意 OpenAI 兼容接口
- **多语言支持** — 支持英文和中文提交信息
- **交互式体验** — 加载动画、提交预览和 [Commit / Edit / Cancel] 提示
- **项目感知** — 读取 `.cursorrules` 以对齐项目约定
- **配置文件** — 通过 `~/.config/curgit/config.toml` 持久化配置
- **一键安装** — `install.sh` 可构建并安装到 `/usr/local/bin`

## 安装

### 一键安装

```bash
# 在项目目录执行
./install.sh

# 或安装到自定义目录
CURGIT_INSTALL_DIR=~/.local/bin ./install.sh
```

### 手动安装

```bash
cargo install --path .
```

或构建 release 二进制：

```bash
cargo build --release
# 二进制位于 ./target/release/curgit
```

## 快速开始

如果你已安装 Cursor，curgit 默认零配置可直接使用：

```bash
git add .
curgit
```

就是这么简单。curgit 默认使用 Cursor 内置的 LLM（通过 `cursor agent`）。

## 语义自动拆分

默认情况下，curgit 使用语义拆分模式，让 LLM 按意图/功能对已暂存变更分组（而不是仅按 diff 大小拆分）。

```bash
git add .
curgit            # 默认：语义拆分流程

curgit --split    # 显式启用语义拆分流程
curgit --no-split # 禁用拆分，仅生成单条提交信息
```

### 工作原理

1. curgit 解析已暂存 patch hunks，并分配稳定的 hunk ID（如 `H1`、`H2`）
2. 将完整 diff + hunk 清单发送给 LLM 进行拆分分析
3. LLM 按逻辑变更分组 hunks/文件，并为每组生成提交信息
4. 展示完整拆分计划供确认
5. 确认后按顺序执行每次提交：
   - 取消暂存全部文件
   - 对每组：将该组选择的 hunks 应用到 index（`git apply --cached`），必要时暂存整文件项
   - 使用生成的信息进行提交

### 输出示例

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

## LLM Provider

| Provider | 参数 | 默认模型 | API Key | 说明 |
|---|---|---|---|---|
| **Cursor**（默认） | `--provider cursor` | Cursor Auto | 否 | 使用本地 Cursor CLI（`cursor agent`） |
| **Ollama** | `--provider ollama` | `qwen2.5-coder:7b` | 否 | 通过 Ollama 本地推理 |
| **OpenAI** | `--provider openai` | `gpt-4o-mini` | 是 | OpenAI GPT 系列 |
| **Claude** | `--provider claude` | `claude-sonnet-4-20250514` | 是 | Anthropic Claude（原生 API） |
| **Kimi** | `--provider kimi` | `moonshot-v1-8k` | 是 | Moonshot AI Kimi |
| **DeepSeek** | `--provider deepseek` | `deepseek-chat` | 是 | DeepSeek AI |
| **Custom** | `--provider custom` | `gpt-4o-mini` | 否 | 任意 OpenAI 兼容端点 |

### 使用 Cursor（默认）

curgit 底层会调用 `cursor agent --trust`，把 diff 作为提示词输入并捕获生成的提交信息。它会使用 Cursor Auto 自动选择的模型，无需 API Key 或额外配置。

```bash
# 默认使用 Cursor Auto
curgit

# 显式指定
curgit --provider cursor
```

在 macOS 上会检测 `/Applications/Cursor.app/Contents/Resources/app/bin/cursor`；Linux 上 PATH 中的 `agent` 或 `cursor-agent`，以及 `cursor` 也会被识别。

### 使用 Ollama（本地离线）

```bash
# 安装 Ollama: https://ollama.ai
ollama pull qwen2.5-coder:7b

curgit --provider ollama
```

### 使用云端 Provider

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

## 配置

### 配置文件

创建 `~/.config/curgit/config.toml` 以持久化设置：

```toml
provider = "cursor"
# model = "gpt-4o-mini"
# api_key = "sk-..."
# api_base = "https://api.openai.com/v1"

# Provider 专属覆盖（推荐多 Provider 场景）
[providers.openai]
api_key = "sk-openai-..."
model = "gpt-4o-mini"

[providers.claude]
api_key = "sk-ant-..."
model = "claude-sonnet-4-20250514"
```

### 环境变量

| 变量 | 说明 |
|---|---|
| `CURGIT_PROVIDER` | 默认 Provider（`cursor`、`ollama`、`openai`、`claude`、`kimi`、`deepseek`、`custom`） |
| `CURGIT_<PROVIDER>_API_KEY` | Provider 专属 API Key，例如 `CURGIT_OPENAI_API_KEY` |
| `CURGIT_<PROVIDER>_API_BASE` | Provider 专属 API Base，例如 `CURGIT_CUSTOM_API_BASE` |
| `CURGIT_<PROVIDER>_MODEL` | Provider 专属模型，例如 `CURGIT_CLAUDE_MODEL` |
| `CURGIT_API_KEY` 或 `OPENAI_API_KEY` | 云端 Provider 的 API Key |
| `CURGIT_API_BASE` 或 `OPENAI_API_BASE` | 覆盖 API Base URL |
| `CURGIT_MODEL` | 覆盖模型名 |

### 优先级

配置解析顺序（从高到低）：

1. CLI 参数（`--provider`、`--model`、`--api-base`）
2. Provider 专属环境变量（如 `CURGIT_OPENAI_API_KEY`）
3. 全局环境变量（如 `CURGIT_API_KEY`）
4. 配置文件（`~/.config/curgit/config.toml`，含 `[providers.<name>]`）
5. Provider 默认值

## 使用方式

```bash
# 先暂存变更
git add .

# 生成提交信息（默认使用 Cursor Auto）
curgit

# 使用指定 Provider
curgit --provider openai

# 使用中文
curgit --lang zh

# 指定其他模型
curgit --provider openai --model gpt-4o

# 拆分控制
curgit --split
curgit --no-split

# Dry run（仅预览，不提交）
curgit --dry-run

# 显示当前配置
curgit --show-config
```

### 工作流

1. `curgit` 读取已暂存 diff（`git diff --cached`）
2. 过滤噪音（lock 文件、二进制等）
3. 默认进行语义拆分规划并按 hunk 粒度分组
4. 将 diff 发送给配置的 LLM（默认 Cursor Auto）
5. 展示生成的提交信息
6. 你可选择：**Commit**、**Edit**、**Cancel**（单次）/ **Proceed**、**Cancel**（拆分）

## 架构

```
src/
├── main.rs     # 入口、CLI 参数、流程编排
├── git.rs      # Git 观察层 —— diff 提取与过滤
├── prompt.rs   # Prompt 工程 —— system/user prompt 构建
├── llm.rs      # 多 Provider LLM 客户端（Cursor、Ollama、OpenAI、Claude、Kimi、DeepSeek）
├── split.rs    # 自动拆分引擎 —— LLM 驱动的提交拆分
└── cli.rs      # CLI 交互层 —— 动画、展示、交互提示
```

## License

MIT
