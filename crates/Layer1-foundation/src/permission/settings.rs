//! Permission 설정 저장/로드
//!
//! 영구 권한(permanent grants)을 JSON으로 관리

use super::service::{Permission, PermissionAction, PermissionScope};
use crate::storage::JsonStore;
use crate::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// 설정 파일명
pub const PERMISSIONS_FILE: &str = "permissions.json";

/// Permission 설정 파일 구조
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PermissionSettings {
    /// 영구 허용된 권한들
    #[serde(default)]
    pub grants: HashSet<PermissionGrant>,

    /// 항상 거부할 패턴들
    #[serde(default)]
    pub denies: HashSet<PermissionDeny>,

    /// 자동 승인 모드
    #[serde(default)]
    pub auto_approve: bool,

    /// 자동 승인할 도구들
    #[serde(default)]
    pub auto_approve_tools: HashSet<String>,
}

/// 저장용 권한 구조
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "camelCase")]
pub struct PermissionGrant {
    /// 도구 이름 (예: "bash", "file_write")
    pub tool: String,

    /// 액션 타입
    pub action_type: PermissionActionType,

    /// 패턴 (glob 지원, 예: "/home/user/project/**")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pattern: Option<String>,
}

/// 거부 패턴
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "camelCase")]
pub struct PermissionDeny {
    /// 도구 이름
    pub tool: String,

    /// 패턴
    pub pattern: String,

    /// 이유
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// 액션 타입 (저장용 간소화 버전)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum PermissionActionType {
    Execute,
    FileWrite,
    FileDelete,
    FileRead,
    Network,
    Custom(String),
}

impl From<&PermissionAction> for PermissionActionType {
    fn from(action: &PermissionAction) -> Self {
        match action {
            PermissionAction::Execute { .. } => Self::Execute,
            PermissionAction::FileWrite { .. } => Self::FileWrite,
            PermissionAction::FileDelete { .. } => Self::FileDelete,
            PermissionAction::FileReadSensitive { .. } => Self::FileRead,
            PermissionAction::Network { .. } => Self::Network,
            PermissionAction::Custom { name, .. } => Self::Custom(name.clone()),
        }
    }
}

impl PermissionSettings {
    pub fn new() -> Self {
        Self::default()
    }

    /// 글로벌 설정 로드
    pub fn load_global() -> Result<Self> {
        let store = JsonStore::global()?;
        Ok(store.load_or_default(PERMISSIONS_FILE))
    }

    /// 프로젝트 설정 로드
    pub fn load_project() -> Result<Self> {
        let store = JsonStore::current_project()?;
        Ok(store.load_or_default(PERMISSIONS_FILE))
    }

    /// 글로벌 + 프로젝트 병합 로드
    pub fn load() -> Result<Self> {
        let mut settings = Self::load_global().unwrap_or_default();
        if let Ok(project) = Self::load_project() {
            settings.merge(project);
        }
        Ok(settings)
    }

    /// 글로벌 설정 저장
    pub fn save_global(&self) -> Result<()> {
        let store = JsonStore::global()?;
        store.save(PERMISSIONS_FILE, self)
    }

    /// 프로젝트 설정 저장
    pub fn save_project(&self) -> Result<()> {
        let store = JsonStore::current_project()?;
        store.save(PERMISSIONS_FILE, self)
    }

    /// 권한 추가
    pub fn add_grant(&mut self, grant: PermissionGrant) {
        self.grants.insert(grant);
    }

    /// Permission에서 Grant 생성 및 추가
    pub fn add_permission(&mut self, permission: &Permission) {
        let grant = PermissionGrant {
            tool: permission.tool_name.clone(),
            action_type: PermissionActionType::from(&permission.action),
            pattern: Self::extract_pattern(&permission.action),
        };
        self.grants.insert(grant);
    }

    /// 거부 패턴 추가
    pub fn add_deny(&mut self, deny: PermissionDeny) {
        self.denies.insert(deny);
    }

    /// 권한 확인
    pub fn is_granted(&self, tool: &str, action: &PermissionAction) -> bool {
        let action_type = PermissionActionType::from(action);
        let pattern = Self::extract_pattern(action);

        for grant in &self.grants {
            if grant.tool == tool && grant.action_type == action_type {
                if let (Some(grant_pattern), Some(ref action_pattern)) = (&grant.pattern, &pattern)
                {
                    if Self::pattern_matches(grant_pattern, action_pattern) {
                        return true;
                    }
                } else if grant.pattern.is_none() {
                    return true;
                }
            }
        }

        false
    }

    /// 거부 확인
    pub fn is_denied(&self, tool: &str, action: &PermissionAction) -> bool {
        let pattern = Self::extract_pattern(action);

        for deny in &self.denies {
            if deny.tool == tool {
                if let Some(ref action_pattern) = pattern {
                    if Self::pattern_matches(&deny.pattern, action_pattern) {
                        return true;
                    }
                }
            }
        }

        false
    }

    /// 자동 승인 도구 확인
    pub fn is_auto_approved(&self, tool: &str) -> bool {
        self.auto_approve || self.auto_approve_tools.contains(tool)
    }

    /// 병합
    pub fn merge(&mut self, other: PermissionSettings) {
        self.grants.extend(other.grants);
        self.denies.extend(other.denies);
        self.auto_approve = self.auto_approve || other.auto_approve;
        self.auto_approve_tools.extend(other.auto_approve_tools);
    }

    /// HashSet<Permission>으로 변환
    pub fn to_permissions(&self) -> HashSet<Permission> {
        self.grants
            .iter()
            .filter_map(|grant| {
                let action = Self::grant_to_action(grant)?;
                Some(Permission {
                    tool_name: grant.tool.clone(),
                    action,
                    scope: PermissionScope::Permanent,
                })
            })
            .collect()
    }

    // === Helper functions ===

    fn extract_pattern(action: &PermissionAction) -> Option<String> {
        match action {
            PermissionAction::Execute { command } => Some(command.clone()),
            PermissionAction::FileWrite { path } => Some(path.clone()),
            PermissionAction::FileDelete { path } => Some(path.clone()),
            PermissionAction::FileReadSensitive { path } => Some(path.clone()),
            PermissionAction::Network { url } => Some(url.clone()),
            PermissionAction::Custom { details, .. } => Some(details.clone()),
        }
    }

    fn grant_to_action(grant: &PermissionGrant) -> Option<PermissionAction> {
        let pattern = grant.pattern.clone().unwrap_or_default();
        Some(match &grant.action_type {
            PermissionActionType::Execute => PermissionAction::Execute { command: pattern },
            PermissionActionType::FileWrite => PermissionAction::FileWrite { path: pattern },
            PermissionActionType::FileDelete => PermissionAction::FileDelete { path: pattern },
            PermissionActionType::FileRead => PermissionAction::FileReadSensitive { path: pattern },
            PermissionActionType::Network => PermissionAction::Network { url: pattern },
            PermissionActionType::Custom(name) => PermissionAction::Custom {
                name: name.clone(),
                details: pattern,
            },
        })
    }

    fn pattern_matches(pattern: &str, value: &str) -> bool {
        if pattern == "**" || pattern == "*" {
            return true;
        }
        if pattern.ends_with("/**") {
            let prefix = &pattern[..pattern.len() - 3];
            return value.starts_with(prefix);
        }
        if pattern.ends_with("/*") {
            let prefix = &pattern[..pattern.len() - 2];
            return value.starts_with(prefix) && !value[prefix.len()..].contains('/');
        }
        pattern == value
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_permission_grant() {
        let mut settings = PermissionSettings::new();

        settings.add_grant(PermissionGrant {
            tool: "bash".to_string(),
            action_type: PermissionActionType::Execute,
            pattern: Some("ls **".to_string()),
        });

        settings.add_grant(PermissionGrant {
            tool: "file_write".to_string(),
            action_type: PermissionActionType::FileWrite,
            pattern: Some("/home/user/project/**".to_string()),
        });

        assert!(settings.is_granted(
            "file_write",
            &PermissionAction::FileWrite {
                path: "/home/user/project/src/main.rs".to_string()
            }
        ));

        assert!(!settings.is_granted(
            "file_write",
            &PermissionAction::FileWrite {
                path: "/etc/passwd".to_string()
            }
        ));
    }

    #[test]
    fn test_permission_deny() {
        let mut settings = PermissionSettings::new();

        settings.add_deny(PermissionDeny {
            tool: "bash".to_string(),
            pattern: "rm -rf /**".to_string(),
            reason: Some("Dangerous command".to_string()),
        });

        assert!(settings.is_denied(
            "bash",
            &PermissionAction::Execute {
                command: "rm -rf /".to_string()
            }
        ));
    }
}
