//! Git Operations
//!
//! Core Git operations using git2 or shell commands.

use std::path::{Path, PathBuf};
use std::process::Command;
use thiserror::Error;
use tracing::{debug, info, warn};

// ============================================================================
// Error Types
// ============================================================================

#[derive(Debug, Error)]
pub enum GitError {
    #[error("Not a git repository: {0}")]
    NotARepository(PathBuf),

    #[error("Git command failed: {0}")]
    CommandFailed(String),

    #[error("No changes to commit")]
    NothingToCommit,

    #[error("Uncommitted changes exist")]
    DirtyWorkingTree,

    #[error("Branch not found: {0}")]
    BranchNotFound(String),

    #[error("Merge conflict detected")]
    MergeConflict,

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

// ============================================================================
// Git Status Types
// ============================================================================

/// Status of a single file
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileStatus {
    /// New file (untracked)
    New,
    /// Modified
    Modified,
    /// Deleted
    Deleted,
    /// Renamed
    Renamed { from: String },
    /// Copied
    Copied { from: String },
    /// Untracked
    Untracked,
    /// Ignored
    Ignored,
}

/// Overall git repository status
#[derive(Debug, Clone, Default)]
pub struct GitStatus {
    /// Current branch name
    pub branch: Option<String>,

    /// Files with their status
    pub files: Vec<(PathBuf, FileStatus)>,

    /// Number of commits ahead of remote
    pub ahead: u32,

    /// Number of commits behind remote
    pub behind: u32,

    /// Whether there are staged changes
    pub has_staged: bool,

    /// Whether there are unstaged changes
    pub has_unstaged: bool,

    /// Whether there are untracked files
    pub has_untracked: bool,
}

impl GitStatus {
    /// Check if working tree is clean
    pub fn is_clean(&self) -> bool {
        !self.has_staged && !self.has_unstaged && !self.has_untracked
    }

    /// Check if there are any changes (staged or unstaged)
    pub fn has_changes(&self) -> bool {
        self.has_staged || self.has_unstaged
    }
}

// ============================================================================
// Git Operations
// ============================================================================

/// Git operations handler
pub struct GitOps {
    /// Repository root directory
    root: PathBuf,
}

impl GitOps {
    /// Create new GitOps for a directory
    pub fn new(path: impl AsRef<Path>) -> Result<Self, GitError> {
        let path = path.as_ref();

        // Find git root
        let root = Self::find_git_root(path)?;

        Ok(Self { root })
    }

    /// Find the git repository root
    fn find_git_root(path: &Path) -> Result<PathBuf, GitError> {
        let mut current = if path.is_file() {
            path.parent().unwrap_or(path).to_path_buf()
        } else {
            path.to_path_buf()
        };

        loop {
            if current.join(".git").exists() {
                return Ok(current);
            }

            if let Some(parent) = current.parent() {
                current = parent.to_path_buf();
            } else {
                return Err(GitError::NotARepository(path.to_path_buf()));
            }
        }
    }

    /// Get repository root
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Check if directory is a git repository
    pub fn is_repo(path: impl AsRef<Path>) -> bool {
        Self::find_git_root(path.as_ref()).is_ok()
    }

    /// Run a git command
    fn run_git(&self, args: &[&str]) -> Result<String, GitError> {
        let output = Command::new("git")
            .args(args)
            .current_dir(&self.root)
            .output()?;

        if output.status.success() {
            Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            Err(GitError::CommandFailed(stderr.to_string()))
        }
    }

    /// Get current branch name
    pub fn current_branch(&self) -> Result<String, GitError> {
        self.run_git(&["rev-parse", "--abbrev-ref", "HEAD"])
    }

    /// Get repository status
    pub fn status(&self) -> Result<GitStatus, GitError> {
        let branch = self.current_branch().ok();

        let output = self.run_git(&["status", "--porcelain=v1"])?;

        let mut status = GitStatus {
            branch,
            ..Default::default()
        };

        for line in output.lines() {
            if line.len() < 3 {
                continue;
            }

            let index_status = line.chars().next().unwrap_or(' ');
            let worktree_status = line.chars().nth(1).unwrap_or(' ');
            let file_path = PathBuf::from(&line[3..]);

            let file_status = match (index_status, worktree_status) {
                ('?', '?') => {
                    status.has_untracked = true;
                    FileStatus::Untracked
                }
                ('!', '!') => FileStatus::Ignored,
                ('A', _) | (_, 'A') => {
                    if index_status == 'A' {
                        status.has_staged = true;
                    }
                    if worktree_status == 'A' {
                        status.has_unstaged = true;
                    }
                    FileStatus::New
                }
                ('M', _) | (_, 'M') => {
                    if index_status == 'M' {
                        status.has_staged = true;
                    }
                    if worktree_status == 'M' {
                        status.has_unstaged = true;
                    }
                    FileStatus::Modified
                }
                ('D', _) | (_, 'D') => {
                    if index_status == 'D' {
                        status.has_staged = true;
                    }
                    if worktree_status == 'D' {
                        status.has_unstaged = true;
                    }
                    FileStatus::Deleted
                }
                ('R', _) => {
                    status.has_staged = true;
                    FileStatus::Renamed {
                        from: String::new(),
                    }
                }
                ('C', _) => {
                    status.has_staged = true;
                    FileStatus::Copied {
                        from: String::new(),
                    }
                }
                _ => FileStatus::Modified,
            };

            status.files.push((file_path, file_status));
        }

        // Get ahead/behind counts
        if let Ok(counts) =
            self.run_git(&["rev-list", "--left-right", "--count", "HEAD...@{upstream}"])
        {
            let parts: Vec<&str> = counts.split_whitespace().collect();
            if parts.len() == 2 {
                status.ahead = parts[0].parse().unwrap_or(0);
                status.behind = parts[1].parse().unwrap_or(0);
            }
        }

        Ok(status)
    }

    /// Get diff of staged changes
    pub fn diff_staged(&self) -> Result<String, GitError> {
        self.run_git(&["diff", "--cached"])
    }

    /// Get diff of unstaged changes
    pub fn diff_unstaged(&self) -> Result<String, GitError> {
        self.run_git(&["diff"])
    }

    /// Get diff of all changes (staged + unstaged)
    pub fn diff_all(&self) -> Result<String, GitError> {
        self.run_git(&["diff", "HEAD"])
    }

    /// Get diff between two commits
    pub fn diff_commits(&self, from: &str, to: &str) -> Result<String, GitError> {
        self.run_git(&["diff", from, to])
    }

    /// Stage files
    pub fn add(&self, paths: &[&str]) -> Result<(), GitError> {
        let mut args = vec!["add"];
        args.extend(paths);
        self.run_git(&args)?;
        Ok(())
    }

    /// Stage all changes
    pub fn add_all(&self) -> Result<(), GitError> {
        self.run_git(&["add", "-A"])?;
        Ok(())
    }

    /// Commit staged changes
    pub fn commit(&self, message: &str) -> Result<String, GitError> {
        let status = self.status()?;
        if !status.has_staged {
            return Err(GitError::NothingToCommit);
        }

        let output = self.run_git(&["commit", "-m", message])?;

        // Extract commit hash
        let hash = self.run_git(&["rev-parse", "--short", "HEAD"])?;

        info!("Created commit: {}", hash);
        Ok(hash)
    }

    /// Commit all changes (stage + commit)
    pub fn commit_all(&self, message: &str) -> Result<String, GitError> {
        self.add_all()?;
        self.commit(message)
    }

    /// Get current commit hash
    pub fn head(&self) -> Result<String, GitError> {
        self.run_git(&["rev-parse", "HEAD"])
    }

    /// Get short commit hash
    pub fn head_short(&self) -> Result<String, GitError> {
        self.run_git(&["rev-parse", "--short", "HEAD"])
    }

    /// Reset to a specific commit
    pub fn reset(&self, commit: &str, hard: bool) -> Result<(), GitError> {
        let mode = if hard { "--hard" } else { "--soft" };
        self.run_git(&["reset", mode, commit])?;
        Ok(())
    }

    /// Stash changes
    pub fn stash(&self, message: Option<&str>) -> Result<(), GitError> {
        let args = if let Some(msg) = message {
            vec!["stash", "push", "-m", msg]
        } else {
            vec!["stash", "push"]
        };
        self.run_git(&args)?;
        Ok(())
    }

    /// Pop stashed changes
    pub fn stash_pop(&self) -> Result<(), GitError> {
        self.run_git(&["stash", "pop"])?;
        Ok(())
    }

    /// Create a tag
    pub fn tag(&self, name: &str, message: Option<&str>) -> Result<(), GitError> {
        let args = if let Some(msg) = message {
            vec!["tag", "-a", name, "-m", msg]
        } else {
            vec!["tag", name]
        };
        self.run_git(&args)?;
        Ok(())
    }

    /// Get log entries
    pub fn log(&self, count: usize) -> Result<Vec<LogEntry>, GitError> {
        let format = "--format=%H|%h|%s|%an|%ae|%aI";
        let output = self.run_git(&["log", format, "-n", &count.to_string()])?;

        let mut entries = Vec::new();
        for line in output.lines() {
            let parts: Vec<&str> = line.splitn(6, '|').collect();
            if parts.len() == 6 {
                entries.push(LogEntry {
                    hash: parts[0].to_string(),
                    short_hash: parts[1].to_string(),
                    message: parts[2].to_string(),
                    author_name: parts[3].to_string(),
                    author_email: parts[4].to_string(),
                    date: parts[5].to_string(),
                });
            }
        }

        Ok(entries)
    }

    /// Check if there are uncommitted changes
    pub fn is_dirty(&self) -> Result<bool, GitError> {
        let status = self.status()?;
        Ok(!status.is_clean())
    }

    /// Get list of modified files
    pub fn modified_files(&self) -> Result<Vec<PathBuf>, GitError> {
        let status = self.status()?;
        Ok(status
            .files
            .into_iter()
            .filter(|(_, s)| matches!(s, FileStatus::Modified | FileStatus::New))
            .map(|(p, _)| p)
            .collect())
    }

    /// Get file content at specific commit
    pub fn show_file(&self, commit: &str, path: &str) -> Result<String, GitError> {
        self.run_git(&["show", &format!("{}:{}", commit, path)])
    }

    /// Check if a commit exists
    pub fn commit_exists(&self, commit: &str) -> bool {
        self.run_git(&["cat-file", "-t", commit]).is_ok()
    }

    /// Get the merge base between two commits
    pub fn merge_base(&self, a: &str, b: &str) -> Result<String, GitError> {
        self.run_git(&["merge-base", a, b])
    }
}

// ============================================================================
// Log Entry
// ============================================================================

/// A git log entry
#[derive(Debug, Clone)]
pub struct LogEntry {
    pub hash: String,
    pub short_hash: String,
    pub message: String,
    pub author_name: String,
    pub author_email: String,
    pub date: String,
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::env::temp_dir;

    #[test]
    fn test_git_status_is_clean() {
        let status = GitStatus::default();
        assert!(status.is_clean());
    }

    #[test]
    fn test_git_status_has_changes() {
        let mut status = GitStatus::default();
        status.has_staged = true;
        assert!(status.has_changes());
        assert!(!status.is_clean());
    }

    #[test]
    fn test_is_repo() {
        // Test depends on environment - just verify the function works
        // without asserting specific result
        let _ = GitOps::is_repo(".");

        // Non-existent path should return false
        assert!(!GitOps::is_repo("/nonexistent/path/that/does/not/exist"));
    }
}
