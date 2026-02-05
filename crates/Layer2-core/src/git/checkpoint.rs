//! Checkpoint System
//!
//! Creates restore points before risky operations for easy rollback.
//! Inspired by Codex's ghost commits and Aider's auto-commit features.

use super::ops::{GitError, GitOps};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tracing::{debug, info};

// ============================================================================
// Checkpoint Types
// ============================================================================

/// Unique identifier for a checkpoint
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CheckpointId(pub String);

impl CheckpointId {
    pub fn new() -> Self {
        Self(uuid::Uuid::new_v4().to_string())
    }

    pub fn from_string(s: impl Into<String>) -> Self {
        Self(s.into())
    }
}

impl Default for CheckpointId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for CheckpointId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// A checkpoint representing a restore point
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Checkpoint {
    /// Unique checkpoint ID
    pub id: CheckpointId,

    /// Git commit hash at checkpoint creation
    pub commit_hash: String,

    /// Description of the checkpoint
    pub description: String,

    /// When the checkpoint was created
    pub created_at: DateTime<Utc>,

    /// Agent turn number (if applicable)
    pub turn: Option<u32>,

    /// Files modified since last checkpoint
    pub modified_files: Vec<PathBuf>,

    /// Whether this was an auto-checkpoint
    pub auto_created: bool,

    /// Stash reference (if uncommitted changes were stashed)
    pub stash_ref: Option<String>,

    /// Additional metadata
    pub metadata: HashMap<String, String>,
}

impl Checkpoint {
    /// Create a new checkpoint
    pub fn new(commit_hash: String, description: impl Into<String>) -> Self {
        Self {
            id: CheckpointId::new(),
            commit_hash,
            description: description.into(),
            created_at: Utc::now(),
            turn: None,
            modified_files: Vec::new(),
            auto_created: false,
            stash_ref: None,
            metadata: HashMap::new(),
        }
    }

    /// Mark as auto-created
    pub fn with_auto(mut self) -> Self {
        self.auto_created = true;
        self
    }

    /// Set turn number
    pub fn with_turn(mut self, turn: u32) -> Self {
        self.turn = Some(turn);
        self
    }

    /// Set modified files
    pub fn with_files(mut self, files: Vec<PathBuf>) -> Self {
        self.modified_files = files;
        self
    }

    /// Add metadata
    pub fn with_metadata(mut self, key: &str, value: &str) -> Self {
        self.metadata.insert(key.to_string(), value.to_string());
        self
    }
}

// ============================================================================
// Checkpoint Manager
// ============================================================================

/// Manages checkpoints for a repository
pub struct CheckpointManager {
    /// Git operations
    git: GitOps,

    /// Active checkpoints
    checkpoints: Vec<Checkpoint>,

    /// Maximum number of checkpoints to keep
    max_checkpoints: usize,

    /// Auto-checkpoint on every turn
    auto_checkpoint: bool,

    /// Current turn counter
    current_turn: u32,
}

impl CheckpointManager {
    /// Create a new checkpoint manager
    pub fn new(path: impl AsRef<Path>) -> Result<Self, GitError> {
        let git = GitOps::new(path)?;

        Ok(Self {
            git,
            checkpoints: Vec::new(),
            max_checkpoints: 50,
            auto_checkpoint: false,
            current_turn: 0,
        })
    }

    /// Enable auto-checkpointing
    pub fn with_auto_checkpoint(mut self, enabled: bool) -> Self {
        self.auto_checkpoint = enabled;
        self
    }

    /// Set maximum checkpoints to keep
    pub fn with_max_checkpoints(mut self, max: usize) -> Self {
        self.max_checkpoints = max;
        self
    }

    /// Create a manual checkpoint
    pub fn create(&mut self, description: &str) -> Result<CheckpointId, GitError> {
        self.create_checkpoint(description, false)
    }

    /// Create an auto-checkpoint (called on each turn)
    pub fn create_auto(&mut self) -> Result<Option<CheckpointId>, GitError> {
        if !self.auto_checkpoint {
            return Ok(None);
        }

        self.current_turn += 1;
        let description = format!("Turn {} checkpoint", self.current_turn);

        let id = self.create_checkpoint(&description, true)?;
        Ok(Some(id))
    }

    /// Internal checkpoint creation
    fn create_checkpoint(
        &mut self,
        description: &str,
        auto: bool,
    ) -> Result<CheckpointId, GitError> {
        let status = self.git.status()?;

        // Get current HEAD
        let commit_hash = self.git.head()?;

        // Get modified files
        let modified_files: Vec<PathBuf> = status.files.iter().map(|(p, _)| p.clone()).collect();

        // Create checkpoint
        let mut checkpoint = Checkpoint::new(commit_hash, description).with_files(modified_files);

        if auto {
            checkpoint = checkpoint.with_auto().with_turn(self.current_turn);
        }

        // If there are uncommitted changes, we might want to stash or commit them
        if status.has_changes() {
            // For ghost commits: create a temporary commit
            if auto {
                // Stash uncommitted changes
                self.git
                    .stash(Some(&format!("checkpoint-{}", checkpoint.id)))?;
                checkpoint.stash_ref = Some(format!("stash@{{0}}"));
            }
        }

        let id = checkpoint.id.clone();

        info!("Created checkpoint: {} - {}", id, description);

        self.checkpoints.push(checkpoint);

        // Cleanup old checkpoints
        self.cleanup_old_checkpoints();

        Ok(id)
    }

    /// Rollback to a specific checkpoint
    pub fn rollback(&mut self, checkpoint_id: &CheckpointId) -> Result<(), GitError> {
        let checkpoint = self
            .checkpoints
            .iter()
            .find(|c| &c.id == checkpoint_id)
            .ok_or_else(|| {
                GitError::CommandFailed(format!("Checkpoint not found: {}", checkpoint_id))
            })?
            .clone();

        info!(
            "Rolling back to checkpoint: {} ({})",
            checkpoint.id, checkpoint.commit_hash
        );

        // Reset to the checkpoint commit
        self.git.reset(&checkpoint.commit_hash, true)?;

        // If there was a stash, pop it
        if checkpoint.stash_ref.is_some() {
            // Note: This might fail if the stash was already popped
            let _ = self.git.stash_pop();
        }

        // Remove checkpoints after this one
        let idx = self.checkpoints.iter().position(|c| &c.id == checkpoint_id);
        if let Some(idx) = idx {
            self.checkpoints.truncate(idx + 1);
        }

        Ok(())
    }

    /// Rollback to the last checkpoint
    pub fn rollback_last(&mut self) -> Result<(), GitError> {
        let last = self
            .checkpoints
            .last()
            .ok_or_else(|| GitError::CommandFailed("No checkpoints available".to_string()))?
            .clone();

        self.rollback(&last.id)
    }

    /// Rollback N turns
    pub fn rollback_turns(&mut self, n: u32) -> Result<(), GitError> {
        let target_turn = self.current_turn.saturating_sub(n);

        // Find checkpoint at or before target turn
        let checkpoint = self
            .checkpoints
            .iter()
            .rev()
            .find(|c| c.turn.map(|t| t <= target_turn).unwrap_or(false))
            .ok_or_else(|| {
                GitError::CommandFailed(format!("No checkpoint found for turn {}", target_turn))
            })?
            .clone();

        self.rollback(&checkpoint.id)
    }

    /// Get all checkpoints
    pub fn list(&self) -> &[Checkpoint] {
        &self.checkpoints
    }

    /// Get a specific checkpoint
    pub fn get(&self, id: &CheckpointId) -> Option<&Checkpoint> {
        self.checkpoints.iter().find(|c| &c.id == id)
    }

    /// Get the latest checkpoint
    pub fn latest(&self) -> Option<&Checkpoint> {
        self.checkpoints.last()
    }

    /// Get diff between current state and a checkpoint
    pub fn diff_from(&self, checkpoint_id: &CheckpointId) -> Result<String, GitError> {
        let checkpoint = self.get(checkpoint_id).ok_or_else(|| {
            GitError::CommandFailed(format!("Checkpoint not found: {}", checkpoint_id))
        })?;

        self.git.diff_commits(&checkpoint.commit_hash, "HEAD")
    }

    /// Cleanup old checkpoints beyond max limit
    fn cleanup_old_checkpoints(&mut self) {
        while self.checkpoints.len() > self.max_checkpoints {
            let removed = self.checkpoints.remove(0);
            debug!("Removed old checkpoint: {}", removed.id);
        }
    }

    /// Clear all checkpoints
    pub fn clear(&mut self) {
        self.checkpoints.clear();
        info!("Cleared all checkpoints");
    }

    /// Get current turn number
    pub fn current_turn(&self) -> u32 {
        self.current_turn
    }

    /// Set current turn (for resuming sessions)
    pub fn set_turn(&mut self, turn: u32) {
        self.current_turn = turn;
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_checkpoint_id_new() {
        let id1 = CheckpointId::new();
        let id2 = CheckpointId::new();
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_checkpoint_new() {
        let checkpoint = Checkpoint::new("abc123".to_string(), "Test checkpoint");
        assert_eq!(checkpoint.commit_hash, "abc123");
        assert_eq!(checkpoint.description, "Test checkpoint");
        assert!(!checkpoint.auto_created);
    }

    #[test]
    fn test_checkpoint_with_auto() {
        let checkpoint = Checkpoint::new("abc123".to_string(), "Test")
            .with_auto()
            .with_turn(5);

        assert!(checkpoint.auto_created);
        assert_eq!(checkpoint.turn, Some(5));
    }

    #[test]
    fn test_checkpoint_with_metadata() {
        let checkpoint =
            Checkpoint::new("abc123".to_string(), "Test").with_metadata("key", "value");

        assert_eq!(checkpoint.metadata.get("key"), Some(&"value".to_string()));
    }
}
