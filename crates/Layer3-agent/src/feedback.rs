//! Feedback Loop System
//!
//! 연구 기반: AI Agentic Programming Survey (2025)
//! - 테스트/컴파일 실패 시 자동 재시도
//! - 피드백 기반 적응형 행동
//! - Self-correction 메커니즘
//!
//! ## 핵심 원리
//! ```text
//! Action → Result → Analyze → Decide
//!    ↑                         ↓
//!    └─── Retry/Modify ←──────┘
//! ```

use std::collections::HashMap;
use std::time::{Duration, Instant};

/// 피드백 유형
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum FeedbackType {
    /// 컴파일/빌드 실패
    BuildFailure,
    /// 테스트 실패
    TestFailure,
    /// 런타임 에러
    RuntimeError,
    /// 타임아웃
    Timeout,
    /// 권한 거부
    PermissionDenied,
    /// 성공
    Success,
}

/// 피드백 데이터
#[derive(Debug, Clone)]
pub struct Feedback {
    pub feedback_type: FeedbackType,
    pub tool_name: String,
    pub input: String,
    pub output: String,
    pub error_message: Option<String>,
    pub timestamp: Instant,
}

impl Feedback {
    pub fn success(tool_name: impl Into<String>, input: impl Into<String>, output: impl Into<String>) -> Self {
        Self {
            feedback_type: FeedbackType::Success,
            tool_name: tool_name.into(),
            input: input.into(),
            output: output.into(),
            error_message: None,
            timestamp: Instant::now(),
        }
    }

    pub fn failure(
        feedback_type: FeedbackType,
        tool_name: impl Into<String>,
        input: impl Into<String>,
        error: impl Into<String>,
    ) -> Self {
        Self {
            feedback_type,
            tool_name: tool_name.into(),
            input: input.into(),
            output: String::new(),
            error_message: Some(error.into()),
            timestamp: Instant::now(),
        }
    }
}

/// 재시도 전략
#[derive(Debug, Clone)]
pub enum RetryStrategy {
    /// 즉시 재시도 (같은 명령어)
    Immediate,
    /// 수정 후 재시도 (에러 메시지 기반)
    ModifyAndRetry { suggestion: String },
    /// 대안 시도
    TryAlternative { alternative: String },
    /// 포기 (사용자 개입 필요)
    GiveUp { reason: String },
    /// 성공 - 재시도 불필요
    NoRetryNeeded,
}

/// 피드백 분석기
#[derive(Debug, Clone)]
pub struct FeedbackAnalyzer {
    /// 재시도 횟수 제한
    max_retries: usize,
    /// 도구별 재시도 카운터
    retry_counts: HashMap<String, usize>,
    /// 최근 실패 패턴
    failure_patterns: Vec<FailurePattern>,
}

#[derive(Debug, Clone)]
struct FailurePattern {
    error_pattern: String,
    suggested_fix: String,
    confidence: f32,
}

impl Default for FeedbackAnalyzer {
    fn default() -> Self {
        Self::new()
    }
}

impl FeedbackAnalyzer {
    pub fn new() -> Self {
        Self {
            max_retries: 3,
            retry_counts: HashMap::new(),
            failure_patterns: Self::default_patterns(),
        }
    }

    pub fn with_max_retries(mut self, max: usize) -> Self {
        self.max_retries = max;
        self
    }

    /// 피드백 분석 및 재시도 전략 결정
    pub fn analyze(&mut self, feedback: &Feedback) -> RetryStrategy {
        // 성공이면 재시도 불필요
        if feedback.feedback_type == FeedbackType::Success {
            self.reset_retry_count(&feedback.tool_name);
            return RetryStrategy::NoRetryNeeded;
        }

        // 재시도 횟수 확인
        let retry_count = self.get_retry_count(&feedback.tool_name);
        if retry_count >= self.max_retries {
            return RetryStrategy::GiveUp {
                reason: format!(
                    "Maximum retries ({}) exceeded for tool '{}'",
                    self.max_retries, feedback.tool_name
                ),
            };
        }

        // 재시도 카운터 증가
        self.increment_retry_count(&feedback.tool_name);

        // 피드백 유형별 전략
        match &feedback.feedback_type {
            FeedbackType::BuildFailure => self.analyze_build_failure(feedback),
            FeedbackType::TestFailure => self.analyze_test_failure(feedback),
            FeedbackType::RuntimeError => self.analyze_runtime_error(feedback),
            FeedbackType::Timeout => RetryStrategy::Immediate,
            FeedbackType::PermissionDenied => RetryStrategy::GiveUp {
                reason: "Permission denied - user intervention required".to_string(),
            },
            FeedbackType::Success => RetryStrategy::NoRetryNeeded,
        }
    }

    /// 빌드 실패 분석
    fn analyze_build_failure(&self, feedback: &Feedback) -> RetryStrategy {
        let error = feedback.error_message.as_deref().unwrap_or("");

        // 패턴 매칭으로 수정 제안
        for pattern in &self.failure_patterns {
            if error.contains(&pattern.error_pattern) {
                return RetryStrategy::ModifyAndRetry {
                    suggestion: pattern.suggested_fix.clone(),
                };
            }
        }

        // Rust 특화 에러 분석
        if error.contains("cannot find") || error.contains("not found") {
            return RetryStrategy::ModifyAndRetry {
                suggestion: "Check imports and module paths. The referenced item may need to be imported or the path corrected.".to_string(),
            };
        }

        if error.contains("mismatched types") {
            return RetryStrategy::ModifyAndRetry {
                suggestion: "Type mismatch detected. Check the expected vs actual types and add appropriate conversions.".to_string(),
            };
        }

        if error.contains("borrow checker") || error.contains("borrowed value") {
            return RetryStrategy::ModifyAndRetry {
                suggestion: "Borrow checker error. Consider using clone(), references, or restructuring ownership.".to_string(),
            };
        }

        // 기본 재시도
        RetryStrategy::Immediate
    }

    /// 테스트 실패 분석
    fn analyze_test_failure(&self, feedback: &Feedback) -> RetryStrategy {
        let error = feedback.error_message.as_deref().unwrap_or("");

        // assertion 실패
        if error.contains("assertion") || error.contains("assert") {
            return RetryStrategy::ModifyAndRetry {
                suggestion: "Test assertion failed. Review the expected vs actual values and fix the implementation or test expectation.".to_string(),
            };
        }

        // panic
        if error.contains("panic") || error.contains("unwrap") {
            return RetryStrategy::ModifyAndRetry {
                suggestion: "Code panicked during test. Check for unwrap() on None/Err values and add proper error handling.".to_string(),
            };
        }

        RetryStrategy::Immediate
    }

    /// 런타임 에러 분석
    fn analyze_runtime_error(&self, feedback: &Feedback) -> RetryStrategy {
        let error = feedback.error_message.as_deref().unwrap_or("");

        if error.contains("No such file") || error.contains("not found") {
            return RetryStrategy::ModifyAndRetry {
                suggestion: "File or directory not found. Verify the path exists and is accessible.".to_string(),
            };
        }

        RetryStrategy::Immediate
    }

    fn get_retry_count(&self, tool_name: &str) -> usize {
        *self.retry_counts.get(tool_name).unwrap_or(&0)
    }

    fn increment_retry_count(&mut self, tool_name: &str) {
        *self.retry_counts.entry(tool_name.to_string()).or_insert(0) += 1;
    }

    fn reset_retry_count(&mut self, tool_name: &str) {
        self.retry_counts.remove(tool_name);
    }

    /// 기본 실패 패턴 정의
    fn default_patterns() -> Vec<FailurePattern> {
        vec![
            FailurePattern {
                error_pattern: "unresolved import".to_string(),
                suggested_fix: "Add the missing import statement or check the module path.".to_string(),
                confidence: 0.9,
            },
            FailurePattern {
                error_pattern: "expected struct".to_string(),
                suggested_fix: "Wrong type used. Check the expected struct type and update accordingly.".to_string(),
                confidence: 0.85,
            },
            FailurePattern {
                error_pattern: "lifetime".to_string(),
                suggested_fix: "Lifetime annotation needed. Consider adding explicit lifetimes or restructuring to avoid the issue.".to_string(),
                confidence: 0.8,
            },
            FailurePattern {
                error_pattern: "trait bound".to_string(),
                suggested_fix: "Missing trait implementation. Implement the required trait or use a type that already implements it.".to_string(),
                confidence: 0.85,
            },
        ]
    }

    /// 세션 통계 초기화
    pub fn reset(&mut self) {
        self.retry_counts.clear();
    }
}

/// 피드백 루프 매니저
#[derive(Debug)]
pub struct FeedbackLoop {
    analyzer: FeedbackAnalyzer,
    history: Vec<Feedback>,
    max_history: usize,
}

impl Default for FeedbackLoop {
    fn default() -> Self {
        Self::new()
    }
}

impl FeedbackLoop {
    pub fn new() -> Self {
        Self {
            analyzer: FeedbackAnalyzer::new(),
            history: Vec::with_capacity(100),
            max_history: 100,
        }
    }

    /// 피드백 기록 및 분석
    pub fn record(&mut self, feedback: Feedback) -> RetryStrategy {
        // 히스토리에 추가
        if self.history.len() >= self.max_history {
            self.history.remove(0);
        }
        self.history.push(feedback.clone());

        // 분석 및 전략 반환
        self.analyzer.analyze(&feedback)
    }

    /// 도구 결과를 피드백으로 변환
    pub fn from_tool_result(
        tool_name: &str,
        input: &str,
        output: &str,
        success: bool,
    ) -> Feedback {
        if success {
            Feedback::success(tool_name, input, output)
        } else {
            // 에러 유형 추론
            let feedback_type = Self::infer_feedback_type(tool_name, output);
            Feedback::failure(feedback_type, tool_name, input, output)
        }
    }

    /// 출력에서 피드백 유형 추론
    fn infer_feedback_type(tool_name: &str, output: &str) -> FeedbackType {
        let output_lower = output.to_lowercase();

        if tool_name == "bash" {
            if output_lower.contains("error") && output_lower.contains("compil") {
                return FeedbackType::BuildFailure;
            }
            if output_lower.contains("test") && output_lower.contains("fail") {
                return FeedbackType::TestFailure;
            }
            if output_lower.contains("permission denied") {
                return FeedbackType::PermissionDenied;
            }
            if output_lower.contains("timeout") || output_lower.contains("timed out") {
                return FeedbackType::Timeout;
            }
        }

        FeedbackType::RuntimeError
    }

    /// 최근 실패 횟수
    pub fn recent_failures(&self, within: Duration) -> usize {
        let now = Instant::now();
        self.history
            .iter()
            .filter(|f| {
                f.feedback_type != FeedbackType::Success
                    && now.duration_since(f.timestamp) <= within
            })
            .count()
    }

    /// 세션 초기화
    pub fn reset(&mut self) {
        self.history.clear();
        self.analyzer.reset();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_feedback_analyzer_success() {
        let mut analyzer = FeedbackAnalyzer::new();
        let feedback = Feedback::success("bash", "cargo build", "Build succeeded");
        
        let strategy = analyzer.analyze(&feedback);
        assert!(matches!(strategy, RetryStrategy::NoRetryNeeded));
    }

    #[test]
    fn test_feedback_analyzer_build_failure() {
        let mut analyzer = FeedbackAnalyzer::new();
        let feedback = Feedback::failure(
            FeedbackType::BuildFailure,
            "bash",
            "cargo build",
            "error: cannot find value `foo` in this scope",
        );
        
        let strategy = analyzer.analyze(&feedback);
        assert!(matches!(strategy, RetryStrategy::ModifyAndRetry { .. }));
    }

    #[test]
    fn test_max_retries() {
        let mut analyzer = FeedbackAnalyzer::new().with_max_retries(2);
        let feedback = Feedback::failure(
            FeedbackType::BuildFailure,
            "bash",
            "cargo build",
            "error: build failed",
        );
        
        // 첫 번째 재시도
        let _ = analyzer.analyze(&feedback);
        // 두 번째 재시도
        let _ = analyzer.analyze(&feedback);
        // 세 번째 - 포기
        let strategy = analyzer.analyze(&feedback);
        assert!(matches!(strategy, RetryStrategy::GiveUp { .. }));
    }
}
