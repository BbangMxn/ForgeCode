//! Tokenizer Trait 정의

use super::types::{EncodingResult, TokenCount, TokenizerError, TokenizerType};

/// 토크나이저 트레이트
///
/// 모든 토크나이저 구현체가 따라야 하는 인터페이스입니다.
pub trait Tokenizer: Send + Sync {
    /// 토크나이저 타입
    fn tokenizer_type(&self) -> TokenizerType;

    /// 텍스트를 토큰 수로 계산
    fn count(&self, text: &str) -> TokenCount;

    /// 텍스트를 토큰 ID로 인코딩
    fn encode(&self, text: &str) -> Result<EncodingResult, TokenizerError>;

    /// 토큰 ID를 텍스트로 디코딩
    fn decode(&self, token_ids: &[u32]) -> Result<String, TokenizerError>;

    /// 정확한 토큰 계산 지원 여부
    fn is_exact(&self) -> bool {
        false
    }

    /// 텍스트를 토큰 제한 내로 자르기
    fn truncate(&self, text: &str, max_tokens: usize) -> String {
        let count = self.count(text);
        if count.total <= max_tokens {
            return text.to_string();
        }

        // 이진 검색으로 적절한 길이 찾기
        let chars: Vec<char> = text.chars().collect();
        let mut low = 0;
        let mut high = chars.len();

        while low < high {
            let mid = (low + high + 1) / 2;
            let substr: String = chars[..mid].iter().collect();
            let sub_count = self.count(&substr);

            if sub_count.total <= max_tokens {
                low = mid;
            } else {
                high = mid - 1;
            }
        }

        chars[..low].iter().collect()
    }

    /// 텍스트를 토큰 제한 내로 자르기 (끝에서부터)
    fn truncate_from_end(&self, text: &str, max_tokens: usize) -> String {
        let count = self.count(text);
        if count.total <= max_tokens {
            return text.to_string();
        }

        let chars: Vec<char> = text.chars().collect();
        let mut low = 0;
        let mut high = chars.len();

        while low < high {
            let mid = (low + high + 1) / 2;
            let start = chars.len() - mid;
            let substr: String = chars[start..].iter().collect();
            let sub_count = self.count(&substr);

            if sub_count.total <= max_tokens {
                low = mid;
            } else {
                high = mid - 1;
            }
        }

        let start = chars.len() - low;
        chars[start..].iter().collect()
    }

    /// 여러 텍스트의 토큰 수 합계
    fn count_many(&self, texts: &[&str]) -> TokenCount {
        let mut total = 0;
        let mut char_count = 0;

        for text in texts {
            let count = self.count(text);
            total += count.total;
            char_count += text.chars().count();
        }

        TokenCount {
            total,
            is_exact: self.is_exact(),
            char_count,
            tokenizer_type: self.tokenizer_type(),
        }
    }

    /// 텍스트가 토큰 제한을 초과하는지 확인
    fn exceeds_limit(&self, text: &str, max_tokens: usize) -> bool {
        self.count(text).total > max_tokens
    }

    /// 토큰 제한까지 남은 토큰 수
    fn remaining(&self, text: &str, max_tokens: usize) -> isize {
        max_tokens as isize - self.count(text).total as isize
    }
}

/// 메시지 토큰 계산을 위한 확장 트레이트
pub trait MessageTokenizer: Tokenizer {
    /// 메시지당 오버헤드 토큰 수
    fn message_overhead(&self) -> usize {
        4 // 기본값: role, content 등 메타데이터
    }

    /// 시스템 프롬프트 오버헤드
    fn system_overhead(&self) -> usize {
        10
    }

    /// 채팅 메시지 배열의 토큰 수 계산
    fn count_messages<M>(&self, messages: &[M]) -> TokenCount
    where
        M: AsRef<str>,
    {
        let mut total = 0;

        for msg in messages {
            total += self.count(msg.as_ref()).total;
            total += self.message_overhead();
        }

        TokenCount {
            total,
            is_exact: self.is_exact(),
            char_count: messages.iter().map(|m| m.as_ref().len()).sum(),
            tokenizer_type: self.tokenizer_type(),
        }
    }
}

/// 모든 Tokenizer는 자동으로 MessageTokenizer 구현
impl<T: Tokenizer + ?Sized> MessageTokenizer for T {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tokenizer::estimator::EstimateTokenizer;

    #[test]
    fn test_truncate() {
        let tokenizer = EstimateTokenizer::new(TokenizerType::Estimate);
        let text = "Hello, this is a test string that might be too long.";

        let truncated = tokenizer.truncate(text, 5);
        let count = tokenizer.count(&truncated);
        assert!(count.total <= 5);
    }

    #[test]
    fn test_count_many() {
        let tokenizer = EstimateTokenizer::new(TokenizerType::Estimate);
        let texts = vec!["Hello", "World", "Test"];

        let count = tokenizer.count_many(&texts);
        assert!(count.total > 0);
    }

    #[test]
    fn test_exceeds_limit() {
        let tokenizer = EstimateTokenizer::new(TokenizerType::Estimate);

        assert!(!tokenizer.exceeds_limit("Hi", 100));
        assert!(tokenizer.exceeds_limit("A".repeat(1000).as_str(), 10));
    }
}
