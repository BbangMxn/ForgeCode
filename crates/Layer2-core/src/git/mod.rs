//! Git Integration Module
//!
//! Provides Git integration features inspired by Aider and Codex:
//! - Auto-commit after AI changes
//! - Checkpoint/rollback for recovery
//! - Diff-based commit message generation
//! - Ghost commits for turn-by-turn history
//!
//! ## Features
//!
//! - **Auto-commit**: Automatically commit AI-generated changes
//! - **Checkpoints**: Create restore points before risky operations
//! - **Rollback**: Revert to previous checkpoints
//! - **Commit Message Generation**: LLM-based commit messages from diffs

pub mod checkpoint;
pub mod commit;
pub mod ops;

pub use checkpoint::{Checkpoint, CheckpointId, CheckpointManager};
pub use commit::{AutoCommitConfig, CommitGenerator, CommitStyle};
pub use ops::{FileStatus, GitError, GitOps, GitStatus};
