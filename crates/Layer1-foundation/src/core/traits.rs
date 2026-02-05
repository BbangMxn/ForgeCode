//! Core Traits - 핵심 인터페이스 정의
//!
//! Layer2 이상에서 구현해야 하는 핵심 trait들을 정의합니다.
//! macOS 스타일의 계층화된 설계로 플러그인 등록이 용이합니다.
//!
//! ## 아키텍처
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────┐
//! │  Layer4-CLI/TUI                                             │
//! │  ├── PermissionDelegate 구현 (UI 프롬프트)                   │
//! │  └── TaskObserver 구현 (진행상황 표시)                       │
//! ├─────────────────────────────────────────────────────────────┤
//! │  Layer3-Task/Agent                                          │
//! │  ├── Task 독립 실행 (병렬 프로그래밍)                         │
//! │  └── Agent 루프 관리                                         │
//! ├─────────────────────────────────────────────────────────────┤
//! │  Layer2-Tool/Provider                                       │
//! │  ├── Tool trait 구현 (Bash, Read, Write 등)                 │
//! │  ├── Provider trait 구현 (Anthropic, OpenAI 등)             │
//! │  └── MCP 서버 → 전용 Shell로 권한 설정된 실행                │
//! ├─────────────────────────────────────────────────────────────┤
//! │  Layer1-Foundation (이 레이어)                              │
//! │  ├── Trait 정의 (Tool, Provider, Configurable)              │
//! │  ├── Permission 관리 (등록, 검사, 저장)                      │
//! │  ├── Shell 설정 (bash, powershell, cmd)                     │
//! │  └── Registry (MCP, Provider, Model)                        │
//! └─────────────────────────────────────────────────────────────┘
//! ```

use crate::permission::{PermissionAction, PermissionDef, PermissionStatus};
use crate::Result;
use async_trait::async_trait;
use serde::{de::DeserializeOwned, Serialize};
use serde_json::Value;
use std::collections::HashMap;

// ============================================================================
// Tool Trait - 도구 인터페이스
// ============================================================================

/// 도구 메타데이터
#[derive(Debug, Clone)]
pub struct ToolMeta {
    /// 도구 이름 (고유 식별자)
    pub name: String,
    /// 표시 이름
    pub display_name: String,
    /// 설명
    pub description: String,
    /// 카테고리 (filesystem, execute, network 등)
    pub category: String,
    /// 이 도구가 필요로 하는 권한들
    pub permissions: Vec<PermissionDef>,
}

impl ToolMeta {
    pub fn new(name: impl Into<String>) -> Self {
        let name = name.into();
        Self {
            display_name: name.clone(),
            name,
            description: String::new(),
            category: "general".to_string(),
            permissions: Vec::new(),
        }
    }

    pub fn display_name(mut self, name: impl Into<String>) -> Self {
        self.display_name = name.into();
        self
    }

    pub fn description(mut self, desc: impl Into<String>) -> Self {
        self.description = desc.into();
        self
    }

    pub fn category(mut self, cat: impl Into<String>) -> Self {
        self.category = cat.into();
        self
    }

    pub fn permission(mut self, perm: PermissionDef) -> Self {
        self.permissions.push(perm);
        self
    }

    pub fn permissions(mut self, perms: Vec<PermissionDef>) -> Self {
        self.permissions.extend(perms);
        self
    }
}

/// 도구 실행 결과 (Tool trait용)
///
/// 이 타입은 `Tool::execute()` 메서드의 반환 타입입니다.
/// LLM 메시지의 ToolResult와는 다릅니다 (types.rs 참조).
#[derive(Debug, Clone)]
pub struct ToolExecutionResult {
    /// 성공 여부
    pub success: bool,
    /// 출력 내용
    pub output: String,
    /// 에러 메시지 (실패 시)
    pub error: Option<String>,
    /// 추가 메타데이터
    pub metadata: HashMap<String, Value>,
}

impl ToolExecutionResult {
    pub fn success(output: impl Into<String>) -> Self {
        Self {
            success: true,
            output: output.into(),
            error: None,
            metadata: HashMap::new(),
        }
    }

    pub fn error(message: impl Into<String>) -> Self {
        let msg = message.into();
        Self {
            success: false,
            output: String::new(),
            error: Some(msg),
            metadata: HashMap::new(),
        }
    }

    pub fn with_metadata(mut self, key: impl Into<String>, value: Value) -> Self {
        self.metadata.insert(key.into(), value);
        self
    }

    /// LLM 메시지용 ToolResultMessage로 변환
    pub fn to_tool_result_message(
        &self,
        tool_call_id: impl Into<String>,
    ) -> super::types::ToolResultMessage {
        if self.success {
            super::types::ToolResultMessage::success(tool_call_id, &self.output)
        } else {
            super::types::ToolResultMessage::error(
                tool_call_id,
                self.error.as_deref().unwrap_or("Unknown error"),
            )
        }
    }

    /// LLM 메시지용 ToolResultMessage로 변환 (deprecated alias)
    #[deprecated(since = "0.2.0", note = "Use to_tool_result_message instead")]
    pub fn to_tool_result(
        &self,
        tool_call_id: impl Into<String>,
    ) -> super::types::ToolResultMessage {
        self.to_tool_result_message(tool_call_id)
    }
}

// 하위 호환성을 위한 type alias
pub type ToolResult = ToolExecutionResult;

/// 도구 인터페이스
///
/// Layer2-tool에서 구현합니다.
/// 각 도구는 자신의 권한을 등록하고, 실행 시 권한 검사를 받습니다.
#[async_trait]
pub trait Tool: Send + Sync {
    /// 도구 이름 (고유 식별자) - 필수 구현
    ///
    /// 이 메서드는 반드시 구현해야 합니다.
    /// Tool registry에서 도구를 식별하는 데 사용됩니다.
    fn name(&self) -> &str;

    /// 도구 메타데이터 반환
    fn meta(&self) -> ToolMeta;

    /// JSON 스키마 반환 (MCP 호환)
    fn schema(&self) -> Value;

    /// 도구 실행
    ///
    /// # Arguments
    /// * `input` - JSON 형식의 입력 파라미터
    /// * `context` - 실행 컨텍스트 (권한 검사, Shell 설정 등)
    async fn execute(&self, input: Value, context: &dyn ToolContext) -> Result<ToolResult>;

    /// 이 도구가 필요로 하는 권한 액션 생성
    ///
    /// 입력 파라미터를 분석하여 필요한 권한을 반환합니다.
    fn required_permission(&self, input: &Value) -> Option<PermissionAction>;

    /// Layer1에 권한 정의 등록
    fn register_permissions(&self) {
        let meta = self.meta();
        for perm in meta.permissions {
            crate::permission::register(perm);
        }
    }
}

/// 도구 실행 컨텍스트
///
/// Layer3-agent에서 구현합니다.
/// 도구 실행에 필요한 환경을 제공합니다.
#[async_trait]
pub trait ToolContext: Send + Sync {
    /// 현재 작업 디렉토리
    fn working_dir(&self) -> &std::path::Path;

    /// 세션 ID
    fn session_id(&self) -> &str;

    /// 환경 변수
    fn env(&self) -> &HashMap<String, String>;

    /// 권한 검사
    async fn check_permission(&self, tool: &str, action: &PermissionAction) -> PermissionStatus;

    /// 권한 요청 (UI 프롬프트 발생)
    async fn request_permission(
        &self,
        tool: &str,
        description: &str,
        action: PermissionAction,
    ) -> Result<bool>;

    /// Shell 설정 가져오기
    fn shell_config(&self) -> &dyn ShellConfig;
}

// ============================================================================
// Shell Config - 쉘 설정 인터페이스
// ============================================================================

/// 쉘 타입
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ShellType {
    /// Bash (Linux/macOS 기본)
    Bash,
    /// Zsh (macOS 기본)
    Zsh,
    /// Fish
    Fish,
    /// PowerShell (Windows 기본)
    PowerShell,
    /// Cmd (Windows 레거시)
    Cmd,
    /// Nushell
    Nushell,
}

impl ShellType {
    /// 현재 OS의 기본 쉘
    pub fn default_for_os() -> Self {
        #[cfg(target_os = "windows")]
        {
            Self::PowerShell
        }
        #[cfg(target_os = "macos")]
        {
            Self::Zsh
        }
        #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
        {
            Self::Bash
        }
    }

    /// 쉘 실행 파일 이름
    pub fn executable(&self) -> &'static str {
        match self {
            ShellType::Bash => "bash",
            ShellType::Zsh => "zsh",
            ShellType::Fish => "fish",
            ShellType::PowerShell => {
                #[cfg(target_os = "windows")]
                {
                    "powershell.exe"
                }
                #[cfg(not(target_os = "windows"))]
                {
                    "pwsh"
                }
            }
            ShellType::Cmd => "cmd.exe",
            ShellType::Nushell => "nu",
        }
    }

    /// 명령어 실행 인자
    pub fn exec_args(&self) -> Vec<&'static str> {
        match self {
            ShellType::Bash | ShellType::Zsh | ShellType::Fish | ShellType::Nushell => {
                vec!["-c"]
            }
            ShellType::PowerShell => vec!["-NoProfile", "-Command"],
            ShellType::Cmd => vec!["/C"],
        }
    }
}

impl std::fmt::Display for ShellType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ShellType::Bash => write!(f, "bash"),
            ShellType::Zsh => write!(f, "zsh"),
            ShellType::Fish => write!(f, "fish"),
            ShellType::PowerShell => write!(f, "powershell"),
            ShellType::Cmd => write!(f, "cmd"),
            ShellType::Nushell => write!(f, "nu"),
        }
    }
}

/// 쉘 설정 인터페이스
pub trait ShellConfig: Send + Sync {
    /// 현재 쉘 타입
    fn shell_type(&self) -> ShellType;

    /// 쉘 실행 파일 경로 (커스텀 또는 기본)
    fn executable(&self) -> &str;

    /// 명령어 실행 인자
    fn exec_args(&self) -> Vec<String>;

    /// 추가 환경 변수
    fn env_vars(&self) -> HashMap<String, String>;

    /// 명령어 타임아웃 (초)
    fn timeout_secs(&self) -> u64;

    /// 작업 디렉토리
    fn working_dir(&self) -> Option<&std::path::Path>;
}

// ============================================================================
// Provider Trait - LLM 프로바이더 인터페이스
// ============================================================================

/// 프로바이더 메타데이터
#[derive(Debug, Clone)]
pub struct ProviderMeta {
    /// 프로바이더 ID
    pub id: String,
    /// 표시 이름
    pub name: String,
    /// 기본 URL
    pub base_url: String,
    /// 지원하는 기능들
    pub capabilities: Vec<String>,
}

/// 채팅 메시지 (Provider trait 전용)
///
/// Note: 일반적인 대화 메시지는 `types::Message`를 사용하세요.
/// 이 타입은 Provider trait의 저수준 API에서만 사용됩니다.
#[derive(Debug, Clone)]
pub struct ChatMessage {
    pub role: super::types::MessageRole,
    pub content: String,
}

// MessageRole은 types.rs에서 정의됩니다
pub use super::types::MessageRole;

/// 채팅 요청
#[derive(Debug, Clone)]
pub struct ChatRequest {
    pub model: String,
    pub messages: Vec<ChatMessage>,
    pub max_tokens: Option<u32>,
    pub temperature: Option<f32>,
    pub tools: Option<Vec<Value>>,
    pub stream: bool,
}

/// 채팅 응답
#[derive(Debug, Clone)]
pub struct ChatResponse {
    pub content: String,
    pub tool_calls: Vec<super::types::ToolCall>,
    pub usage: Option<super::types::TokenUsage>,
    pub stop_reason: Option<String>,
}

// Note: ToolCall, TokenUsage, StreamEvent는 types.rs에서 정의됩니다.
// 여기서는 Provider trait에서 사용하기 위해 re-export합니다.
pub use super::types::{StreamEvent, TokenUsage, ToolCall};

/// LLM 프로바이더 인터페이스
#[async_trait]
pub trait Provider: Send + Sync {
    /// 프로바이더 메타데이터
    fn meta(&self) -> ProviderMeta;

    /// 채팅 완료 (비스트리밍)
    async fn chat(&self, request: ChatRequest) -> Result<ChatResponse>;

    /// 채팅 완료 (스트리밍)
    async fn chat_stream(
        &self,
        request: ChatRequest,
        callback: Box<dyn Fn(StreamEvent) + Send>,
    ) -> Result<ChatResponse>;

    /// 연결 테스트
    async fn health_check(&self) -> Result<bool>;
}

// ============================================================================
// Task Trait - 독립 실행 태스크 인터페이스
// ============================================================================

/// 태스크 상태
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskState {
    /// 대기 중
    Pending,
    /// 실행 중
    Running,
    /// 일시 정지
    Paused,
    /// 완료
    Completed,
    /// 실패
    Failed,
    /// 취소됨
    Cancelled,
}

/// 태스크 메타데이터
#[derive(Debug, Clone)]
pub struct TaskMeta {
    pub id: String,
    pub name: String,
    pub description: String,
    pub state: TaskState,
    pub progress: Option<f32>,
}

/// 태스크 인터페이스
///
/// 독립적으로 실행되는 프로그래밍 작업을 정의합니다.
#[async_trait]
pub trait Task: Send + Sync {
    /// 태스크 메타데이터
    fn meta(&self) -> TaskMeta;

    /// 태스크 실행
    async fn run(&self, context: &dyn TaskContext) -> Result<TaskResult>;

    /// 태스크 취소
    async fn cancel(&self) -> Result<()>;

    /// 진행 상황 조회
    fn progress(&self) -> Option<f32>;
}

/// 태스크 실행 결과
#[derive(Debug, Clone)]
pub struct TaskResult {
    pub success: bool,
    pub output: String,
    pub artifacts: Vec<TaskArtifact>,
}

/// 태스크 산출물
#[derive(Debug, Clone)]
pub struct TaskArtifact {
    pub name: String,
    pub path: Option<String>,
    pub content: Option<String>,
}

/// 태스크 실행 컨텍스트
#[async_trait]
pub trait TaskContext: Send + Sync {
    /// 세션 ID
    fn session_id(&self) -> &str;

    /// 도구 실행
    async fn execute_tool(&self, tool: &str, input: Value) -> Result<ToolResult>;

    /// 프로바이더 호출
    async fn chat(&self, request: ChatRequest) -> Result<ChatResponse>;

    /// 진행 상황 보고
    fn report_progress(&self, progress: f32, message: &str);

    /// 하위 태스크 생성
    async fn spawn_subtask(&self, task: Box<dyn Task>) -> Result<String>;
}

/// 태스크 관찰자 (UI 연동)
pub trait TaskObserver: Send + Sync {
    /// 태스크 상태 변경
    fn on_state_change(&self, task_id: &str, state: TaskState);

    /// 진행 상황 업데이트
    fn on_progress(&self, task_id: &str, progress: f32, message: &str);

    /// 태스크 완료
    fn on_complete(&self, task_id: &str, result: &TaskResult);
}

// ============================================================================
// Configurable Trait - 설정 가능 인터페이스
// ============================================================================

/// 설정 가능 인터페이스
///
/// JSON/TOML 설정 파일과 연동됩니다.
pub trait Configurable: Serialize + DeserializeOwned + Default {
    /// 설정 파일 이름
    const FILE_NAME: &'static str;

    /// 글로벌 설정 로드
    fn load_global() -> Result<Self> {
        let store = crate::storage::JsonStore::global()?;
        Ok(store.load_or_default(Self::FILE_NAME))
    }

    /// 프로젝트 설정 로드
    fn load_project() -> Result<Self> {
        let store = crate::storage::JsonStore::current_project()?;
        Ok(store.load_or_default(Self::FILE_NAME))
    }

    /// 글로벌 + 프로젝트 병합 로드
    fn load() -> Result<Self>
    where
        Self: Sized + Clone,
    {
        let global = Self::load_global().unwrap_or_default();
        // 프로젝트 설정이 있으면 병합 (기본: 프로젝트 우선)
        if let Ok(project) = Self::load_project() {
            // 기본 구현은 프로젝트 설정 반환
            // 병합이 필요하면 오버라이드
            Ok(project)
        } else {
            Ok(global)
        }
    }

    /// 글로벌 설정 저장
    fn save_global(&self) -> Result<()> {
        let store = crate::storage::JsonStore::global()?;
        store.save(Self::FILE_NAME, self)
    }

    /// 프로젝트 설정 저장
    fn save_project(&self) -> Result<()> {
        let store = crate::storage::JsonStore::current_project()?;
        store.save(Self::FILE_NAME, self)
    }
}

// ============================================================================
// Permission Delegate - UI 연동
// ============================================================================

/// 권한 요청 응답
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PermissionResponse {
    /// 이번만 허용
    AllowOnce,
    /// 세션 동안 허용
    AllowSession,
    /// 영구 허용
    AllowPermanent,
    /// 거부
    Deny,
    /// 영구 거부
    DenyPermanent,
}

/// 권한 UI 델리게이트
///
/// Layer4-CLI에서 구현합니다.
/// 권한 요청 시 사용자에게 프롬프트를 표시합니다.
#[async_trait]
pub trait PermissionDelegate: Send + Sync {
    /// 권한 요청 UI 표시
    async fn request_permission(
        &self,
        tool_name: &str,
        action: &PermissionAction,
        description: &str,
        risk_score: u8,
    ) -> PermissionResponse;

    /// 알림 표시 (정보성)
    fn notify(&self, message: &str);

    /// 에러 표시
    fn show_error(&self, error: &str);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_shell_type_default() {
        let shell = ShellType::default_for_os();
        #[cfg(target_os = "windows")]
        assert_eq!(shell, ShellType::PowerShell);
        #[cfg(target_os = "macos")]
        assert_eq!(shell, ShellType::Zsh);
    }

    #[test]
    fn test_tool_meta_builder() {
        let meta = ToolMeta::new("bash")
            .display_name("Bash Shell")
            .description("Execute shell commands")
            .category("execute");

        assert_eq!(meta.name, "bash");
        assert_eq!(meta.display_name, "Bash Shell");
        assert_eq!(meta.category, "execute");
    }

    #[test]
    fn test_tool_result() {
        let result = ToolResult::success("output").with_metadata("key", serde_json::json!("value"));
        assert!(result.success);
        assert_eq!(result.output, "output");
        assert!(result.metadata.contains_key("key"));
    }
}
