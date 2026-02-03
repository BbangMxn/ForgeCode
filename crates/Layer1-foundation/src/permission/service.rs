//! Permission service for ForgeCode
//!
//! Manages runtime permission grants and integrates with persistent storage.
//! This is a pure data management layer - UI/CLI interaction is handled elsewhere.

use super::settings::PermissionSettings;
use crate::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::sync::RwLock;

/// Types of permission actions
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum PermissionAction {
    /// Execute a shell command
    Execute { command: String },

    /// Write to a file
    FileWrite { path: String },

    /// Delete a file
    FileDelete { path: String },

    /// Read a sensitive file
    FileReadSensitive { path: String },

    /// Network request
    Network { url: String },

    /// Custom action
    Custom { name: String, details: String },
}

impl PermissionAction {
    /// Get a human-readable description
    pub fn description(&self) -> String {
        match self {
            Self::Execute { command } => format!("Execute: {}", command),
            Self::FileWrite { path } => format!("Write file: {}", path),
            Self::FileDelete { path } => format!("Delete file: {}", path),
            Self::FileReadSensitive { path } => format!("Read sensitive: {}", path),
            Self::Network { url } => format!("Network: {}", url),
            Self::Custom { name, details } => format!("{}: {}", name, details),
        }
    }
}

/// A granted permission
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct Permission {
    pub tool_name: String,
    pub action: PermissionAction,
    pub scope: PermissionScope,
}

/// Scope of a granted permission
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum PermissionScope {
    /// Valid for current request only
    Once,

    /// Valid for current session
    Session,

    /// Saved permanently
    Permanent,
}

/// Permission check result
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PermissionStatus {
    /// Permission granted
    Granted,

    /// Permission denied (in deny list)
    Denied,

    /// Permission not found - needs user decision
    Unknown,

    /// Auto-approved (tool or global auto-approve)
    AutoApproved,
}

/// Permission service managing grants and queries
///
/// This service handles:
/// - Session grants (in-memory, cleared on restart)
/// - Permanent grants (loaded from/saved to JSON storage)
/// - Permission checking and granting
pub struct PermissionService {
    /// Session grants (in-memory only)
    session_grants: RwLock<HashSet<Permission>>,

    /// Persistent settings (loaded from storage)
    settings: RwLock<PermissionSettings>,
}

impl PermissionService {
    /// Create a new permission service with default settings
    pub fn new() -> Self {
        Self {
            session_grants: RwLock::new(HashSet::new()),
            settings: RwLock::new(PermissionSettings::default()),
        }
    }

    /// Create with loaded settings
    pub fn with_settings(settings: PermissionSettings) -> Self {
        Self {
            session_grants: RwLock::new(HashSet::new()),
            settings: RwLock::new(settings),
        }
    }

    /// Load settings from storage (global + project merged)
    pub fn load() -> Result<Self> {
        let settings = PermissionSettings::load()?;
        Ok(Self::with_settings(settings))
    }

    /// Create with auto-approve enabled for all tools
    pub fn with_auto_approve() -> Self {
        let mut settings = PermissionSettings::default();
        settings.auto_approve = true;
        Self::with_settings(settings)
    }

    /// Check permission status for an action
    pub fn check(&self, tool_name: &str, action: &PermissionAction) -> PermissionStatus {
        // 1. Check deny list first
        if let Ok(settings) = self.settings.read() {
            if settings.is_denied(tool_name, action) {
                return PermissionStatus::Denied;
            }

            // 2. Check auto-approve
            if settings.is_auto_approved(tool_name) {
                return PermissionStatus::AutoApproved;
            }

            // 3. Check permanent grants
            if settings.is_granted(tool_name, action) {
                return PermissionStatus::Granted;
            }
        }

        // 4. Check session grants
        if let Ok(grants) = self.session_grants.read() {
            for grant in grants.iter() {
                if grant.tool_name == tool_name && &grant.action == action {
                    return PermissionStatus::Granted;
                }
            }
        }

        PermissionStatus::Unknown
    }

    /// Check if an action is permitted (convenience method)
    pub fn is_permitted(&self, tool_name: &str, action: &PermissionAction) -> bool {
        matches!(
            self.check(tool_name, action),
            PermissionStatus::Granted | PermissionStatus::AutoApproved
        )
    }

    /// Grant a permission
    pub fn grant(&self, permission: Permission) {
        match permission.scope {
            PermissionScope::Once => {
                // Once permissions are not stored
            }
            PermissionScope::Session => {
                if let Ok(mut grants) = self.session_grants.write() {
                    grants.insert(permission);
                }
            }
            PermissionScope::Permanent => {
                if let Ok(mut settings) = self.settings.write() {
                    settings.add_permission(&permission);
                }
            }
        }
    }

    /// Grant and save permanent permission
    pub fn grant_permanent(&self, tool_name: &str, action: PermissionAction) -> Result<()> {
        let permission = Permission {
            tool_name: tool_name.to_string(),
            action,
            scope: PermissionScope::Permanent,
        };

        if let Ok(mut settings) = self.settings.write() {
            settings.add_permission(&permission);
            settings.save_global()?;
        }

        Ok(())
    }

    /// Grant session permission
    pub fn grant_session(&self, tool_name: &str, action: PermissionAction) {
        let permission = Permission {
            tool_name: tool_name.to_string(),
            action,
            scope: PermissionScope::Session,
        };

        if let Ok(mut grants) = self.session_grants.write() {
            grants.insert(permission);
        }
    }

    /// Clear all session grants
    pub fn clear_session(&self) {
        if let Ok(mut grants) = self.session_grants.write() {
            grants.clear();
        }
    }

    /// Get all session grants
    pub fn session_grants(&self) -> Vec<Permission> {
        self.session_grants
            .read()
            .map(|g| g.iter().cloned().collect())
            .unwrap_or_default()
    }

    /// Get all permanent grants
    pub fn permanent_grants(&self) -> Vec<Permission> {
        self.settings
            .read()
            .map(|s| s.to_permissions().into_iter().collect())
            .unwrap_or_default()
    }

    /// Check if auto-approve is enabled for a tool
    pub fn is_auto_approved(&self, tool_name: &str) -> bool {
        self.settings
            .read()
            .map(|s| s.is_auto_approved(tool_name))
            .unwrap_or(false)
    }

    /// Enable auto-approve for a tool
    pub fn set_auto_approve_tool(&self, tool_name: &str, enabled: bool) -> Result<()> {
        if let Ok(mut settings) = self.settings.write() {
            if enabled {
                settings.auto_approve_tools.insert(tool_name.to_string());
            } else {
                settings.auto_approve_tools.remove(tool_name);
            }
            settings.save_global()?;
        }
        Ok(())
    }

    /// Save current settings
    pub fn save(&self) -> Result<()> {
        if let Ok(settings) = self.settings.read() {
            settings.save_global()?;
        }
        Ok(())
    }

    /// Reload settings from storage
    pub fn reload(&self) -> Result<()> {
        let new_settings = PermissionSettings::load()?;
        if let Ok(mut settings) = self.settings.write() {
            *settings = new_settings;
        }
        Ok(())
    }

    /// Request permission for an action
    ///
    /// This method checks if permission is already granted, or if auto-approve is enabled.
    /// If permission status is Unknown, the caller (CLI layer) should prompt the user.
    ///
    /// Returns:
    /// - Ok(true) if permitted (granted or auto-approved)
    /// - Ok(false) if denied
    /// - Err if permission is unknown and needs user interaction
    ///
    /// Note: For full UI interaction, the CLI layer should:
    /// 1. Call check() first
    /// 2. If Unknown, prompt the user
    /// 3. Call grant_session() or grant_permanent() based on user response
    pub async fn request(
        &self,
        _session_id: &str,
        tool_name: &str,
        _description: &str,
        action: PermissionAction,
    ) -> Result<bool> {
        match self.check(tool_name, &action) {
            PermissionStatus::Granted | PermissionStatus::AutoApproved => Ok(true),
            PermissionStatus::Denied => Ok(false),
            PermissionStatus::Unknown => {
                // In a headless/non-interactive context, we could:
                // 1. Deny by default (safe)
                // 2. Allow by default (dangerous)
                // 3. Return an error indicating user interaction needed
                //
                // For now, we return an error so the CLI can handle it
                Err(crate::Error::PermissionDenied(format!(
                    "Permission required for {}: {}",
                    tool_name,
                    action.description()
                )))
            }
        }
    }
}

impl Default for PermissionService {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_grant() {
        let service = PermissionService::new();

        let action = PermissionAction::FileWrite {
            path: "/tmp/test.txt".to_string(),
        };

        // Initially not permitted
        assert!(!service.is_permitted("file_write", &action));

        // Grant session permission
        service.grant_session("file_write", action.clone());

        // Now permitted
        assert!(service.is_permitted("file_write", &action));

        // Clear session
        service.clear_session();

        // No longer permitted
        assert!(!service.is_permitted("file_write", &action));
    }

    #[test]
    fn test_permission_status() {
        let service = PermissionService::new();

        let action = PermissionAction::Execute {
            command: "ls".to_string(),
        };

        // Initially unknown
        assert_eq!(service.check("bash", &action), PermissionStatus::Unknown);

        // Grant session
        service.grant_session("bash", action.clone());

        // Now granted
        assert_eq!(service.check("bash", &action), PermissionStatus::Granted);
    }
}
