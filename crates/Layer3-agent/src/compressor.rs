//! Context Compressor
//!
//! Claude Code 스타일의 자동 컨텍스트 압축 시스템입니다.
//! 토큰 사용량이 임계값(기본 92%)을 초과하면 자동으로 대화 히스토리를 요약합니다.

use crate::history::MessageHistory;
use forge_foundation::Result;
use forge_provider::{Message, MessageRole};
use std::sync::atomic::{AtomicU64, Ordering};

// ============================================================================
// Compressor Configuration
// ============================================================================

/// 압축 설정
#[derive(Debug, Clone)]
pub struct CompressorConfig {
    /// 압축 시작 임계값 (0.0 ~ 1.0, 기본 0.92)
    pub threshold: f32,

    /// 최대 컨텍스트 크기 (토큰)
    pub max_context_tokens: usize,

    /// 압축 후 목표 사용률 (기본 0.5)
    pub target_usage_after_compress: f32,

    /// 유지할 최근 메시지 수
    pub keep_recent_messages: usize,

    /// 시스템 프롬프트 유지 여부
    pub preserve_system_prompt: bool,

    /// Tool 결과 요약 여부
    pub summarize_tool_results: bool,

    /// 최대 요약 길이 (토큰)
    pub max_summary_tokens: usize,
}

impl Default for CompressorConfig {
    fn default() -> Self {
        Self {
            threshold: 0.92,
            max_context_tokens: 200_000, // Claude 3.5 Sonnet default
            target_usage_after_compress: 0.5,
            keep_recent_messages: 10,
            preserve_system_prompt: true,
            summarize_tool_results: true,
            max_summary_tokens: 4000,
        }
    }
}

impl CompressorConfig {
    /// Claude Code 스타일 설정
    pub fn claude_code_style() -> Self {
        Self {
            threshold: 0.92,
            max_context_tokens: 200_000,
            target_usage_after_compress: 0.5,
            keep_recent_messages: 10,
            preserve_system_prompt: true,
            summarize_tool_results: true,
            max_summary_tokens: 4000,
        }
    }

    /// 빠른 압축 설정 (더 공격적)
    pub fn aggressive() -> Self {
        Self {
            threshold: 0.8,
            max_context_tokens: 200_000,
            target_usage_after_compress: 0.3,
            keep_recent_messages: 5,
            preserve_system_prompt: true,
            summarize_tool_results: true,
            max_summary_tokens: 2000,
        }
    }

    /// 보수적 압축 설정
    pub fn conservative() -> Self {
        Self {
            threshold: 0.95,
            max_context_tokens: 200_000,
            target_usage_after_compress: 0.7,
            keep_recent_messages: 20,
            preserve_system_prompt: true,
            summarize_tool_results: false,
            max_summary_tokens: 8000,
        }
    }

    /// 컨텍스트 크기 설정
    pub fn with_max_tokens(mut self, tokens: usize) -> Self {
        self.max_context_tokens = tokens;
        self
    }

    /// 임계값 설정
    pub fn with_threshold(mut self, threshold: f32) -> Self {
        self.threshold = threshold.clamp(0.5, 0.99);
        self
    }
}

// ============================================================================
// Compression Result
// ============================================================================

/// 압축 결과
#[derive(Debug, Clone)]
pub struct CompressionResult {
    /// 압축 수행 여부
    pub compressed: bool,

    /// 압축 전 토큰 수
    pub tokens_before: usize,

    /// 압축 후 토큰 수
    pub tokens_after: usize,

    /// 절약된 토큰 수
    pub tokens_saved: usize,

    /// 제거된 메시지 수
    pub messages_removed: usize,

    /// 생성된 요약
    pub summary: Option<String>,
}

impl CompressionResult {
    /// 압축하지 않음
    pub fn not_needed(current_tokens: usize) -> Self {
        Self {
            compressed: false,
            tokens_before: current_tokens,
            tokens_after: current_tokens,
            tokens_saved: 0,
            messages_removed: 0,
            summary: None,
        }
    }
}

// ============================================================================
// Context Compressor
// ============================================================================

/// 컨텍스트 압축기
///
/// 토큰 사용량이 임계값을 초과하면 자동으로 히스토리를 요약합니다.
pub struct ContextCompressor {
    config: CompressorConfig,

    /// 누적 압축 횟수
    compression_count: AtomicU64,

    /// 누적 절약 토큰
    total_tokens_saved: AtomicU64,
}

impl ContextCompressor {
    /// 새 압축기 생성
    pub fn new(config: CompressorConfig) -> Self {
        Self {
            config,
            compression_count: AtomicU64::new(0),
            total_tokens_saved: AtomicU64::new(0),
        }
    }

    /// 기본 설정으로 생성
    pub fn default_config() -> Self {
        Self::new(CompressorConfig::default())
    }

    /// 설정 참조
    pub fn config(&self) -> &CompressorConfig {
        &self.config
    }

    /// 압축 필요 여부 확인
    pub fn needs_compression(&self, history: &MessageHistory) -> bool {
        let current_tokens = history.estimate_tokens();
        let threshold_tokens =
            (self.config.max_context_tokens as f32 * self.config.threshold) as usize;
        current_tokens > threshold_tokens
    }

    /// 현재 사용률 계산
    pub fn current_usage(&self, history: &MessageHistory) -> f32 {
        let current_tokens = history.estimate_tokens();
        current_tokens as f32 / self.config.max_context_tokens as f32
    }

    /// 압축 수행
    ///
    /// LLM을 사용하지 않는 기본 압축:
    /// 1. 오래된 메시지 제거
    /// 2. Tool 결과 요약
    /// 3. 최근 메시지 유지
    pub fn compress(&self, history: &mut MessageHistory) -> Result<CompressionResult> {
        let tokens_before = history.estimate_tokens();

        // 압축 필요 없음
        if !self.needs_compression(history) {
            return Ok(CompressionResult::not_needed(tokens_before));
        }

        // 목표 토큰 수 계산
        let target_tokens = (self.config.max_context_tokens as f32
            * self.config.target_usage_after_compress) as usize;

        // 메시지 가져오기
        let messages = history.to_messages();
        let total_messages = messages.len();

        if total_messages <= self.config.keep_recent_messages {
            return Ok(CompressionResult::not_needed(tokens_before));
        }

        // 요약 생성 (LLM 없이 기본 요약)
        let summary = self.create_basic_summary(&messages, total_messages);

        // 새 히스토리 구성
        let mut new_messages = Vec::new();

        // 요약을 첫 번째 메시지로 추가
        new_messages.push(Message::user(format!(
            "Previous conversation summary:\n{}\n\nContinuing from here.",
            summary
        )));

        // 최근 메시지 유지
        let keep_start = total_messages.saturating_sub(self.config.keep_recent_messages);
        for msg in messages.into_iter().skip(keep_start) {
            new_messages.push(self.maybe_compress_message(msg));
        }

        // 히스토리 교체
        let messages_removed = total_messages - new_messages.len();
        history.clear();

        for msg in new_messages {
            history.add(msg);
        }

        let tokens_after = history.estimate_tokens();
        let tokens_saved = tokens_before.saturating_sub(tokens_after);

        // 통계 업데이트
        self.compression_count.fetch_add(1, Ordering::Relaxed);
        self.total_tokens_saved
            .fetch_add(tokens_saved as u64, Ordering::Relaxed);

        Ok(CompressionResult {
            compressed: true,
            tokens_before,
            tokens_after,
            tokens_saved,
            messages_removed,
            summary: Some(summary),
        })
    }

    /// 기본 요약 생성 (LLM 없이)
    fn create_basic_summary(&self, messages: &[Message], total: usize) -> String {
        let mut summary_parts = Vec::new();

        // 대화 통계
        let user_count = messages
            .iter()
            .filter(|m| m.role == MessageRole::User)
            .count();
        let assistant_count = messages
            .iter()
            .filter(|m| m.role == MessageRole::Assistant)
            .count();

        summary_parts.push(format!(
            "Conversation had {} messages ({} user, {} assistant).",
            total, user_count, assistant_count
        ));

        // Tool 사용 요약
        let tool_calls: Vec<_> = messages
            .iter()
            .filter_map(|m| m.tool_calls.as_ref())
            .flatten()
            .collect();

        if !tool_calls.is_empty() {
            let tool_names: std::collections::HashSet<_> =
                tool_calls.iter().map(|tc| tc.name.as_str()).collect();
            summary_parts.push(format!(
                "Tools used: {} (total {} calls)",
                tool_names.into_iter().collect::<Vec<_>>().join(", "),
                tool_calls.len()
            ));
        }

        // 주요 토픽 추출 (첫 번째 사용자 메시지 기반)
        if let Some(first_user) = messages.iter().find(|m| m.role == MessageRole::User) {
            let preview: String = first_user.content.chars().take(200).collect();
            summary_parts.push(format!("Initial topic: \"{}...\"", preview.trim()));
        }

        // 마지막 assistant 응답 요약
        if let Some(last_assistant) = messages
            .iter()
            .rev()
            .find(|m| m.role == MessageRole::Assistant && !m.content.is_empty())
        {
            let preview: String = last_assistant.content.chars().take(200).collect();
            summary_parts.push(format!("Last response: \"{}...\"", preview.trim()));
        }

        summary_parts.join("\n")
    }

    /// 메시지 압축 (필요시)
    fn maybe_compress_message(&self, mut message: Message) -> Message {
        if !self.config.summarize_tool_results {
            return message;
        }

        // Tool 결과가 너무 길면 요약
        if let Some(ref mut result) = message.tool_result {
            if result.content.len() > 2000 {
                let preview: String = result.content.chars().take(500).collect();
                let suffix: String = result
                    .content
                    .chars()
                    .rev()
                    .take(500)
                    .collect::<String>()
                    .chars()
                    .rev()
                    .collect();
                result.content = format!(
                    "[Truncated - {} chars total]\n{}...\n\n...{}",
                    result.content.len(),
                    preview,
                    suffix
                );
            }
        }

        message
    }

    /// 압축 통계
    pub fn stats(&self) -> CompressionStats {
        CompressionStats {
            compression_count: self.compression_count.load(Ordering::Relaxed),
            total_tokens_saved: self.total_tokens_saved.load(Ordering::Relaxed),
        }
    }
}

impl Default for ContextCompressor {
    fn default() -> Self {
        Self::default_config()
    }
}

/// 압축 통계
#[derive(Debug, Clone)]
pub struct CompressionStats {
    pub compression_count: u64,
    pub total_tokens_saved: u64,
}

// ============================================================================
// LLM-based Compressor (Optional)
// ============================================================================

/// LLM 기반 압축기 트레이트
///
/// 더 지능적인 요약을 위해 LLM을 사용할 수 있습니다.
#[async_trait::async_trait]
pub trait LlmCompressor: Send + Sync {
    /// LLM을 사용하여 대화 요약 생성
    async fn summarize_conversation(&self, messages: &[Message]) -> Result<String>;

    /// LLM을 사용하여 Tool 결과 요약
    async fn summarize_tool_result(&self, tool_name: &str, result: &str) -> Result<String>;
}

/// LLM 압축을 지원하는 확장 압축기
pub struct SmartCompressor {
    base: ContextCompressor,
    llm: Option<Box<dyn LlmCompressor>>,
}

impl SmartCompressor {
    pub fn new(config: CompressorConfig) -> Self {
        Self {
            base: ContextCompressor::new(config),
            llm: None,
        }
    }

    pub fn with_llm(mut self, llm: Box<dyn LlmCompressor>) -> Self {
        self.llm = Some(llm);
        self
    }

    /// 스마트 압축 수행
    pub async fn compress(&self, history: &mut MessageHistory) -> Result<CompressionResult> {
        if self.llm.is_none() {
            return self.base.compress(history);
        }

        // LLM 기반 압축 로직은 필요시 구현
        // 현재는 기본 압축 사용
        self.base.compress(history)
    }

    pub fn needs_compression(&self, history: &MessageHistory) -> bool {
        self.base.needs_compression(history)
    }

    pub fn current_usage(&self, history: &MessageHistory) -> f32 {
        self.base.current_usage(history)
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_history(message_count: usize) -> MessageHistory {
        let mut history = MessageHistory::new();

        for i in 0..message_count {
            history.add_user(format!(
                "User message {} with some content to add tokens",
                i
            ));
            history.add_assistant(format!(
                "Assistant response {} with more content for testing compression",
                i
            ));
        }

        history
    }

    #[test]
    fn test_compression_not_needed() {
        let config = CompressorConfig {
            threshold: 0.92,
            max_context_tokens: 100_000,
            ..Default::default()
        };
        let compressor = ContextCompressor::new(config);

        let mut history = create_test_history(5);
        let result = compressor.compress(&mut history).unwrap();

        assert!(!result.compressed);
    }

    #[test]
    fn test_compression_triggered() {
        let config = CompressorConfig {
            threshold: 0.1, // Very low threshold to trigger compression
            max_context_tokens: 1000,
            keep_recent_messages: 2,
            ..Default::default()
        };
        let compressor = ContextCompressor::new(config);

        let mut history = create_test_history(20);
        let result = compressor.compress(&mut history).unwrap();

        assert!(result.compressed);
        assert!(result.tokens_saved > 0);
        assert!(result.messages_removed > 0);
    }

    #[test]
    fn test_current_usage() {
        let config = CompressorConfig {
            max_context_tokens: 10000,
            ..Default::default()
        };
        let compressor = ContextCompressor::new(config);

        let history = create_test_history(10);
        let usage = compressor.current_usage(&history);

        assert!(usage > 0.0);
        assert!(usage < 1.0);
    }

    #[test]
    fn test_config_presets() {
        let aggressive = CompressorConfig::aggressive();
        assert!(aggressive.threshold < 0.9);
        assert!(aggressive.keep_recent_messages < 10);

        let conservative = CompressorConfig::conservative();
        assert!(conservative.threshold > 0.9);
        assert!(conservative.keep_recent_messages > 10);
    }

    #[test]
    fn test_stats_tracking() {
        let config = CompressorConfig {
            threshold: 0.1,
            max_context_tokens: 1000,
            keep_recent_messages: 2,
            ..Default::default()
        };
        let compressor = ContextCompressor::new(config);

        let mut history = create_test_history(20);
        compressor.compress(&mut history).unwrap();

        let stats = compressor.stats();
        assert_eq!(stats.compression_count, 1);
        assert!(stats.total_tokens_saved > 0);
    }
}
