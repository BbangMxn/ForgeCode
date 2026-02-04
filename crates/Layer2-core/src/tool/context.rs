//! Tool Context - 도구 실행 컨텍스트
//!
//! Layer1 ToolContext trait 구현
//! - PermissionService 연동
//! - PermissionDelegate 연동 (대화형 권한 승인)
//! - ShellConfig 연동
//!
//! ## 대화형 권한 승인
//!
//! `PermissionDelegate`를 설정하면 권한이 필요할 때 UI에 프롬프트를 표시합니다.
//!
//! ```ignore
//! let delegate = Arc::new(MyCliDelegate::new());
//! let ctx = RuntimeContext::new(session_id, working_dir, permissions)
//!     .with_permission_delegate(delegate);
//!
//! // 권한 요청 시 delegate.request_permission()이 호출됨
//! let result = tool.execute(input, &ctx).await?;
//! ```

use async_trait::async_trait;
use forge_foundation::{
    PermissionAction, PermissionDelegate, PermissionResponse,
    PermissionService, PermissionStatus, Result, ShellConfig, ShellType, ToolContext,
};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tracing::debug;

// ============================================================================
// DefaultShellConfig - 기본 Shell 설정
// ============================================================================

/// 기본 Shell 설정 구현
pub struct DefaultShellConfig {
    shell_type: ShellType,
    executable: String,
    timeout_secs: u64,
    env_vars: HashMap<String, String>,
    working_dir: Option<PathBuf>,
}

impl DefaultShellConfig {
    /// 현재 OS 기본 설정으로 생성
    pub fn new() -> Self {
        let shell_type = ShellType::default_for_os();
        Self {
            executable: shell_type.executable().to_string(),
            shell_type,
            timeout_secs: 120, // 2분 기본
            env_vars: HashMap::new(),
            working_dir: None,
        }
    }

    /// Shell 타입 지정
    pub fn with_shell_type(mut self, shell_type: ShellType) -> Self {
        self.shell_type = shell_type;
        self.executable = shell_type.executable().to_string();
        self
    }

    /// 타임아웃 설정
    pub fn with_timeout(mut self, secs: u64) -> Self {
        self.timeout_secs = secs;
        self
    }

    /// 작업 디렉토리 설정
    pub fn with_working_dir(mut self, dir: PathBuf) -> Self {
        self.working_dir = Some(dir);
        self
    }

    /// 환경 변수 추가
    pub fn with_env(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.env_vars.insert(key.into(), value.into());
        self
    }
}

impl Default for DefaultShellConfig {
    fn default() -> Self {
        Self::new()
    }
}

impl ShellConfig for DefaultShellConfig {
    fn shell_type(&self) -> ShellType {
        self.shell_type
    }

    fn executable(&self) -> &str {
        &self.executable
    }

    fn exec_args(&self) -> Vec<String> {
        self.shell_type.exec_args().iter().map(|s| s.to_string()).collect()
    }

    fn env_vars(&self) -> HashMap<String, String> {
        self.env_vars.clone()
    }

    fn timeout_secs(&self) -> u64 {
        self.timeout_secs
    }

    fn working_dir(&self) -> Option<&Path> {
        self.working_dir.as_deref()
    }
}

// ============================================================================
// RuntimeContext - Layer1 ToolContext 구현
// ============================================================================

/// 런타임 컨텍스트 - Layer1 ToolContext 구현
///
/// 도구 실행에 필요한 모든 환경을 제공합니다:
/// - 작업 디렉토리
/// - 세션 정보
/// - 환경 변수
/// - 권한 서비스
/// - 권한 델리게이트 (대화형 권한 승인)
/// - Shell 설정
pub struct RuntimeContext {
    session_id: String,
    working_dir: PathBuf,
    env: HashMap<String, String>,
    permissions: Arc<PermissionService>,
    permission_delegate: Option<Arc<dyn PermissionDelegate>>,
    shell_config: Box<dyn ShellConfig>,
}

impl RuntimeContext {
    /// 새 컨텍스트 생성
    pub fn new(
        session_id: impl Into<String>,
        working_dir: PathBuf,
        permissions: Arc<PermissionService>,
    ) -> Self {
        Self {
            session_id: session_id.into(),
            working_dir: working_dir.clone(),
            env: std::env::vars().collect(),
            permissions,
            permission_delegate: None,
            shell_config: Box::new(DefaultShellConfig::new().with_working_dir(working_dir)),
        }
    }

    /// Shell 설정 커스텀
    pub fn with_shell_config(mut self, config: impl ShellConfig + 'static) -> Self {
        self.shell_config = Box::new(config);
        self
    }

    /// 환경 변수 추가
    pub fn with_env(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.env.insert(key.into(), value.into());
        self
    }

    /// 권한 델리게이트 설정 (대화형 권한 승인용)
    ///
    /// 델리게이트가 설정되면 권한이 Unknown 상태일 때
    /// UI를 통해 사용자에게 권한 승인을 요청합니다.
    pub fn with_permission_delegate(mut self, delegate: Arc<dyn PermissionDelegate>) -> Self {
        self.permission_delegate = Some(delegate);
        self
    }

    /// 권한 서비스 접근
    pub fn permission_service(&self) -> &PermissionService {
        &self.permissions
    }

    /// 권한 델리게이트 접근
    pub fn permission_delegate(&self) -> Option<&Arc<dyn PermissionDelegate>> {
        self.permission_delegate.as_ref()
    }

    /// 권한을 직접 부여 (세션 범위)
    pub fn grant_session(&self, tool_name: &str, action: PermissionAction) {
        self.permissions.grant_session(tool_name, action);
    }
}

#[async_trait]
impl ToolContext for RuntimeContext {
    fn working_dir(&self) -> &Path {
        &self.working_dir
    }

    fn session_id(&self) -> &str {
        &self.session_id
    }

    fn env(&self) -> &HashMap<String, String> {
        &self.env
    }

    async fn check_permission(&self, tool: &str, action: &PermissionAction) -> PermissionStatus {
        self.permissions.check(tool, action)
    }

    async fn request_permission(
        &self,
        tool: &str,
        description: &str,
        action: PermissionAction,
    ) -> Result<bool> {
        // 1. 이미 허용된 경우 바로 반환
        let status = self.permissions.check(tool, &action);
        match status {
            PermissionStatus::Granted | PermissionStatus::AutoApproved => {
                return Ok(true);
            }
            PermissionStatus::Denied => {
                return Ok(false);
            }
            PermissionStatus::Unknown => {
                // 계속해서 delegate에 요청
            }
        }

        // 2. Delegate가 있으면 대화형 권한 승인 요청
        if let Some(ref delegate) = self.permission_delegate {
            debug!("Requesting permission via delegate for {}: {:?}", tool, action);

            // 위험도 계산 (action 타입에 따라)
            let risk_score = match &action {
                PermissionAction::Execute { .. } => 7,
                PermissionAction::FileDelete { .. } => 8,
                PermissionAction::FileWrite { .. } => 5,
                PermissionAction::FileReadSensitive { .. } => 4,
                PermissionAction::Network { .. } => 6,
                PermissionAction::Custom { .. } => 5,
            };

            let response = delegate
                .request_permission(tool, &action, description, risk_score)
                .await;

            match response {
                PermissionResponse::AllowOnce => {
                    // 1회만 허용 - 저장하지 않고 true 반환
                    debug!("Permission granted once for {}", tool);
                    return Ok(true);
                }
                PermissionResponse::AllowSession => {
                    // 세션 동안 허용
                    self.permissions.grant_session(tool, action);
                    debug!("Permission granted for session for {}", tool);
                    return Ok(true);
                }
                PermissionResponse::AllowPermanent => {
                    // 영구 허용
                    if let Err(e) = self.permissions.grant_permanent(tool, action) {
                        debug!("Failed to save permanent permission: {}", e);
                        // 저장 실패해도 일단 허용
                    }
                    debug!("Permission granted permanently for {}", tool);
                    return Ok(true);
                }
                PermissionResponse::Deny => {
                    debug!("Permission denied for {}", tool);
                    return Ok(false);
                }
                PermissionResponse::DenyPermanent => {
                    // TODO: 영구 거부 저장
                    debug!("Permission denied permanently for {}", tool);
                    return Ok(false);
                }
            }
        }

        // 3. Delegate가 없으면 기존 권한 상태만 확인
        Ok(self.permissions.is_permitted(tool, &action))
    }

    fn shell_config(&self) -> &dyn ShellConfig {
        self.shell_config.as_ref()
    }
}

// ============================================================================
// 테스트
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_shell_config() {
        let config = DefaultShellConfig::new();

        #[cfg(target_os = "windows")]
        assert_eq!(config.shell_type(), ShellType::PowerShell);

        #[cfg(target_os = "macos")]
        assert_eq!(config.shell_type(), ShellType::Zsh);

        assert_eq!(config.timeout_secs(), 120);
    }

    #[test]
    fn test_shell_config_builder() {
        let config = DefaultShellConfig::new()
            .with_shell_type(ShellType::Bash)
            .with_timeout(60)
            .with_env("MY_VAR", "value");

        assert_eq!(config.shell_type(), ShellType::Bash);
        assert_eq!(config.timeout_secs(), 60);
        assert_eq!(config.env_vars().get("MY_VAR"), Some(&"value".to_string()));
    }

    #[tokio::test]
    async fn test_runtime_context() {
        let permissions = Arc::new(PermissionService::new());
        let ctx = RuntimeContext::new(
            "test-session",
            PathBuf::from("/tmp"),
            permissions,
        );

        assert_eq!(ctx.session_id(), "test-session");
        assert_eq!(ctx.working_dir(), Path::new("/tmp"));
    }
}
