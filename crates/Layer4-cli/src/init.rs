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
///
/// Returns true if initialization exists or was successfully created.
/// If a project directory is detected without existing config, automatically initializes.
pub fn check_and_auto_init() -> bool {
    // Check for existing .forgecode directory only
    if Path::new(".forgecode").exists() {
        return true; // Already initialized
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
        "CMakeLists.txt",
        "setup.py",
        "composer.json",
        "Gemfile",
    ];

    let is_project = project_indicators.iter().any(|f| Path::new(f).exists());

    if is_project {
        println!("📁 Project detected - auto-initializing ForgeCode...\n");

        // Auto-initialize with default settings
        match auto_init_minimal() {
            Ok(_) => {
                println!("✓ Created .forgecode/ with default configuration");
                println!("  Edit .forgecode/FORGE.md to customize project instructions\n");
                return true;
            }
            Err(e) => {
                eprintln!("⚠ Auto-init failed: {}. Run 'forge init' manually.\n", e);
                return false;
            }
        }
    }

    // Not a project directory - still allow running
    false
}

/// Minimal auto-initialization (creates basic structure without prompts)
fn auto_init_minimal() -> anyhow::Result<()> {
    let forge_dir = Path::new(".forgecode");

    // Create directory structure
    fs::create_dir_all(forge_dir.join("skills"))?;

    // Create minimal FORGE.md (auto-detect project type)
    let project_type = detect_project_type();
    let forge_md = generate_forge_md(&project_type);
    fs::write(forge_dir.join("FORGE.md"), forge_md)?;

    // Create settings.json with sensible defaults
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

    Ok(())
}

/// Detect project type from files
fn detect_project_type() -> String {
    if Path::new("Cargo.toml").exists() {
        "rust".to_string()
    } else if Path::new("package.json").exists() {
        "javascript".to_string()
    } else if Path::new("pyproject.toml").exists() || Path::new("setup.py").exists() {
        "python".to_string()
    } else if Path::new("go.mod").exists() {
        "go".to_string()
    } else if Path::new("pom.xml").exists() || Path::new("build.gradle").exists() {
        "java".to_string()
    } else if Path::new("CMakeLists.txt").exists() {
        "cpp".to_string()
    } else if Path::new("composer.json").exists() {
        "php".to_string()
    } else if Path::new("Gemfile").exists() {
        "ruby".to_string()
    } else {
        "generic".to_string()
    }
}

/// Generate FORGE.md based on project type
fn generate_forge_md(project_type: &str) -> String {
    let (build_cmd, test_cmd) = match project_type {
        "rust" => ("cargo build", "cargo test"),
        "javascript" => ("npm install && npm run build", "npm test"),
        "python" => ("pip install -e .", "pytest"),
        "go" => ("go build ./...", "go test ./..."),
        "java" => ("mvn compile", "mvn test"),
        "cpp" => ("cmake --build build", "ctest --test-dir build"),
        "php" => ("composer install", "vendor/bin/phpunit"),
        "ruby" => ("bundle install", "bundle exec rspec"),
        _ => ("# Add build commands", "# Add test commands"),
    };

    format!(r#"# Project Instructions

<!-- ForgeCode project configuration - auto-generated -->
<!-- Edit this file to customize AI assistant behavior -->

## Build & Run

```bash
{}
```

## Testing

```bash
{}
```

## Code Style

- Follow existing code conventions
- Add comments for complex logic
- Write tests for new features

## Project Structure

<!-- Describe important directories and files here -->
"#, build_cmd, test_cmd)
}
