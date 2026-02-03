//! Session management

use chrono::{DateTime, Utc};
use forge_foundation::{Error, Result, SessionRecord, Storage};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

/// A conversation session
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    /// Unique session ID
    pub id: String,

    /// Session title (auto-generated or user-provided)
    pub title: Option<String>,

    /// When the session was created
    pub created_at: DateTime<Utc>,

    /// When the session was last updated
    pub updated_at: DateTime<Utc>,

    /// Whether the session is active
    pub active: bool,
}

impl Session {
    /// Create a new session
    pub fn new() -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4().to_string(),
            title: None,
            created_at: now,
            updated_at: now,
            active: true,
        }
    }

    /// Create a session with a specific ID
    pub fn with_id(id: impl Into<String>) -> Self {
        let now = Utc::now();
        Self {
            id: id.into(),
            title: None,
            created_at: now,
            updated_at: now,
            active: true,
        }
    }

    /// Set session title
    pub fn set_title(&mut self, title: impl Into<String>) {
        self.title = Some(title.into());
        self.updated_at = Utc::now();
    }

    /// Mark session as updated
    pub fn touch(&mut self) {
        self.updated_at = Utc::now();
    }
}

impl Default for Session {
    fn default() -> Self {
        Self::new()
    }
}

/// Manages multiple sessions
pub struct SessionManager {
    /// Active sessions in memory
    sessions: Arc<RwLock<HashMap<String, Session>>>,

    /// Current active session ID
    current_session_id: Arc<RwLock<Option<String>>>,

    /// Storage for persistence
    storage: Option<Arc<Storage>>,
}

impl SessionManager {
    /// Create a new session manager
    pub fn new() -> Self {
        Self {
            sessions: Arc::new(RwLock::new(HashMap::new())),
            current_session_id: Arc::new(RwLock::new(None)),
            storage: None,
        }
    }

    /// Create with storage for persistence
    pub fn with_storage(storage: Arc<Storage>) -> Self {
        Self {
            sessions: Arc::new(RwLock::new(HashMap::new())),
            current_session_id: Arc::new(RwLock::new(None)),
            storage: Some(storage),
        }
    }

    /// Create or get a session
    pub async fn get_or_create(&self) -> Session {
        // Check if there's a current session
        {
            let current_id = self.current_session_id.read().await;
            if let Some(id) = current_id.as_ref() {
                let sessions = self.sessions.read().await;
                if let Some(session) = sessions.get(id) {
                    return session.clone();
                }
            }
        }

        // Create new session
        let session = Session::new();
        self.add_session(session.clone()).await;
        self.set_current(&session.id).await;

        session
    }

    /// Add a session
    pub async fn add_session(&self, session: Session) {
        let id = session.id.clone();

        // Store in memory
        {
            let mut sessions = self.sessions.write().await;
            sessions.insert(id.clone(), session);
        }

        // Persist to storage
        if let Some(ref storage) = self.storage {
            let record = SessionRecord {
                id: id.clone(),
                title: None,
                ..Default::default()
            };
            let _ = storage.create_session(&record);
        }
    }

    /// Get a session by ID
    pub async fn get(&self, id: &str) -> Option<Session> {
        let sessions = self.sessions.read().await;
        sessions.get(id).cloned()
    }

    /// Get current session
    pub async fn current(&self) -> Option<Session> {
        let current_id = self.current_session_id.read().await;
        if let Some(id) = current_id.as_ref() {
            self.get(id).await
        } else {
            None
        }
    }

    /// Set current session
    pub async fn set_current(&self, id: &str) {
        let mut current = self.current_session_id.write().await;
        *current = Some(id.to_string());
    }

    /// List all sessions
    pub async fn list(&self) -> Vec<Session> {
        let sessions = self.sessions.read().await;
        let mut list: Vec<_> = sessions.values().cloned().collect();
        list.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        list
    }

    /// Update session title
    pub async fn set_title(&self, id: &str, title: &str) -> Result<()> {
        let mut sessions = self.sessions.write().await;
        if let Some(session) = sessions.get_mut(id) {
            session.set_title(title);

            // Persist - get existing record and update
            if let Some(ref storage) = self.storage {
                if let Ok(Some(mut record)) = storage.get_session(id) {
                    record.title = Some(title.to_string());
                    let _ = storage.update_session(&record);
                }
            }

            Ok(())
        } else {
            Err(Error::NotFound(format!("Session {} not found", id)))
        }
    }

    /// Delete a session
    pub async fn delete(&self, id: &str) -> Result<()> {
        let mut sessions = self.sessions.write().await;
        sessions.remove(id);

        // Update current session if needed
        {
            let mut current = self.current_session_id.write().await;
            if current.as_ref() == Some(&id.to_string()) {
                *current = None;
            }
        }

        Ok(())
    }

    /// Get session count
    pub async fn count(&self) -> usize {
        self.sessions.read().await.len()
    }
}

impl Default for SessionManager {
    fn default() -> Self {
        Self::new()
    }
}
