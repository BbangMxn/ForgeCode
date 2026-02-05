//! Hooks System - 확장 가능한 훅 시스템
//!
//! Claude Code 스타일의 플러그인/훅 아키텍처:
//! - 이벤트 기반 훅
//! - 필터 훅 (입출력 변환)
//! - 커스텀 도구 등록

#![allow(dead_code)]

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// 훅 이벤트 타입
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum HookEvent {
    /// 세션 시작
    SessionStart,
    /// 세션 종료
    SessionEnd,
    /// 메시지 전송 전
    BeforeMessage,
    /// 메시지 전송 후
    AfterMessage,
    /// 도구 실행 전
    BeforeToolExec,
    /// 도구 실행 후
    AfterToolExec,
    /// 파일 읽기 전
    BeforeFileRead,
    /// 파일 쓰기 전
    BeforeFileWrite,
    /// 파일 쓰기 후
    AfterFileWrite,
    /// 에러 발생
    OnError,
    /// 컨텍스트 압축
    OnCompress,
    /// 커스텀 이벤트
    Custom(String),
}

impl std::fmt::Display for HookEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HookEvent::SessionStart => write!(f, "session.start"),
            HookEvent::SessionEnd => write!(f, "session.end"),
            HookEvent::BeforeMessage => write!(f, "message.before"),
            HookEvent::AfterMessage => write!(f, "message.after"),
            HookEvent::BeforeToolExec => write!(f, "tool.before"),
            HookEvent::AfterToolExec => write!(f, "tool.after"),
            HookEvent::BeforeFileRead => write!(f, "file.read.before"),
            HookEvent::BeforeFileWrite => write!(f, "file.write.before"),
            HookEvent::AfterFileWrite => write!(f, "file.write.after"),
            HookEvent::OnError => write!(f, "error"),
            HookEvent::OnCompress => write!(f, "compress"),
            HookEvent::Custom(name) => write!(f, "custom.{}", name),
        }
    }
}

/// 훅 컨텍스트 - 이벤트와 함께 전달되는 데이터
#[derive(Debug, Clone, Default)]
pub struct HookContext {
    /// 세션 ID
    pub session_id: Option<String>,
    /// 현재 모델
    pub model: Option<String>,
    /// 메시지 내용
    pub message: Option<String>,
    /// 도구 이름
    pub tool_name: Option<String>,
    /// 도구 입력
    pub tool_input: Option<String>,
    /// 도구 출력
    pub tool_output: Option<String>,
    /// 파일 경로
    pub file_path: Option<String>,
    /// 파일 내용
    pub file_content: Option<String>,
    /// 에러 메시지
    pub error: Option<String>,
    /// 추가 데이터
    pub extra: HashMap<String, String>,
}

impl HookContext {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_session(mut self, session_id: &str) -> Self {
        self.session_id = Some(session_id.to_string());
        self
    }

    pub fn with_message(mut self, message: &str) -> Self {
        self.message = Some(message.to_string());
        self
    }

    pub fn with_tool(mut self, name: &str, input: Option<&str>, output: Option<&str>) -> Self {
        self.tool_name = Some(name.to_string());
        self.tool_input = input.map(|s| s.to_string());
        self.tool_output = output.map(|s| s.to_string());
        self
    }

    pub fn with_file(mut self, path: &str, content: Option<&str>) -> Self {
        self.file_path = Some(path.to_string());
        self.file_content = content.map(|s| s.to_string());
        self
    }

    pub fn with_error(mut self, error: &str) -> Self {
        self.error = Some(error.to_string());
        self
    }

    pub fn set_extra(&mut self, key: &str, value: &str) {
        self.extra.insert(key.to_string(), value.to_string());
    }

    pub fn get_extra(&self, key: &str) -> Option<&str> {
        self.extra.get(key).map(|s| s.as_str())
    }
}

/// 훅 결과
#[derive(Debug, Clone)]
pub enum HookResult {
    /// 계속 진행
    Continue,
    /// 수정된 컨텍스트로 계속
    Modified(HookContext),
    /// 중단 (메시지 포함)
    Abort(String),
    /// 건너뛰기 (다음 훅으로)
    Skip,
}

impl Default for HookResult {
    fn default() -> Self {
        HookResult::Continue
    }
}

/// 훅 핸들러 trait
pub trait HookHandler: Send + Sync {
    /// 훅 이름
    fn name(&self) -> &str;

    /// 이벤트 처리
    fn handle(&self, event: &HookEvent, context: &HookContext) -> HookResult;

    /// 우선순위 (높을수록 먼저 실행)
    fn priority(&self) -> i32 {
        0
    }
}

/// 동기 훅 핸들러 (클로저용)
pub struct SyncHookHandler<F>
where
    F: Fn(&HookEvent, &HookContext) -> HookResult + Send + Sync,
{
    name: String,
    priority: i32,
    handler: F,
}

impl<F> SyncHookHandler<F>
where
    F: Fn(&HookEvent, &HookContext) -> HookResult + Send + Sync,
{
    pub fn new(name: &str, handler: F) -> Self {
        Self {
            name: name.to_string(),
            priority: 0,
            handler,
        }
    }

    pub fn with_priority(mut self, priority: i32) -> Self {
        self.priority = priority;
        self
    }
}

impl<F> HookHandler for SyncHookHandler<F>
where
    F: Fn(&HookEvent, &HookContext) -> HookResult + Send + Sync,
{
    fn name(&self) -> &str {
        &self.name
    }

    fn handle(&self, event: &HookEvent, context: &HookContext) -> HookResult {
        (self.handler)(event, context)
    }

    fn priority(&self) -> i32 {
        self.priority
    }
}

/// 훅 매니저
pub struct HookManager {
    /// 이벤트별 핸들러 목록
    handlers: RwLock<HashMap<HookEvent, Vec<Arc<dyn HookHandler>>>>,
    /// 전역 핸들러 (모든 이벤트)
    global_handlers: RwLock<Vec<Arc<dyn HookHandler>>>,
    /// 활성화 여부
    enabled: RwLock<bool>,
}

impl HookManager {
    pub fn new() -> Self {
        Self {
            handlers: RwLock::new(HashMap::new()),
            global_handlers: RwLock::new(Vec::new()),
            enabled: RwLock::new(true),
        }
    }

    /// 특정 이벤트에 핸들러 등록
    pub async fn register(&self, event: HookEvent, handler: Arc<dyn HookHandler>) {
        let mut handlers = self.handlers.write().await;
        let entry = handlers.entry(event).or_insert_with(Vec::new);
        entry.push(handler);
        // 우선순위순 정렬
        entry.sort_by(|a, b| b.priority().cmp(&a.priority()));
    }

    /// 전역 핸들러 등록 (모든 이벤트)
    pub async fn register_global(&self, handler: Arc<dyn HookHandler>) {
        let mut global = self.global_handlers.write().await;
        global.push(handler);
        global.sort_by(|a, b| b.priority().cmp(&a.priority()));
    }

    /// 핸들러 제거
    pub async fn unregister(&self, event: &HookEvent, handler_name: &str) {
        let mut handlers = self.handlers.write().await;
        if let Some(event_handlers) = handlers.get_mut(event) {
            event_handlers.retain(|h| h.name() != handler_name);
        }
    }

    /// 전역 핸들러 제거
    pub async fn unregister_global(&self, handler_name: &str) {
        let mut global = self.global_handlers.write().await;
        global.retain(|h| h.name() != handler_name);
    }

    /// 이벤트 트리거
    pub async fn trigger(&self, event: &HookEvent, context: HookContext) -> HookResult {
        if !*self.enabled.read().await {
            return HookResult::Continue;
        }

        let mut current_context = context;

        // 전역 핸들러 먼저 실행
        {
            let global = self.global_handlers.read().await;
            for handler in global.iter() {
                match handler.handle(event, &current_context) {
                    HookResult::Continue => {}
                    HookResult::Modified(ctx) => {
                        current_context = ctx;
                    }
                    HookResult::Abort(msg) => {
                        return HookResult::Abort(msg);
                    }
                    HookResult::Skip => continue,
                }
            }
        }

        // 이벤트별 핸들러 실행
        {
            let handlers = self.handlers.read().await;
            if let Some(event_handlers) = handlers.get(event) {
                for handler in event_handlers.iter() {
                    match handler.handle(event, &current_context) {
                        HookResult::Continue => {}
                        HookResult::Modified(ctx) => {
                            current_context = ctx;
                        }
                        HookResult::Abort(msg) => {
                            return HookResult::Abort(msg);
                        }
                        HookResult::Skip => continue,
                    }
                }
            }
        }

        HookResult::Modified(current_context)
    }

    /// 훅 시스템 활성화/비활성화
    pub async fn set_enabled(&self, enabled: bool) {
        *self.enabled.write().await = enabled;
    }

    /// 등록된 핸들러 수
    pub async fn handler_count(&self, event: &HookEvent) -> usize {
        let handlers = self.handlers.read().await;
        handlers.get(event).map(|h| h.len()).unwrap_or(0)
    }

    /// 모든 이벤트 목록
    pub async fn registered_events(&self) -> Vec<HookEvent> {
        let handlers = self.handlers.read().await;
        handlers.keys().cloned().collect()
    }
}

impl Default for HookManager {
    fn default() -> Self {
        Self::new()
    }
}

/// 빌트인 훅들
pub mod builtin {
    use super::*;

    /// 로깅 훅 - 모든 이벤트 로깅
    pub struct LoggingHook {
        verbose: bool,
    }

    impl LoggingHook {
        pub fn new(verbose: bool) -> Self {
            Self { verbose }
        }
    }

    impl HookHandler for LoggingHook {
        fn name(&self) -> &str {
            "builtin.logging"
        }

        fn handle(&self, event: &HookEvent, context: &HookContext) -> HookResult {
            if self.verbose {
                tracing::debug!(
                    event = %event,
                    session = ?context.session_id,
                    tool = ?context.tool_name,
                    "Hook event triggered"
                );
            }
            HookResult::Continue
        }

        fn priority(&self) -> i32 {
            100 // 높은 우선순위로 먼저 실행
        }
    }

    /// 파일 보호 훅 - 특정 파일/디렉토리 보호
    pub struct FileProtectionHook {
        protected_paths: Vec<String>,
    }

    impl FileProtectionHook {
        pub fn new(protected_paths: Vec<String>) -> Self {
            Self { protected_paths }
        }
    }

    impl HookHandler for FileProtectionHook {
        fn name(&self) -> &str {
            "builtin.file_protection"
        }

        fn handle(&self, event: &HookEvent, context: &HookContext) -> HookResult {
            // 파일 쓰기 이벤트에서만 확인
            if !matches!(event, HookEvent::BeforeFileWrite) {
                return HookResult::Continue;
            }

            if let Some(path) = &context.file_path {
                for protected in &self.protected_paths {
                    if path.contains(protected) {
                        return HookResult::Abort(format!(
                            "Cannot modify protected path: {}",
                            protected
                        ));
                    }
                }
            }

            HookResult::Continue
        }

        fn priority(&self) -> i32 {
            50
        }
    }

    /// 토큰 제한 훅 - 메시지 길이 제한
    pub struct TokenLimitHook {
        max_tokens: usize,
    }

    impl TokenLimitHook {
        pub fn new(max_tokens: usize) -> Self {
            Self { max_tokens }
        }

        fn estimate_tokens(text: &str) -> usize {
            // 간단한 토큰 추정 (4자당 1토큰)
            text.len() / 4
        }
    }

    impl HookHandler for TokenLimitHook {
        fn name(&self) -> &str {
            "builtin.token_limit"
        }

        fn handle(&self, event: &HookEvent, context: &HookContext) -> HookResult {
            if !matches!(event, HookEvent::BeforeMessage) {
                return HookResult::Continue;
            }

            if let Some(message) = &context.message {
                let estimated = Self::estimate_tokens(message);
                if estimated > self.max_tokens {
                    return HookResult::Abort(format!(
                        "Message too long: ~{} tokens (max: {})",
                        estimated, self.max_tokens
                    ));
                }
            }

            HookResult::Continue
        }
    }

    /// 도구 허용 목록 훅
    pub struct ToolAllowlistHook {
        allowed_tools: Vec<String>,
    }

    impl ToolAllowlistHook {
        pub fn new(allowed_tools: Vec<String>) -> Self {
            Self { allowed_tools }
        }
    }

    impl HookHandler for ToolAllowlistHook {
        fn name(&self) -> &str {
            "builtin.tool_allowlist"
        }

        fn handle(&self, event: &HookEvent, context: &HookContext) -> HookResult {
            if !matches!(event, HookEvent::BeforeToolExec) {
                return HookResult::Continue;
            }

            if let Some(tool) = &context.tool_name {
                if !self.allowed_tools.iter().any(|t| t == tool) {
                    return HookResult::Abort(format!(
                        "Tool '{}' is not in the allowlist",
                        tool
                    ));
                }
            }

            HookResult::Continue
        }
    }
}

/// 헬퍼: 간단한 클로저 핸들러 생성
pub fn hook<F>(name: &str, handler: F) -> Arc<SyncHookHandler<F>>
where
    F: Fn(&HookEvent, &HookContext) -> HookResult + Send + Sync,
{
    Arc::new(SyncHookHandler::new(name, handler))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_hook_manager() {
        let manager = HookManager::new();

        // 핸들러 등록
        let handler = hook("test", |_event, _ctx| HookResult::Continue);
        manager.register(HookEvent::SessionStart, handler).await;

        // 핸들러 수 확인
        assert_eq!(manager.handler_count(&HookEvent::SessionStart).await, 1);
    }

    #[tokio::test]
    async fn test_hook_trigger() {
        let manager = HookManager::new();

        // 메시지를 수정하는 핸들러
        let handler = hook("modifier", |_event, ctx| {
            let mut new_ctx = ctx.clone();
            new_ctx.message = Some("Modified!".to_string());
            HookResult::Modified(new_ctx)
        });

        manager.register(HookEvent::BeforeMessage, handler).await;

        let ctx = HookContext::new().with_message("Original");
        let result = manager.trigger(&HookEvent::BeforeMessage, ctx).await;

        if let HookResult::Modified(new_ctx) = result {
            assert_eq!(new_ctx.message, Some("Modified!".to_string()));
        } else {
            panic!("Expected Modified result");
        }
    }

    #[tokio::test]
    async fn test_hook_abort() {
        let manager = HookManager::new();

        let handler = hook("aborter", |_event, _ctx| {
            HookResult::Abort("Aborted!".to_string())
        });

        manager.register(HookEvent::BeforeToolExec, handler).await;

        let ctx = HookContext::new().with_tool("dangerous_tool", None, None);
        let result = manager.trigger(&HookEvent::BeforeToolExec, ctx).await;

        assert!(matches!(result, HookResult::Abort(_)));
    }

    #[tokio::test]
    async fn test_file_protection_hook() {
        let hook = builtin::FileProtectionHook::new(vec![".env".to_string(), "secrets/".to_string()]);

        let ctx = HookContext::new().with_file(".env", Some("SECRET=123"));
        let result = hook.handle(&HookEvent::BeforeFileWrite, &ctx);

        assert!(matches!(result, HookResult::Abort(_)));
    }

    #[tokio::test]
    async fn test_priority_ordering() {
        let manager = HookManager::new();

        // 낮은 우선순위
        let low = Arc::new(SyncHookHandler::new("low", |_e, _c| HookResult::Continue).with_priority(1));
        
        // 높은 우선순위
        let high = Arc::new(SyncHookHandler::new("high", |_e, _c| HookResult::Continue).with_priority(100));

        manager.register(HookEvent::SessionStart, low).await;
        manager.register(HookEvent::SessionStart, high).await;

        // 높은 우선순위가 먼저 실행됨
        let handlers = manager.handlers.read().await;
        let h = handlers.get(&HookEvent::SessionStart).unwrap();
        assert_eq!(h[0].name(), "high");
        assert_eq!(h[1].name(), "low");
    }
}
