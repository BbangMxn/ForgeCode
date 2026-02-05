# ForgeCode 🔧

AI-powered coding assistant for the terminal. Like Claude Code, but open source.

## Features

- 🤖 Multiple LLM providers (Ollama, Anthropic, OpenAI, Gemini)
- 🔒 Permission system for safe command execution
- 📁 File read/write/edit tools
- 🔍 Glob and grep search
- 🖥️ Task management for long-running processes
- 🎨 Beautiful TUI interface

## Installation

### One-line Install

**Windows (PowerShell):**
```powershell
irm https://raw.githubusercontent.com/BbangMxn/ForgeCode/main/install.ps1 | iex
```

**Linux/macOS:**
```bash
curl -fsSL https://raw.githubusercontent.com/BbangMxn/ForgeCode/main/install.sh | bash
```

### From Source (Cargo)

```bash
cargo install --git https://github.com/BbangMxn/ForgeCode
```

### Manual Build

```bash
git clone https://github.com/BbangMxn/ForgeCode
cd ForgeCode
cargo build --release
# Binary at: target/release/forge
```

## Quick Start

```bash
# First run - setup wizard will guide you
forge

# Non-interactive mode
forge --prompt "Read Cargo.toml and explain the project structure"

# Specify provider
forge --provider ollama --model qwen3:8b
```

## Configuration

ForgeCode stores configuration in `.forgecode/settings.json`:

```json
{
  "provider": {
    "default": "ollama",
    "ollama": {
      "base_url": "http://localhost:11434",
      "model": "qwen3:8b"
    }
  },
  "permissions": {
    "allow": ["Bash(ls:*)", "Bash(cat:*)"],
    "deny": ["Bash(rm -rf /)"],
    "ask": ["Bash(*)", "Write(*)"]
  }
}
```

## Architecture

```
Layer4-cli      → TUI, CLI interface
Layer3-agent    → Agent loop, memory, feedback
Layer2-core     → Tools (read, write, bash, glob, grep)
Layer2-provider → LLM gateway (Anthropic, OpenAI, Ollama)
Layer2-task     → Task/PTY management
Layer1-foundation → Permissions, caching, utilities
```

## License

MIT License
