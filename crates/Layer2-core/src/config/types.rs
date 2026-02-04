//! Configuration 타입 정의
//!
//! Claude Code 호환 설정 스키마

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use super::workflow::{GitConfig, SecurityConfig};

// ============================================================================
// ForgeConfig - 통합 설정
// ============================================================================

/// ForgeCode 통합 설정 (Claude Code 호환)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ForgeConfig {
    // ========================================================================
    // Provider 설정
    // ========================================================================
    /// API 키 (환경변수 우선)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,

    /// Provider 설정들
    #[serde(default)]
    pub providers: HashMap<String, ProviderConfig>,

    /// 기본 모델
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,

    /// 모델별 설정
    #[serde(default)]
    pub models: HashMap<String, ModelConfig>,

    // ========================================================================
    // MCP 서버 설정
    // ========================================================================
    /// MCP 서버들
    #[serde(default, rename = "mcpServers")]
    pub mcp_servers: HashMap<String, McpServerConfig>,

    // ========================================================================
    // 권한 설정
    // ========================================================================
    /// 권한 설정
    #[serde(default)]
    pub permissions: PermissionConfig,

    // ========================================================================
    // Shell 설정
    // ========================================================================
    /// Shell 설정
    #[serde(default)]
    pub shell: ShellConfigSection,

    // ========================================================================
    // UI/테마 설정
    // ========================================================================
    /// 테마 설정
    #[serde(default)]
    pub theme: ThemeConfig,

    // ========================================================================
    // Git 워크플로우 설정
    // ========================================================================
    /// Git 자동화 설정
    #[serde(default)]
    pub git: GitConfig,

    // ========================================================================
    // 보안 설정
    // ========================================================================
    /// 보안 설정 (환경변수 보호, 경로 제한 등)
    #[serde(default)]
    pub security: SecurityConfig,

    // ========================================================================
    // 일반 설정
    // ========================================================================
    /// 자동 컨텍스트 수집
    #[serde(default = "default_true")]
    pub auto_context: bool,

    /// 스트리밍 출력
    #[serde(default = "default_true")]
    pub streaming: bool,

    /// 히스토리 저장
    #[serde(default = "default_true")]
    pub save_history: bool,

    /// 최대 컨텍스트 토큰
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_context_tokens: Option<u32>,

    /// 응답 최대 토큰
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_response_tokens: Option<u32>,

    // ========================================================================
    // 확장 설정 (임의의 키-값)
    // ========================================================================
    /// 확장 설정
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

fn default_true() -> bool {
    true
}

impl Default for ForgeConfig {
    fn default() -> Self {
        Self {
            api_key: None,
            providers: HashMap::new(),
            model: None,
            models: HashMap::new(),
            mcp_servers: HashMap::new(),
            permissions: PermissionConfig::default(),
            shell: ShellConfigSection::default(),
            theme: ThemeConfig::default(),
            git: GitConfig::default(),
            security: SecurityConfig::default(),
            auto_context: true,
            streaming: true,
            save_history: true,
            max_context_tokens: None,
            max_response_tokens: None,
            extra: HashMap::new(),
        }
    }
}

impl ForgeConfig {
    /// 새 설정 생성
    pub fn new() -> Self {
        Self::default()
    }

    /// API 키 가져오기 (환경변수 우선)
    pub fn get_api_key(&self) -> Option<String> {
        std::env::var("ANTHROPIC_API_KEY")
            .ok()
            .or_else(|| std::env::var("CLAUDE_API_KEY").ok())
            .or_else(|| self.api_key.clone())
    }

    /// 모델 가져오기 (기본값 포함)
    pub fn get_model(&self) -> String {
        self.model
            .clone()
            .unwrap_or_else(|| "claude-sonnet-4-20250514".to_string())
    }

    /// MCP 서버 존재 여부
    pub fn has_mcp_servers(&self) -> bool {
        !self.mcp_servers.is_empty()
    }
}

// ============================================================================
// ProviderConfig - Provider 설정
// ============================================================================

/// Provider 설정
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderConfig {
    /// API 키
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,

    /// 기본 URL
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,

    /// 조직 ID
    #[serde(skip_serializing_if = "Option::is_none")]
    pub organization_id: Option<String>,

    /// 기본 모델
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,

    /// 타임아웃 (초)
    #[serde(default = "default_timeout")]
    pub timeout: u64,
}

fn default_timeout() -> u64 {
    120
}

impl Default for ProviderConfig {
    fn default() -> Self {
        Self {
            api_key: None,
            base_url: None,
            organization_id: None,
            model: None,
            timeout: default_timeout(),
        }
    }
}

// ============================================================================
// ModelConfig - 모델별 설정
// ============================================================================

/// 모델별 설정
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelConfig {
    /// Temperature
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,

    /// Top P
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f32>,

    /// 최대 토큰
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,

    /// 시스템 프롬프트
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system_prompt: Option<String>,
}

// ============================================================================
// McpServerConfig - MCP 서버 설정
// ============================================================================

/// MCP 서버 설정 (Claude Code 호환)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpServerConfig {
    /// 명령어 (stdio transport)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,

    /// 인자
    #[serde(default)]
    pub args: Vec<String>,

    /// 환경 변수
    #[serde(default)]
    pub env: HashMap<String, String>,

    /// URL (SSE/WebSocket transport)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,

    /// 활성화 여부
    #[serde(default = "default_true")]
    pub enabled: bool,

    /// 자동 연결
    #[serde(default)]
    pub auto_connect: bool,

    /// 타임아웃 (초)
    #[serde(default = "default_mcp_timeout")]
    pub timeout: u64,
}

fn default_mcp_timeout() -> u64 {
    30
}

impl McpServerConfig {
    /// Stdio transport인지 확인
    pub fn is_stdio(&self) -> bool {
        self.command.is_some()
    }

    /// SSE transport인지 확인
    pub fn is_sse(&self) -> bool {
        self.url.is_some()
    }
}

impl Default for McpServerConfig {
    fn default() -> Self {
        Self {
            command: None,
            args: Vec::new(),
            env: HashMap::new(),
            url: None,
            enabled: true,
            auto_connect: false,
            timeout: default_mcp_timeout(),
        }
    }
}

// ============================================================================
// PermissionConfig - 권한 설정
// ============================================================================

/// 권한 설정
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PermissionConfig {
    /// 자동 승인 패턴
    #[serde(default)]
    pub auto_approve: Vec<PermissionPattern>,

    /// 항상 거부 패턴
    #[serde(default)]
    pub always_deny: Vec<PermissionPattern>,

    /// 허용된 디렉토리
    #[serde(default)]
    pub allowed_directories: Vec<String>,

    /// 거부된 디렉토리
    #[serde(default)]
    pub denied_directories: Vec<String>,

    /// 위험한 명령어 확인
    #[serde(default = "default_true")]
    pub confirm_dangerous: bool,
}

/// 권한 패턴
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PermissionPattern {
    /// Tool 이름 (또는 "*")
    pub tool: String,

    /// 경로 패턴 (glob)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path_pattern: Option<String>,

    /// 명령어 패턴 (Bash용)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub command_pattern: Option<String>,
}

// ============================================================================
// ShellConfigSection - Shell 설정
// ============================================================================

/// Shell 설정
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShellConfigSection {
    /// 사용할 셸
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shell: Option<String>,

    /// 셸 인자
    #[serde(default)]
    pub shell_args: Vec<String>,

    /// 기본 타임아웃 (초)
    #[serde(default = "default_shell_timeout")]
    pub timeout: u64,

    /// 작업 디렉토리
    #[serde(skip_serializing_if = "Option::is_none")]
    pub working_directory: Option<String>,

    /// 환경 변수
    #[serde(default)]
    pub env: HashMap<String, String>,
}

fn default_shell_timeout() -> u64 {
    120
}

impl Default for ShellConfigSection {
    fn default() -> Self {
        Self {
            shell: None,
            shell_args: Vec::new(),
            timeout: default_shell_timeout(),
            working_directory: None,
            env: HashMap::new(),
        }
    }
}

// ============================================================================
// ThemeConfig - 테마 설정
// ============================================================================

/// 테마 설정
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThemeConfig {
    /// 컬러 스킴
    #[serde(default = "default_color_scheme")]
    pub color_scheme: String,

    /// 코드 하이라이트 테마
    #[serde(skip_serializing_if = "Option::is_none")]
    pub highlight_theme: Option<String>,

    /// 마크다운 렌더링
    #[serde(default = "default_true")]
    pub render_markdown: bool,

    /// 코드 블록 표시
    #[serde(default = "default_true")]
    pub show_code_blocks: bool,
}

fn default_color_scheme() -> String {
    "auto".to_string()
}

impl Default for ThemeConfig {
    fn default() -> Self {
        Self {
            color_scheme: default_color_scheme(),
            highlight_theme: None,
            render_markdown: true,
            show_code_blocks: true,
        }
    }
}

// ============================================================================
// 테스트
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_forge_config_default() {
        let config = ForgeConfig::new();
        assert!(config.api_key.is_none());
        assert!(config.auto_context);
        assert!(config.streaming);
    }

    #[test]
    fn test_forge_config_parse() {
        let json = r#"{
            "apiKey": "test-key",
            "model": "claude-opus-4-5-20251101",
            "mcpServers": {
                "filesystem": {
                    "command": "npx",
                    "args": ["-y", "@anthropic/mcp-server-filesystem", "."]
                }
            }
        }"#;

        let config: ForgeConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.api_key, Some("test-key".to_string()));
        assert_eq!(config.model, Some("claude-opus-4-5-20251101".to_string()));
        assert!(config.mcp_servers.contains_key("filesystem"));
    }

    #[test]
    fn test_mcp_server_config() {
        let json = r#"{
            "command": "node",
            "args": ["server.js"],
            "env": {"DEBUG": "true"}
        }"#;

        let config: McpServerConfig = serde_json::from_str(json).unwrap();
        assert!(config.is_stdio());
        assert!(!config.is_sse());
        assert_eq!(config.command, Some("node".to_string()));
    }

    #[test]
    fn test_permission_config() {
        let json = r#"{
            "autoApprove": [
                {"tool": "Read", "pathPattern": "*.rs"}
            ],
            "alwaysDeny": [
                {"tool": "Bash", "commandPattern": "rm -rf *"}
            ]
        }"#;

        let config: PermissionConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.auto_approve.len(), 1);
        assert_eq!(config.always_deny.len(), 1);
    }

    #[test]
    fn test_get_api_key_from_env() {
        // 환경변수 설정
        std::env::set_var("ANTHROPIC_API_KEY", "env-key");

        let config = ForgeConfig {
            api_key: Some("config-key".to_string()),
            ..Default::default()
        };

        // 환경변수가 우선
        assert_eq!(config.get_api_key(), Some("env-key".to_string()));

        // 정리
        std::env::remove_var("ANTHROPIC_API_KEY");
    }

    #[test]
    fn test_git_config_parse() {
        let json = r#"{
            "git": {
                "workflow": {
                    "autoCommit": {
                        "enabled": true,
                        "messageTemplate": "feat: {description}"
                    },
                    "autoPush": {
                        "enabled": true,
                        "remote": "origin"
                    },
                    "autoTag": {
                        "enabled": true,
                        "pattern": "v{version}"
                    }
                }
            }
        }"#;

        let config: ForgeConfig = serde_json::from_str(json).unwrap();
        assert!(config.git.workflow.auto_commit.enabled);
        assert!(config.git.workflow.auto_push.enabled);
        assert!(config.git.workflow.auto_tag.enabled);
    }

    #[test]
    fn test_security_config_parse() {
        let json = r#"{
            "security": {
                "env": {
                    "blockedPatterns": ["AWS_*", "*_TOKEN", "*_SECRET"],
                    "allowedPatterns": ["PATH", "HOME"]
                },
                "path": {
                    "allowedPaths": ["/home/user/project"],
                    "blockedPaths": ["/etc", "/root"]
                }
            }
        }"#;

        let config: ForgeConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.security.env.blocked_patterns.len(), 3);
        assert_eq!(config.security.env.allowed_patterns.len(), 2);
        assert_eq!(config.security.path.blocked_paths.len(), 2);
    }

    #[test]
    fn test_security_env_filter() {
        use super::workflow::EnvSecurityConfig;

        let security = EnvSecurityConfig {
            blocked_patterns: vec!["AWS_*".to_string(), "*_TOKEN".to_string()],
            allowed_patterns: vec!["PATH".to_string()],
            mask_in_output: true,
        };

        // 차단된 패턴
        assert!(security.is_blocked("AWS_ACCESS_KEY"));
        assert!(security.is_blocked("GITHUB_TOKEN"));

        // 허용된 패턴
        assert!(!security.is_blocked("PATH"));

        // 기본 (차단되지 않음)
        assert!(!security.is_blocked("HOME"));
    }
}
