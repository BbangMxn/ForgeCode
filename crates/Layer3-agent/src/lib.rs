//! # forge-agent
//!
//! Agent system for ForgeCode - implements the core agent loop that:
//! - Manages conversation with LLM
//! - Handles tool calls
//! - Maintains session state

pub mod agent;
pub mod context;
pub mod history;
pub mod session;

pub use agent::{Agent, AgentEvent};
pub use context::{AgentContext, ProviderInfo};
pub use history::MessageHistory;
pub use session::{Session, SessionManager};
