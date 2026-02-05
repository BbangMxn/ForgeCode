//! ForgeCode init command
//!
//! Initializes a new project with .forgecode configuration.

use std::fs;
use std::path::Path;

/// Initialize ForgeCode configuration in the current directory
pub fn init_project(force: bool) -> anyhow::Result<()> {
    let forge_dir = Path::new(".forgecode");

    // Check if already initialized
    if forge_dir.exists() && !force {
        println!("✓ ForgeCode already initialized in this directory.");
        println!("  Use --force to reinitialize.");
        return Ok(());
    }

    println!("Initializing ForgeCode...");

    // Create directory structure
    fs::create_dir_all(forge_dir.join("skills"))?;

    // Create FORGE.md
    let forge_md = r#"# Project Instructions

<!-- Add project-specific instructions for ForgeCode here -->

## Build & Run

```bash
# Add your build commands
```

## Testing

```bash
# Add your test commands
```
"#;
    fs::write(forge_dir.join("FORGE.md"), forge_md)?;
    println!("  Created .forgecode/FORGE.md");

    // Create settings.json
    let settings_json = r#"{
  "$schema": "https://forgecode.dev/schema/settings.json",
  "version": "0.1.0",

  "provider": {
    "default": "anthropic",
    "anthropic": {
      "model": "claude-sonnet-4-20250514",
      "max_tokens": 8192
    }
  },

  "execution": {
    "default_mode": "local",
    "allow_local": true
  },

  "permissions": {
    "allow": [],
    "deny": [],
    "ask": ["Bash(*)", "Write(*)"]
  },

  "tools": {
    "disabled": []
  },

  "mcp": {
    "servers": {}
  }
}
"#;
    fs::write(forge_dir.join("settings.json"), settings_json)?;
    println!("  Created .forgecode/settings.json");

    // Create example skill
    let commit_skill = r#"# Commit Skill

Create a git commit with a well-formatted message.

## Usage

```
/commit [message]
```

## Behavior

1. Check git status for staged changes
2. Generate or use provided commit message
3. Create the commit
"#;
    fs::create_dir_all(forge_dir.join("skills/commit"))?;
    fs::write(forge_dir.join("skills/commit/SKILL.md"), commit_skill)?;
    println!("  Created .forgecode/skills/commit/SKILL.md");

    println!("\n✓ ForgeCode initialized successfully!");
    println!("\nNext steps:");
    println!("  1. Edit .forgecode/FORGE.md with your project instructions");
    println!("  2. Configure .forgecode/settings.json for your provider");
    println!("  3. Run 'forge' to start the assistant");

    Ok(())
}

/// Check if ForgeCode needs initialization and auto-init if appropriate
pub fn check_and_auto_init() -> bool {
    let forge_dirs = [".forgecode", ".forge", ".claude"];

    for dir in &forge_dirs {
        if Path::new(dir).exists() {
            return true; // Already initialized
        }
    }

    // Not initialized - check if this looks like a project directory
    let project_indicators = [
        "Cargo.toml",
        "package.json",
        "pyproject.toml",
        "go.mod",
        "pom.xml",
        "build.gradle",
        "Makefile",
        ".git",
    ];

    let is_project = project_indicators.iter().any(|f| Path::new(f).exists());

    if is_project {
        println!("📁 Project detected but ForgeCode not initialized.");
        println!("   Run 'forge init' to set up configuration.\n");
    }

    is_project
}
