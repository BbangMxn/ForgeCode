//! Auto-commit and Commit Message Generation
//!
//! Provides automatic commit functionality and LLM-based commit message generation.
//! Inspired by Aider's auto-commit feature.

use super::ops::{GitError, GitOps};
use serde::{Deserialize, Serialize};
use std::path::Path;
use tracing::{debug, info};

// ============================================================================
// Configuration
// ============================================================================

/// Commit message style
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum CommitStyle {
    /// Conventional commits (feat:, fix:, etc.)
    #[default]
    Conventional,

    /// Simple descriptive message
    Simple,

    /// Include AI attribution
    WithAttribution,

    /// Custom prefix
    Custom,
}

/// Auto-commit configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutoCommitConfig {
    /// Enable auto-commit
    pub enabled: bool,

    /// Commit style
    pub style: CommitStyle,

    /// Custom prefix (if style is Custom)
    pub custom_prefix: Option<String>,

    /// Include "(aider)" or "(forge)" attribution
    pub include_attribution: bool,

    /// Attribution text
    pub attribution: String,

    /// Auto-commit dirty files before AI changes
    pub commit_dirty_first: bool,

    /// Commit message for dirty files
    pub dirty_commit_message: String,

    /// Maximum commit message length
    pub max_message_length: usize,
}

impl Default for AutoCommitConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            style: CommitStyle::Conventional,
            custom_prefix: None,
            include_attribution: true,
            attribution: "(forge)".to_string(),
            commit_dirty_first: true,
            dirty_commit_message: "WIP: uncommitted changes before AI edit".to_string(),
            max_message_length: 72,
        }
    }
}

impl AutoCommitConfig {
    /// Create config with auto-commit disabled
    pub fn disabled() -> Self {
        Self {
            enabled: false,
            ..Default::default()
        }
    }

    /// Create Aider-style config
    pub fn aider_style() -> Self {
        Self {
            enabled: true,
            style: CommitStyle::Conventional,
            include_attribution: true,
            attribution: "(aider)".to_string(),
            ..Default::default()
        }
    }
}

// ============================================================================
// Commit Generator
// ============================================================================

/// Generates commit messages from diffs
pub struct CommitGenerator {
    /// Configuration
    config: AutoCommitConfig,

    /// Git operations
    git: GitOps,
}

impl CommitGenerator {
    /// Create a new commit generator
    pub fn new(path: impl AsRef<Path>, config: AutoCommitConfig) -> Result<Self, GitError> {
        let git = GitOps::new(path)?;
        Ok(Self { config, git })
    }

    /// Create with default config
    pub fn default_config(path: impl AsRef<Path>) -> Result<Self, GitError> {
        Self::new(path, AutoCommitConfig::default())
    }

    /// Generate commit message from diff
    pub fn generate_message(&self, diff: &str, context: Option<&str>) -> String {
        // Analyze diff to determine type
        let commit_type = self.analyze_diff_type(diff);
        let summary = self.summarize_changes(diff);

        let mut message = match self.config.style {
            CommitStyle::Conventional => {
                format!("{}: {}", commit_type, summary)
            }
            CommitStyle::Simple => summary,
            CommitStyle::WithAttribution => {
                format!("{} {}", summary, self.config.attribution)
            }
            CommitStyle::Custom => {
                if let Some(prefix) = &self.config.custom_prefix {
                    format!("{} {}", prefix, summary)
                } else {
                    summary
                }
            }
        };

        // Add attribution if configured
        if self.config.include_attribution && self.config.style != CommitStyle::WithAttribution {
            message = format!("{} {}", message, self.config.attribution);
        }

        // Add context if provided
        if let Some(ctx) = context {
            message = format!("{}\n\n{}", message, ctx);
        }

        // Truncate if needed
        if message.len() > self.config.max_message_length {
            message.truncate(self.config.max_message_length - 3);
            message.push_str("...");
        }

        message
    }

    /// Analyze diff to determine commit type (feat, fix, refactor, etc.)
    fn analyze_diff_type(&self, diff: &str) -> &'static str {
        let diff_lower = diff.to_lowercase();

        // Check for test files
        if diff.contains("test") || diff.contains("spec") {
            return "test";
        }

        // Check for documentation
        if diff.contains(".md") || diff.contains("README") || diff.contains("doc") {
            return "docs";
        }

        // Check for configuration files
        if diff.contains("config")
            || diff.contains(".toml")
            || diff.contains(".json")
            || diff.contains(".yaml")
        {
            return "chore";
        }

        // Check for bug fixes (common patterns)
        if diff_lower.contains("fix") || diff_lower.contains("bug") || diff_lower.contains("error")
        {
            return "fix";
        }

        // Check for refactoring
        if diff_lower.contains("refactor")
            || diff_lower.contains("rename")
            || diff_lower.contains("move")
        {
            return "refactor";
        }

        // Check for style/formatting
        if diff_lower.contains("style")
            || diff_lower.contains("format")
            || diff_lower.contains("lint")
        {
            return "style";
        }

        // Check for performance
        if diff_lower.contains("perf")
            || diff_lower.contains("optim")
            || diff_lower.contains("speed")
        {
            return "perf";
        }

        // Default to feat for new additions
        if diff.contains("+") && !diff.contains("-") {
            return "feat";
        }

        // Default
        "chore"
    }

    /// Summarize changes from diff
    fn summarize_changes(&self, diff: &str) -> String {
        let mut files_changed = Vec::new();
        let mut additions = 0;
        let mut deletions = 0;

        for line in diff.lines() {
            if line.starts_with("diff --git") {
                // Extract filename
                if let Some(file) = line.split(" b/").last() {
                    files_changed.push(file.to_string());
                }
            } else if line.starts_with('+') && !line.starts_with("+++") {
                additions += 1;
            } else if line.starts_with('-') && !line.starts_with("---") {
                deletions += 1;
            }
        }

        // Generate summary based on changes
        if files_changed.len() == 1 {
            let file = &files_changed[0];
            let filename = Path::new(file)
                .file_name()
                .map(|f| f.to_string_lossy().to_string())
                .unwrap_or_else(|| file.clone());

            if additions > 0 && deletions == 0 {
                format!("add {}", filename)
            } else if deletions > 0 && additions == 0 {
                format!("remove code from {}", filename)
            } else {
                format!("update {}", filename)
            }
        } else if files_changed.len() <= 3 {
            let names: Vec<String> = files_changed
                .iter()
                .filter_map(|f| Path::new(f).file_name())
                .map(|f| f.to_string_lossy().to_string())
                .collect();
            format!("update {}", names.join(", "))
        } else {
            format!("update {} files", files_changed.len())
        }
    }

    /// Auto-commit changes with generated message
    pub fn auto_commit(&self, context: Option<&str>) -> Result<Option<String>, GitError> {
        if !self.config.enabled {
            return Ok(None);
        }

        let status = self.git.status()?;

        if !status.has_changes() {
            debug!("No changes to commit");
            return Ok(None);
        }

        // Get diff for message generation
        let diff = if status.has_staged {
            self.git.diff_staged()?
        } else {
            self.git.diff_unstaged()?
        };

        // Generate message
        let message = self.generate_message(&diff, context);

        // Stage all changes if not already staged
        if !status.has_staged {
            self.git.add_all()?;
        }

        // Commit
        let hash = self.git.commit(&message)?;

        info!(
            "Auto-committed: {} - {}",
            hash,
            message.lines().next().unwrap_or(&message)
        );

        Ok(Some(hash))
    }

    /// Commit dirty files before AI changes
    pub fn commit_dirty(&self) -> Result<Option<String>, GitError> {
        if !self.config.commit_dirty_first {
            return Ok(None);
        }

        let status = self.git.status()?;

        if !status.has_changes() {
            return Ok(None);
        }

        self.git.add_all()?;
        let hash = self.git.commit(&self.config.dirty_commit_message)?;

        info!("Committed dirty files: {}", hash);

        Ok(Some(hash))
    }

    /// Generate message using LLM (placeholder for integration)
    pub async fn generate_message_with_llm(
        &self,
        diff: &str,
        chat_context: &str,
    ) -> Result<String, GitError> {
        // This would integrate with the LLM provider
        // For now, use the basic generator
        let message = self.generate_message(diff, Some(chat_context));
        Ok(message)
    }
}

// ============================================================================
// LLM Commit Message Prompt
// ============================================================================

/// Generate prompt for LLM-based commit message
pub fn llm_commit_prompt(diff: &str, chat_context: &str) -> String {
    format!(
        r#"Generate a concise git commit message for the following changes.

Rules:
1. Use conventional commit format: type(scope): description
2. Types: feat, fix, docs, style, refactor, perf, test, chore
3. Keep the first line under 72 characters
4. Be specific but concise
5. Use imperative mood ("add" not "added")

Chat context (what the user asked for):
{}

Diff:
```
{}
```

Respond with only the commit message, nothing else."#,
        chat_context, diff
    )
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_auto_commit_config_default() {
        let config = AutoCommitConfig::default();
        assert!(config.enabled);
        assert!(config.include_attribution);
        assert_eq!(config.style, CommitStyle::Conventional);
    }

    #[test]
    fn test_auto_commit_config_disabled() {
        let config = AutoCommitConfig::disabled();
        assert!(!config.enabled);
    }

    #[test]
    fn test_analyze_diff_type_test() {
        let generator = create_test_generator();
        assert_eq!(
            generator.analyze_diff_type("diff --git a/test_foo.rs"),
            "test"
        );
    }

    #[test]
    fn test_analyze_diff_type_docs() {
        let generator = create_test_generator();
        assert_eq!(
            generator.analyze_diff_type("diff --git a/README.md"),
            "docs"
        );
    }

    #[test]
    fn test_analyze_diff_type_fix() {
        let generator = create_test_generator();
        assert_eq!(generator.analyze_diff_type("fix the bug in parser"), "fix");
    }

    #[test]
    fn test_generate_message_conventional() {
        let generator = create_test_generator();
        let diff = "diff --git a/src/main.rs b/src/main.rs\n+fn new_function() {}";
        let message = generator.generate_message(diff, None);

        assert!(message.starts_with("feat:") || message.starts_with("chore:"));
        assert!(message.contains("(forge)"));
    }

    #[test]
    fn test_summarize_single_file() {
        let generator = create_test_generator();
        let diff = "diff --git a/src/lib.rs b/src/lib.rs\n+new line";
        let summary = generator.summarize_changes(diff);

        assert!(summary.contains("lib.rs"));
    }

    #[test]
    fn test_summarize_multiple_files() {
        let generator = create_test_generator();
        let diff = "diff --git a/src/a.rs b/src/a.rs\ndiff --git a/src/b.rs b/src/b.rs\ndiff --git a/src/c.rs b/src/c.rs\ndiff --git a/src/d.rs b/src/d.rs";
        let summary = generator.summarize_changes(diff);

        assert!(summary.contains("4 files"));
    }

    fn create_test_generator() -> CommitGenerator {
        // Create a mock generator for testing
        // In real tests, you'd use a temp git repo
        CommitGenerator {
            config: AutoCommitConfig::default(),
            git: GitOps::new(".").unwrap(),
        }
    }
}
