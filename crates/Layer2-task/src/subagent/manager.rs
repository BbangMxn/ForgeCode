//! Sub-agent manager - orchestrates sub-agent lifecycle

use crate::subagent::{
    Discovery, SubAgent, SubAgentConfig, SubAgentId, SubAgentState, SubAgentType,
};
use forge_foundation::{Error, Result};
use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex, Notify, RwLock};
use tracing::{debug, info, warn};

/// Output directory for background agents
fn default_output_dir() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("forgecode")
        .join("agents")
}

/// Sub-agent manager configuration
#[derive(Debug, Clone)]
pub struct SubAgentManagerConfig {
    /// Maximum concurrent sub-agents
    pub max_concurrent: usize,

    /// Output directory for background agents
    pub output_dir: PathBuf,

    /// Default max turns
    pub default_max_turns: u32,

    /// Enable queue for waiting agents
    pub enable_queue: bool,

    /// Maximum queue size (0 = unlimited)
    pub max_queue_size: usize,

    /// Queue timeout in seconds (0 = no timeout)
    pub queue_timeout_secs: u64,
}

impl Default for SubAgentManagerConfig {
    fn default() -> Self {
        Self {
            max_concurrent: 4,
            output_dir: default_output_dir(),
            default_max_turns: 50,
            enable_queue: true,
            max_queue_size: 16,
            queue_timeout_secs: 300, // 5 minutes
        }
    }
}

/// Queue entry priority
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum QueuePriority {
    /// Low priority (background tasks)
    Low = 0,
    /// Normal priority (default)
    Normal = 1,
    /// High priority (user-initiated)
    High = 2,
    /// Critical priority (system tasks)
    Critical = 3,
}

impl Default for QueuePriority {
    fn default() -> Self {
        Self::Normal
    }
}

/// A queued spawn request
#[derive(Debug)]
struct QueuedSpawn {
    /// Queue entry ID
    id: u64,
    /// Agent ID
    agent_id: SubAgentId,
    /// Priority
    priority: QueuePriority,
    /// When queued
    queued_at: std::time::Instant,
    /// Notify when ready to start
    ready_tx: mpsc::Sender<()>,
}

/// Queue statistics
#[derive(Debug, Clone, Default)]
pub struct QueueStats {
    /// Current queue length
    pub queue_length: usize,
    /// Total agents queued
    pub total_queued: u64,
    /// Total timeouts
    pub total_timeouts: u64,
    /// Average wait time in milliseconds
    pub avg_wait_ms: u64,
}

/// Sub-agent manager - handles sub-agent lifecycle
pub struct SubAgentManager {
    /// All sub-agents by ID
    agents: Arc<RwLock<HashMap<SubAgentId, SubAgent>>>,

    /// Currently running agent count
    running_count: Arc<Mutex<usize>>,

    /// Shared context store
    context_store: Arc<RwLock<crate::subagent::context::ContextStore>>,

    /// Configuration
    config: SubAgentManagerConfig,

    /// Waiting queue (priority sorted)
    queue: Arc<Mutex<VecDeque<QueuedSpawn>>>,

    /// Queue entry ID counter
    queue_id_counter: AtomicU64,

    /// Notify when slot becomes available
    slot_available: Arc<Notify>,

    /// Queue statistics
    queue_stats: Arc<Mutex<QueueStats>>,

    /// Total wait time for average calculation
    total_wait_ms: AtomicU64,
}

impl SubAgentManager {
    /// Create a new sub-agent manager
    pub fn new(config: SubAgentManagerConfig) -> Self {
        // Ensure output directory exists
        if let Err(e) = std::fs::create_dir_all(&config.output_dir) {
            warn!("Failed to create output directory: {}", e);
        }

        Self {
            agents: Arc::new(RwLock::new(HashMap::new())),
            running_count: Arc::new(Mutex::new(0)),
            context_store: Arc::new(RwLock::new(crate::subagent::context::ContextStore::new())),
            config,
            queue: Arc::new(Mutex::new(VecDeque::new())),
            queue_id_counter: AtomicU64::new(0),
            slot_available: Arc::new(Notify::new()),
            queue_stats: Arc::new(Mutex::new(QueueStats::default())),
            total_wait_ms: AtomicU64::new(0),
        }
    }

    /// Create with default configuration
    pub fn with_default_config() -> Self {
        Self::new(SubAgentManagerConfig::default())
    }

    /// Spawn a new sub-agent
    pub async fn spawn(
        &self,
        parent_session_id: &str,
        agent_type: SubAgentType,
        prompt: &str,
        description: &str,
    ) -> Result<SubAgentId> {
        let config = SubAgentConfig::for_type(agent_type);
        self.spawn_with_priority(
            parent_session_id,
            config,
            prompt,
            description,
            QueuePriority::Normal,
        )
        .await
    }

    /// Spawn a sub-agent with custom configuration
    pub async fn spawn_with_config(
        &self,
        parent_session_id: &str,
        config: SubAgentConfig,
        prompt: &str,
        description: &str,
    ) -> Result<SubAgentId> {
        self.spawn_with_priority(
            parent_session_id,
            config,
            prompt,
            description,
            QueuePriority::Normal,
        )
        .await
    }

    /// Spawn a sub-agent with priority
    pub async fn spawn_with_priority(
        &self,
        parent_session_id: &str,
        config: SubAgentConfig,
        prompt: &str,
        description: &str,
        priority: QueuePriority,
    ) -> Result<SubAgentId> {
        // Create agent first
        let mut agent = SubAgent::new(parent_session_id, config, prompt, description);

        // Set output file for background agents
        if agent.config.run_in_background {
            let output_file = self.config.output_dir.join(format!("{}.output", agent.id));
            agent.set_output_file(output_file);
        }

        let agent_id = agent.id;

        // Store agent
        {
            let mut agents = self.agents.write().await;
            agents.insert(agent_id, agent);
        }

        // Check concurrent limit
        let needs_queue = {
            let count = self.running_count.lock().await;
            *count >= self.config.max_concurrent
        };

        if needs_queue {
            if !self.config.enable_queue {
                return Err(Error::Task(format!(
                    "Maximum concurrent sub-agents reached ({}) and queue is disabled",
                    self.config.max_concurrent
                )));
            }

            // Check queue size limit
            {
                let queue = self.queue.lock().await;
                if self.config.max_queue_size > 0 && queue.len() >= self.config.max_queue_size {
                    // Remove the agent we just added
                    let mut agents = self.agents.write().await;
                    agents.remove(&agent_id);
                    return Err(Error::Task(format!(
                        "Agent queue is full (max: {})",
                        self.config.max_queue_size
                    )));
                }
            }

            // Add to queue
            info!(
                "Queuing sub-agent {} (priority: {:?}): {}",
                agent_id, priority, description
            );

            self.enqueue_agent(agent_id, priority).await?;
        }

        info!(
            "Spawned sub-agent {} ({}): {}",
            agent_id, parent_session_id, description
        );

        Ok(agent_id)
    }

    /// Enqueue an agent and wait for slot
    async fn enqueue_agent(&self, agent_id: SubAgentId, priority: QueuePriority) -> Result<()> {
        let queue_id = self.queue_id_counter.fetch_add(1, Ordering::SeqCst);
        let queued_at = std::time::Instant::now();

        // Create channel for notification
        let (ready_tx, mut ready_rx) = mpsc::channel(1);

        // Add to queue
        {
            let mut queue = self.queue.lock().await;
            let entry = QueuedSpawn {
                id: queue_id,
                agent_id,
                priority,
                queued_at,
                ready_tx,
            };

            // Insert by priority (higher priority first)
            let insert_pos = queue
                .iter()
                .position(|e| e.priority < priority)
                .unwrap_or(queue.len());
            queue.insert(insert_pos, entry);

            // Update stats
            let mut stats = self.queue_stats.lock().await;
            stats.queue_length = queue.len();
            stats.total_queued += 1;
        }

        debug!("Agent {} queued at position (id: {})", agent_id, queue_id);

        // Wait for slot with timeout
        let timeout_duration = if self.config.queue_timeout_secs > 0 {
            Some(std::time::Duration::from_secs(
                self.config.queue_timeout_secs,
            ))
        } else {
            None
        };

        let wait_result = if let Some(timeout) = timeout_duration {
            tokio::time::timeout(timeout, ready_rx.recv()).await
        } else {
            Ok(ready_rx.recv().await)
        };

        let wait_ms = queued_at.elapsed().as_millis() as u64;

        match wait_result {
            Ok(Some(())) => {
                // Successfully got a slot
                self.total_wait_ms.fetch_add(wait_ms, Ordering::SeqCst);
                self.update_avg_wait().await;
                debug!("Agent {} got slot after {}ms", agent_id, wait_ms);
                Ok(())
            }
            Ok(None) => {
                // Channel closed (manager shutdown?)
                self.remove_from_queue(agent_id).await;
                Err(Error::Task("Queue channel closed".to_string()))
            }
            Err(_) => {
                // Timeout
                self.remove_from_queue(agent_id).await;
                {
                    let mut stats = self.queue_stats.lock().await;
                    stats.total_timeouts += 1;
                }
                warn!(
                    "Agent {} queue timeout after {}s",
                    agent_id, self.config.queue_timeout_secs
                );
                Err(Error::Task(format!(
                    "Queue timeout after {}s",
                    self.config.queue_timeout_secs
                )))
            }
        }
    }

    /// Remove an agent from the queue
    async fn remove_from_queue(&self, agent_id: SubAgentId) {
        let mut queue = self.queue.lock().await;
        queue.retain(|e| e.agent_id != agent_id);

        let mut stats = self.queue_stats.lock().await;
        stats.queue_length = queue.len();
    }

    /// Update average wait time
    async fn update_avg_wait(&self) {
        let stats = self.queue_stats.lock().await;
        if stats.total_queued > 0 {
            let total = self.total_wait_ms.load(Ordering::SeqCst);
            let mut stats = self.queue_stats.lock().await;
            stats.avg_wait_ms = total / stats.total_queued;
        }
    }

    /// Notify next queued agent
    async fn notify_next_in_queue(&self) {
        let mut queue = self.queue.lock().await;
        if let Some(entry) = queue.pop_front() {
            let wait_ms = entry.queued_at.elapsed().as_millis() as u64;
            debug!(
                "Notifying queued agent {} (waited {}ms)",
                entry.agent_id, wait_ms
            );
            let _ = entry.ready_tx.send(()).await;

            let mut stats = self.queue_stats.lock().await;
            stats.queue_length = queue.len();
        }

        // Also notify any waiters
        self.slot_available.notify_one();
    }

    /// Get queue statistics
    pub async fn queue_stats(&self) -> QueueStats {
        self.queue_stats.lock().await.clone()
    }

    /// Get current queue length
    pub async fn queue_length(&self) -> usize {
        self.queue.lock().await.len()
    }

    /// Get queued agent IDs (in order)
    pub async fn queued_agents(&self) -> Vec<SubAgentId> {
        self.queue.lock().await.iter().map(|e| e.agent_id).collect()
    }

    /// Spawn a background sub-agent
    pub async fn spawn_background(
        &self,
        parent_session_id: &str,
        agent_type: SubAgentType,
        prompt: &str,
        description: &str,
    ) -> Result<SubAgentId> {
        let config = SubAgentConfig::for_type(agent_type).run_in_background();
        self.spawn_with_config(parent_session_id, config, prompt, description)
            .await
    }

    /// Start an agent (mark as running)
    pub async fn start(&self, agent_id: SubAgentId) -> Result<()> {
        let mut agents = self.agents.write().await;
        let agent = agents
            .get_mut(&agent_id)
            .ok_or_else(|| Error::NotFound(format!("Agent {} not found", agent_id)))?;

        if agent.state.is_running() {
            return Err(Error::Task(format!(
                "Agent {} is already running",
                agent_id
            )));
        }

        let max_turns = agent.config.max_turns;
        agent.start(max_turns);

        // Increment running count
        {
            let mut count = self.running_count.lock().await;
            *count += 1;
        }

        debug!("Started sub-agent {}", agent_id);
        Ok(())
    }

    /// Record a turn for an agent
    pub async fn record_turn(&self, agent_id: SubAgentId) -> Result<bool> {
        let mut agents = self.agents.write().await;
        let agent = agents
            .get_mut(&agent_id)
            .ok_or_else(|| Error::NotFound(format!("Agent {} not found", agent_id)))?;

        Ok(agent.next_turn())
    }

    /// Complete an agent
    pub async fn complete(&self, agent_id: SubAgentId, summary: &str) -> Result<()> {
        let was_running = {
            let mut agents = self.agents.write().await;
            let agent = agents
                .get_mut(&agent_id)
                .ok_or_else(|| Error::NotFound(format!("Agent {} not found", agent_id)))?;

            let was_running = agent.state.is_running();

            // Share discoveries if configured
            if agent.config.share_discoveries {
                let discoveries = agent.context.export_discoveries();
                let mut store = self.context_store.write().await;
                for discovery in discoveries {
                    store.add(discovery);
                }
            }

            agent.complete(summary);
            was_running
        };

        // Decrement running count and notify queue
        if was_running {
            {
                let mut count = self.running_count.lock().await;
                *count = count.saturating_sub(1);
            }
            // Notify next in queue
            self.notify_next_in_queue().await;
        }

        info!("Completed sub-agent {}: {}", agent_id, summary);
        Ok(())
    }

    /// Fail an agent
    pub async fn fail(&self, agent_id: SubAgentId, error: &str) -> Result<()> {
        let was_running = {
            let mut agents = self.agents.write().await;
            let agent = agents
                .get_mut(&agent_id)
                .ok_or_else(|| Error::NotFound(format!("Agent {} not found", agent_id)))?;

            let was_running = agent.state.is_running();
            agent.fail(error);
            was_running
        };

        // Decrement running count and notify queue
        if was_running {
            {
                let mut count = self.running_count.lock().await;
                *count = count.saturating_sub(1);
            }
            // Notify next in queue
            self.notify_next_in_queue().await;
        }

        warn!("Failed sub-agent {}: {}", agent_id, error);
        Ok(())
    }

    /// Cancel an agent
    pub async fn cancel(&self, agent_id: SubAgentId, reason: Option<&str>) -> Result<()> {
        let was_running = {
            let mut agents = self.agents.write().await;
            let agent = agents
                .get_mut(&agent_id)
                .ok_or_else(|| Error::NotFound(format!("Agent {} not found", agent_id)))?;

            if agent.state.is_terminal() {
                return Err(Error::Task(format!(
                    "Agent {} is already in terminal state",
                    agent_id
                )));
            }

            let was_running = agent.state.is_running();
            agent.cancel(reason.map(String::from));
            was_running
        };

        // Decrement running count if was running and notify queue
        if was_running {
            {
                let mut count = self.running_count.lock().await;
                *count = count.saturating_sub(1);
            }
            // Notify next in queue
            self.notify_next_in_queue().await;
        }

        info!("Cancelled sub-agent {}", agent_id);
        Ok(())
    }

    /// Resume a paused or completed agent
    pub async fn resume(&self, agent_id: SubAgentId, new_prompt: &str) -> Result<SubAgentId> {
        let agents = self.agents.read().await;
        let agent = agents
            .get(&agent_id)
            .ok_or_else(|| Error::NotFound(format!("Agent {} not found", agent_id)))?;

        if !agent.state.is_resumable() {
            return Err(Error::Task(format!(
                "Agent {} cannot be resumed (state: {})",
                agent_id,
                agent.state.display_name()
            )));
        }

        // Create new agent with inherited context
        let mut new_config = agent.config.clone();
        new_config.inherit_context = true;

        let mut new_agent = SubAgent::new(
            &agent.parent_session_id,
            new_config,
            new_prompt,
            &format!("Resume of {}: {}", agent_id, agent.description),
        );

        // Copy context
        new_agent.context = agent.context.clone();
        new_agent
            .context
            .add_message(crate::subagent::context::ContextMessage::user(new_prompt));

        let new_id = new_agent.id;

        drop(agents); // Release read lock

        // Store new agent
        {
            let mut agents = self.agents.write().await;
            agents.insert(new_id, new_agent);
        }

        info!("Resumed agent {} as {}", agent_id, new_id);
        Ok(new_id)
    }

    /// Get an agent by ID
    pub async fn get(&self, agent_id: SubAgentId) -> Option<SubAgent> {
        let agents = self.agents.read().await;
        agents.get(&agent_id).cloned()
    }

    /// Get agent state
    pub async fn get_state(&self, agent_id: SubAgentId) -> Option<SubAgentState> {
        let agents = self.agents.read().await;
        agents.get(&agent_id).map(|a| a.state.clone())
    }

    /// Get output file for a background agent
    pub async fn get_output_file(&self, agent_id: SubAgentId) -> Option<PathBuf> {
        let agents = self.agents.read().await;
        agents.get(&agent_id).and_then(|a| a.output_file.clone())
    }

    /// Get all agents for a session
    pub async fn get_by_session(&self, session_id: &str) -> Vec<SubAgent> {
        let agents = self.agents.read().await;
        agents
            .values()
            .filter(|a| a.parent_session_id == session_id)
            .cloned()
            .collect()
    }

    /// Get running agents
    pub async fn get_running(&self) -> Vec<SubAgent> {
        let agents = self.agents.read().await;
        agents
            .values()
            .filter(|a| a.state.is_running())
            .cloned()
            .collect()
    }

    /// Get running count
    pub async fn running_count(&self) -> usize {
        *self.running_count.lock().await
    }

    /// Add a discovery to an agent's context
    pub async fn add_discovery(&self, agent_id: SubAgentId, discovery: Discovery) -> Result<()> {
        let mut agents = self.agents.write().await;
        let agent = agents
            .get_mut(&agent_id)
            .ok_or_else(|| Error::NotFound(format!("Agent {} not found", agent_id)))?;

        agent.context.add_discovery(discovery);
        Ok(())
    }

    /// Get discoveries from the shared context store
    pub async fn get_shared_discoveries(&self, category: Option<&str>) -> Vec<Discovery> {
        let store = self.context_store.read().await;
        match category {
            Some(cat) => store.get_by_category(cat).into_iter().cloned().collect(),
            None => store
                .categories()
                .iter()
                .flat_map(|cat| store.get_by_category(cat))
                .cloned()
                .collect(),
        }
    }

    /// Check if a tool is allowed for an agent
    pub async fn is_tool_allowed(&self, agent_id: SubAgentId, tool_name: &str) -> bool {
        let agents = self.agents.read().await;
        agents
            .get(&agent_id)
            .map(|a| a.config.is_tool_allowed(tool_name))
            .unwrap_or(false)
    }

    /// Write output to file for background agent
    pub async fn write_output(&self, agent_id: SubAgentId, content: &str) -> Result<()> {
        let agents = self.agents.read().await;
        let agent = agents
            .get(&agent_id)
            .ok_or_else(|| Error::NotFound(format!("Agent {} not found", agent_id)))?;

        if let Some(ref output_file) = agent.output_file {
            std::fs::write(output_file, content)
                .map_err(|e| Error::Task(format!("Failed to write output: {}", e)))?;
        }

        Ok(())
    }

    /// Append output to file for background agent
    pub async fn append_output(&self, agent_id: SubAgentId, content: &str) -> Result<()> {
        let agents = self.agents.read().await;
        let agent = agents
            .get(&agent_id)
            .ok_or_else(|| Error::NotFound(format!("Agent {} not found", agent_id)))?;

        if let Some(ref output_file) = agent.output_file {
            use std::io::Write;
            let mut file = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(output_file)
                .map_err(|e| Error::Task(format!("Failed to open output file: {}", e)))?;

            writeln!(file, "{}", content)
                .map_err(|e| Error::Task(format!("Failed to write output: {}", e)))?;
        }

        Ok(())
    }

    /// Cleanup old agents (keep only last N per session)
    pub async fn cleanup(&self, keep_per_session: usize) {
        let mut agents = self.agents.write().await;

        // Group by session
        let mut by_session: HashMap<String, Vec<SubAgentId>> = HashMap::new();
        for (id, agent) in agents.iter() {
            if agent.state.is_terminal() {
                by_session
                    .entry(agent.parent_session_id.clone())
                    .or_default()
                    .push(*id);
            }
        }

        // Remove old agents
        for (_, mut ids) in by_session {
            if ids.len() > keep_per_session {
                // Sort by completion time (oldest first)
                ids.sort_by_key(|id| {
                    agents
                        .get(id)
                        .and_then(|a| a.completed_at)
                        .unwrap_or_else(|| chrono::Utc::now())
                });

                // Remove oldest
                for id in ids.iter().take(ids.len() - keep_per_session) {
                    agents.remove(id);
                    debug!("Cleaned up agent {}", id);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_spawn_agent() {
        let manager = SubAgentManager::with_default_config();

        let agent_id = manager
            .spawn(
                "session-1",
                SubAgentType::Explore,
                "Find APIs",
                "API search",
            )
            .await
            .unwrap();

        let agent = manager.get(agent_id).await.unwrap();
        assert_eq!(agent.parent_session_id, "session-1");
        assert!(matches!(agent.config.agent_type, SubAgentType::Explore));
    }

    #[tokio::test]
    async fn test_agent_lifecycle() {
        let manager = SubAgentManager::with_default_config();

        let agent_id = manager
            .spawn("session-1", SubAgentType::Explore, "Test", "Test")
            .await
            .unwrap();

        // Start
        manager.start(agent_id).await.unwrap();
        let state = manager.get_state(agent_id).await.unwrap();
        assert!(state.is_running());

        // Complete
        manager.complete(agent_id, "Done").await.unwrap();
        let state = manager.get_state(agent_id).await.unwrap();
        assert!(state.is_terminal());
    }

    #[tokio::test]
    async fn test_resume() {
        let manager = SubAgentManager::with_default_config();

        let agent_id = manager
            .spawn(
                "session-1",
                SubAgentType::Explore,
                "Find APIs",
                "API search",
            )
            .await
            .unwrap();

        manager.start(agent_id).await.unwrap();
        manager.complete(agent_id, "Found 3 APIs").await.unwrap();

        // Resume
        let new_id = manager.resume(agent_id, "Find more details").await.unwrap();

        let new_agent = manager.get(new_id).await.unwrap();
        assert!(new_agent.config.inherit_context);
    }

    #[tokio::test]
    async fn test_tool_allowed() {
        let manager = SubAgentManager::with_default_config();

        let agent_id = manager
            .spawn("session-1", SubAgentType::Explore, "Test", "Test")
            .await
            .unwrap();

        assert!(manager.is_tool_allowed(agent_id, "read").await);
        assert!(manager.is_tool_allowed(agent_id, "grep").await);
        assert!(!manager.is_tool_allowed(agent_id, "write").await);
        assert!(!manager.is_tool_allowed(agent_id, "bash").await);
    }

    #[tokio::test]
    async fn test_queue_priority() {
        let config = SubAgentManagerConfig {
            max_concurrent: 1,
            enable_queue: true,
            max_queue_size: 10,
            queue_timeout_secs: 1,
            ..Default::default()
        };
        let manager = SubAgentManager::new(config);

        // Spawn first agent (takes the slot)
        let agent1 = manager
            .spawn("session-1", SubAgentType::Explore, "Task 1", "First task")
            .await
            .unwrap();
        manager.start(agent1).await.unwrap();

        // Queue stats should be empty
        let stats = manager.queue_stats().await;
        assert_eq!(stats.queue_length, 0);
    }

    #[tokio::test]
    async fn test_queue_disabled() {
        let config = SubAgentManagerConfig {
            max_concurrent: 1,
            enable_queue: false,
            ..Default::default()
        };
        let manager = SubAgentManager::new(config);

        // First agent takes the slot
        let agent1 = manager
            .spawn("session-1", SubAgentType::Explore, "Task 1", "First")
            .await
            .unwrap();
        manager.start(agent1).await.unwrap();

        // Second spawn should fail immediately (queue disabled)
        let result = manager
            .spawn("session-1", SubAgentType::Explore, "Task 2", "Second")
            .await;

        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("queue is disabled"));
    }
}
