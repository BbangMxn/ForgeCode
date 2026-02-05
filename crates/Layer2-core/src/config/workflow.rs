//! Workflow Configuration - Git 워크플로우 & 보안 설정
//!
//! Git 자동화와 환경 변수 보호를 위한 설정 타입들입니다.
//!
//! ## 사용 예시
//!
//! ```json
//! {
//!   "git": {
//!     "workflow": {
//!       "autoCommit": { "enabled": true, "messageTemplate": "feat: {description}" },
//!       "autoPush": { "enabled": false, "requireConfirmation": true },
//!       "autoTag": { "enabled": false, "pattern": "v{version}" }
//!     },
//!     "hooks": {
//!       "preCommit": ["cargo fmt", "cargo clippy"],
//!       "prePush": ["cargo test"]
//!     },
//!     "protectedBranches": ["main", "production"],
//!     "conventionalCommits": true
//!   },
//!   "security": {
//!     "env": {
//!       "blocked": ["AWS_*", "*_SECRET", "*_TOKEN"],
//!       "allowed": ["PATH", "HOME", "TERM"],
//!       "maskInOutput": true
//!     },
//!     "paths": {
//!       "allowed": ["./", "/tmp"],
//!       "denied": ["~/.ssh", "~/.aws"]
//!     }
//!   }
//! }
//! ```

use serde::{Deserialize, Serialize};

// ============================================================================
// Git Workflow Configuration
// ============================================================================

/// Git 워크플로우 설정
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GitConfig {
    /// 워크플로우 자동화
    #[serde(default)]
    pub workflow: GitWorkflowConfig,

    /// Git hooks
    #[serde(default)]
    pub hooks: GitHooksConfig,

    /// 보호된 브랜치 (직접 push 금지)
    #[serde(default)]
    pub protected_branches: Vec<String>,

    /// Conventional Commits 강제
    #[serde(default)]
    pub conventional_commits: bool,

    /// 커밋 메시지 검증 패턴
    #[serde(skip_serializing_if = "Option::is_none")]
    pub commit_message_pattern: Option<String>,

    /// 기본 브랜치
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_branch: Option<String>,
}

/// Git 워크플로우 자동화 설정
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GitWorkflowConfig {
    /// 자동 커밋
    #[serde(default)]
    pub auto_commit: AutoCommitConfig,

    /// 자동 푸시
    #[serde(default)]
    pub auto_push: AutoPushConfig,

    /// 자동 태그
    #[serde(default)]
    pub auto_tag: AutoTagConfig,

    /// 자동 스테이지 (add)
    #[serde(default)]
    pub auto_stage: AutoStageConfig,
}

/// 자동 커밋 설정
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AutoCommitConfig {
    /// 활성화 여부
    #[serde(default)]
    pub enabled: bool,

    /// 커밋 메시지 템플릿
    /// - {description}: AI가 생성한 설명
    /// - {files}: 변경된 파일 목록
    /// - {scope}: 스코프 (폴더명 등)
    /// - {type}: 커밋 타입 (feat, fix 등)
    #[serde(default = "default_commit_template")]
    pub message_template: String,

    /// 스코프 포함 여부
    #[serde(default = "default_true")]
    pub include_scope: bool,

    /// 빈 커밋 허용
    #[serde(default)]
    pub allow_empty: bool,

    /// 서명 추가 (-S)
    #[serde(default)]
    pub sign_commits: bool,

    /// 확인 필요 여부
    #[serde(default = "default_true")]
    pub require_confirmation: bool,
}

fn default_commit_template() -> String {
    "{type}: {description}".to_string()
}

fn default_true() -> bool {
    true
}

impl Default for AutoCommitConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            message_template: default_commit_template(),
            include_scope: true,
            allow_empty: false,
            sign_commits: false,
            require_confirmation: true,
        }
    }
}

/// 자동 푸시 설정
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AutoPushConfig {
    /// 활성화 여부
    #[serde(default)]
    pub enabled: bool,

    /// 대상 브랜치 (None이면 현재 브랜치)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,

    /// 리모트 이름
    #[serde(default = "default_remote")]
    pub remote: String,

    /// 확인 필요 여부
    #[serde(default = "default_true")]
    pub require_confirmation: bool,

    /// Force push 허용 (위험!)
    #[serde(default)]
    pub allow_force: bool,

    /// Force with lease 사용
    #[serde(default = "default_true")]
    pub force_with_lease: bool,

    /// 태그도 함께 푸시
    #[serde(default)]
    pub push_tags: bool,
}

fn default_remote() -> String {
    "origin".to_string()
}

impl Default for AutoPushConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            branch: None,
            remote: default_remote(),
            require_confirmation: true,
            allow_force: false,
            force_with_lease: true,
            push_tags: false,
        }
    }
}

/// 자동 태그 설정
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AutoTagConfig {
    /// 활성화 여부
    #[serde(default)]
    pub enabled: bool,

    /// 태그 패턴
    /// - {major}, {minor}, {patch}: 버전 컴포넌트
    /// - {version}: 전체 버전
    /// - {date}: 날짜 (YYYY-MM-DD)
    #[serde(default = "default_tag_pattern")]
    pub pattern: String,

    /// 적용할 브랜치 (빈 배열이면 모든 브랜치)
    #[serde(default)]
    pub on_branches: Vec<String>,

    /// 어노테이션 태그 사용
    #[serde(default = "default_true")]
    pub annotated: bool,

    /// 태그 메시지 템플릿
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message_template: Option<String>,

    /// 확인 필요 여부
    #[serde(default = "default_true")]
    pub require_confirmation: bool,
}

fn default_tag_pattern() -> String {
    "v{version}".to_string()
}

impl Default for AutoTagConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            pattern: default_tag_pattern(),
            on_branches: vec!["main".to_string()],
            annotated: true,
            message_template: None,
            require_confirmation: true,
        }
    }
}

/// 자동 스테이지 설정
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AutoStageConfig {
    /// 활성화 여부
    #[serde(default = "default_true")]
    pub enabled: bool,

    /// 포함할 패턴 (glob)
    #[serde(default)]
    pub include_patterns: Vec<String>,

    /// 제외할 패턴 (glob)
    #[serde(default = "default_exclude_patterns")]
    pub exclude_patterns: Vec<String>,
}

fn default_exclude_patterns() -> Vec<String> {
    vec![
        "*.log".to_string(),
        "*.tmp".to_string(),
        ".env*".to_string(),
        "node_modules/**".to_string(),
        "target/**".to_string(),
    ]
}

impl Default for AutoStageConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            include_patterns: vec![],
            exclude_patterns: default_exclude_patterns(),
        }
    }
}

/// Git Hooks 설정
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GitHooksConfig {
    /// pre-commit 시 실행할 명령어
    #[serde(default)]
    pub pre_commit: Vec<String>,

    /// pre-push 시 실행할 명령어
    #[serde(default)]
    pub pre_push: Vec<String>,

    /// commit-msg 검증 명령어
    #[serde(default)]
    pub commit_msg: Vec<String>,

    /// post-commit 시 실행할 명령어
    #[serde(default)]
    pub post_commit: Vec<String>,

    /// Hook 실패 시 중단 여부
    #[serde(default = "default_true")]
    pub fail_on_error: bool,

    /// Hook 타임아웃 (초)
    #[serde(default = "default_hook_timeout")]
    pub timeout: u64,
}

fn default_hook_timeout() -> u64 {
    60
}

// ============================================================================
// Security Configuration
// ============================================================================

/// 보안 설정
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SecurityConfig {
    /// 환경 변수 보안
    #[serde(default)]
    pub env: EnvSecurityConfig,

    /// 경로 보안
    #[serde(default)]
    pub paths: PathSecurityConfig,

    /// 명령어 보안
    #[serde(default)]
    pub commands: CommandSecurityConfig,

    /// 네트워크 보안
    #[serde(default)]
    pub network: NetworkSecurityConfig,
}

/// 환경 변수 보안 설정
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EnvSecurityConfig {
    /// 차단할 환경 변수 패턴 (glob)
    #[serde(default = "default_blocked_env")]
    pub blocked: Vec<String>,

    /// 허용할 환경 변수 패턴 (glob) - blocked보다 우선
    #[serde(default = "default_allowed_env")]
    pub allowed: Vec<String>,

    /// 출력에서 마스킹
    #[serde(default = "default_true")]
    pub mask_in_output: bool,

    /// 접근 시 경고
    #[serde(default = "default_true")]
    pub warn_on_access: bool,

    /// 마스킹 문자
    #[serde(default = "default_mask_char")]
    pub mask_char: String,
}

fn default_blocked_env() -> Vec<String> {
    vec![
        // AWS
        "AWS_*".to_string(),
        // Azure
        "AZURE_*".to_string(),
        // GCP
        "GCP_*".to_string(),
        "GOOGLE_*".to_string(),
        // 일반 비밀
        "*_SECRET".to_string(),
        "*_SECRET_*".to_string(),
        "*_TOKEN".to_string(),
        "*_TOKEN_*".to_string(),
        "*_KEY".to_string(),
        "*_API_KEY".to_string(),
        "*_PASSWORD".to_string(),
        "*_PRIVATE_*".to_string(),
        // 특정 서비스
        "GITHUB_TOKEN".to_string(),
        "OPENAI_API_KEY".to_string(),
        "ANTHROPIC_API_KEY".to_string(),
        "CLAUDE_API_KEY".to_string(),
        "DATABASE_URL".to_string(),
        "MONGODB_URI".to_string(),
        "REDIS_URL".to_string(),
        // SSH/GPG
        "SSH_*".to_string(),
        "GPG_*".to_string(),
    ]
}

fn default_allowed_env() -> Vec<String> {
    vec![
        // 시스템 기본
        "PATH".to_string(),
        "HOME".to_string(),
        "USER".to_string(),
        "SHELL".to_string(),
        "TERM".to_string(),
        "LANG".to_string(),
        "LC_*".to_string(),
        // 개발 환경
        "NODE_ENV".to_string(),
        "RUST_LOG".to_string(),
        "RUST_BACKTRACE".to_string(),
        "DEBUG".to_string(),
        "VERBOSE".to_string(),
        // 에디터
        "EDITOR".to_string(),
        "VISUAL".to_string(),
        // 프록시 (값은 마스킹)
        "HTTP_PROXY".to_string(),
        "HTTPS_PROXY".to_string(),
        "NO_PROXY".to_string(),
    ]
}

fn default_mask_char() -> String {
    "***".to_string()
}

impl Default for EnvSecurityConfig {
    fn default() -> Self {
        Self {
            blocked: default_blocked_env(),
            allowed: default_allowed_env(),
            mask_in_output: true,
            warn_on_access: true,
            mask_char: default_mask_char(),
        }
    }
}

impl EnvSecurityConfig {
    /// 환경 변수가 차단되었는지 확인
    pub fn is_blocked(&self, name: &str) -> bool {
        // allowed에 있으면 허용
        for pattern in &self.allowed {
            if glob_match(pattern, name) {
                return false;
            }
        }

        // blocked에 있으면 차단
        for pattern in &self.blocked {
            if glob_match(pattern, name) {
                return true;
            }
        }

        false
    }

    /// 환경 변수 값 마스킹
    pub fn mask_value(&self, name: &str, value: &str) -> String {
        if self.mask_in_output && self.is_blocked(name) {
            self.mask_char.clone()
        } else {
            value.to_string()
        }
    }

    /// 환경 변수 필터링 (차단된 것 제거)
    pub fn filter_env(
        &self,
        env: &std::collections::HashMap<String, String>,
    ) -> std::collections::HashMap<String, String> {
        env.iter()
            .filter(|(k, _)| !self.is_blocked(k))
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect()
    }
}

/// 경로 보안 설정
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PathSecurityConfig {
    /// 허용된 경로 (상대/절대)
    #[serde(default = "default_allowed_paths")]
    pub allowed: Vec<String>,

    /// 거부된 경로 (상대/절대)
    #[serde(default = "default_denied_paths")]
    pub denied: Vec<String>,

    /// 심볼릭 링크 따라가기 허용
    #[serde(default)]
    pub follow_symlinks: bool,

    /// 상위 디렉토리 접근 허용 (../)
    #[serde(default)]
    pub allow_parent_traversal: bool,
}

fn default_allowed_paths() -> Vec<String> {
    vec!["./".to_string(), "/tmp".to_string()]
}

fn default_denied_paths() -> Vec<String> {
    vec![
        "~/.ssh".to_string(),
        "~/.aws".to_string(),
        "~/.gnupg".to_string(),
        "~/.config/gcloud".to_string(),
        "/etc/passwd".to_string(),
        "/etc/shadow".to_string(),
        "/etc/sudoers".to_string(),
    ]
}

impl Default for PathSecurityConfig {
    fn default() -> Self {
        Self {
            allowed: default_allowed_paths(),
            denied: default_denied_paths(),
            follow_symlinks: false,
            allow_parent_traversal: false,
        }
    }
}

/// 명령어 보안 설정
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandSecurityConfig {
    /// 확인이 필요한 명령어 패턴
    #[serde(default = "default_dangerous_commands")]
    pub require_confirmation: Vec<String>,

    /// 완전히 차단된 명령어 패턴
    #[serde(default = "default_blocked_commands")]
    pub blocked: Vec<String>,

    /// 위험도 임계값 (이 이상이면 확인 필요)
    #[serde(default = "default_risk_threshold")]
    pub risk_threshold: u8,
}

fn default_dangerous_commands() -> Vec<String> {
    vec![
        "rm -rf *".to_string(),
        "git push --force".to_string(),
        "git reset --hard".to_string(),
        "chmod -R 777".to_string(),
        "dd if=*".to_string(),
        "mkfs*".to_string(),
        ":(){ :|:& };:".to_string(),
    ]
}

fn default_blocked_commands() -> Vec<String> {
    vec![
        "rm -rf /".to_string(),
        "rm -rf /*".to_string(),
        "> /dev/sd*".to_string(),
        ":(){ :|:& };:".to_string(),
    ]
}

fn default_risk_threshold() -> u8 {
    7
}

impl Default for CommandSecurityConfig {
    fn default() -> Self {
        Self {
            require_confirmation: default_dangerous_commands(),
            blocked: default_blocked_commands(),
            risk_threshold: default_risk_threshold(),
        }
    }
}

/// 네트워크 보안 설정
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NetworkSecurityConfig {
    /// 허용된 도메인 (빈 배열이면 모두 허용)
    #[serde(default)]
    pub allowed_domains: Vec<String>,

    /// 차단된 도메인
    #[serde(default)]
    pub blocked_domains: Vec<String>,

    /// 외부 네트워크 요청 시 확인
    #[serde(default)]
    pub confirm_external: bool,

    /// curl | sh 패턴 차단
    #[serde(default = "default_true")]
    pub block_pipe_to_shell: bool,
}

impl Default for NetworkSecurityConfig {
    fn default() -> Self {
        Self {
            allowed_domains: vec![],
            blocked_domains: vec![],
            confirm_external: false,
            block_pipe_to_shell: true,
        }
    }
}

// ============================================================================
// 헬퍼 함수
// ============================================================================

/// 간단한 glob 패턴 매칭
pub fn glob_match(pattern: &str, value: &str) -> bool {
    if pattern == "*" {
        return true;
    }

    let pattern_lower = pattern.to_lowercase();
    let value_lower = value.to_lowercase();

    if pattern_lower.starts_with('*') && pattern_lower.ends_with('*') {
        // *TOKEN* 패턴
        let inner = &pattern_lower[1..pattern_lower.len() - 1];
        value_lower.contains(inner)
    } else if pattern_lower.starts_with('*') {
        // *_TOKEN 패턴
        let suffix = &pattern_lower[1..];
        value_lower.ends_with(suffix)
    } else if pattern_lower.ends_with('*') {
        // AWS_* 패턴
        let prefix = &pattern_lower[..pattern_lower.len() - 1];
        value_lower.starts_with(prefix)
    } else {
        // 정확히 일치
        pattern_lower == value_lower
    }
}

// ============================================================================
// 테스트
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_glob_match() {
        assert!(glob_match("AWS_*", "AWS_SECRET_KEY"));
        assert!(glob_match("*_TOKEN", "GITHUB_TOKEN"));
        assert!(glob_match("*SECRET*", "MY_SECRET_KEY"));
        assert!(!glob_match("AWS_*", "AZURE_SECRET"));
        assert!(glob_match("*", "ANYTHING"));
    }

    #[test]
    fn test_env_security_blocked() {
        let config = EnvSecurityConfig::default();

        assert!(config.is_blocked("AWS_SECRET_KEY"));
        assert!(config.is_blocked("GITHUB_TOKEN"));
        assert!(config.is_blocked("DATABASE_URL"));
        assert!(!config.is_blocked("PATH"));
        assert!(!config.is_blocked("HOME"));
        assert!(!config.is_blocked("NODE_ENV"));
    }

    #[test]
    fn test_env_security_mask() {
        let config = EnvSecurityConfig::default();

        assert_eq!(config.mask_value("AWS_SECRET_KEY", "secret123"), "***");
        assert_eq!(config.mask_value("PATH", "/usr/bin"), "/usr/bin");
    }

    #[test]
    fn test_git_config_default() {
        let config = GitConfig::default();

        assert!(!config.workflow.auto_commit.enabled);
        assert!(!config.workflow.auto_push.enabled);
        assert!(!config.workflow.auto_tag.enabled);
        assert!(config.protected_branches.is_empty());
    }

    #[test]
    fn test_git_config_parse() {
        let json = r#"{
            "workflow": {
                "autoCommit": {
                    "enabled": true,
                    "messageTemplate": "feat({scope}): {description}"
                },
                "autoPush": {
                    "enabled": false,
                    "requireConfirmation": true
                }
            },
            "protectedBranches": ["main", "production"],
            "conventionalCommits": true
        }"#;

        let config: GitConfig = serde_json::from_str(json).unwrap();
        assert!(config.workflow.auto_commit.enabled);
        assert!(!config.workflow.auto_push.enabled);
        assert!(config.workflow.auto_push.require_confirmation);
        assert_eq!(config.protected_branches.len(), 2);
        assert!(config.conventional_commits);
    }

    #[test]
    fn test_security_config_parse() {
        let json = r#"{
            "env": {
                "blocked": ["CUSTOM_SECRET", "MY_*"],
                "allowed": ["MY_PUBLIC_VAR"],
                "maskInOutput": true
            },
            "paths": {
                "allowed": ["./src", "./tests"],
                "denied": ["./secrets"]
            }
        }"#;

        let config: SecurityConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.env.blocked.len(), 2);
        assert_eq!(config.env.allowed.len(), 1);
        assert!(config.env.mask_in_output);
    }
}
