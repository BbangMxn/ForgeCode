//! Clipboard Manager - 클립보드 복사 기능
//!
//! 코드 블록 복사 등 클립보드 작업 처리

#![allow(dead_code)]

use std::sync::OnceLock;

/// 클립보드 매니저
pub struct ClipboardManager {
    clipboard: Option<arboard::Clipboard>,
}

static CLIPBOARD: OnceLock<std::sync::Mutex<ClipboardManager>> = OnceLock::new();

impl ClipboardManager {
    /// 새 클립보드 매니저 생성
    pub fn new() -> Self {
        let clipboard = arboard::Clipboard::new().ok();
        Self { clipboard }
    }

    /// 전역 인스턴스 가져오기
    pub fn global() -> &'static std::sync::Mutex<ClipboardManager> {
        CLIPBOARD.get_or_init(|| std::sync::Mutex::new(ClipboardManager::new()))
    }

    /// 텍스트 복사
    pub fn copy(&mut self, text: &str) -> Result<(), ClipboardError> {
        match &mut self.clipboard {
            Some(cb) => cb
                .set_text(text)
                .map_err(|e| ClipboardError::SetFailed(e.to_string())),
            None => Err(ClipboardError::NotAvailable),
        }
    }

    /// 텍스트 붙여넣기
    pub fn paste(&mut self) -> Result<String, ClipboardError> {
        match &mut self.clipboard {
            Some(cb) => cb
                .get_text()
                .map_err(|e| ClipboardError::GetFailed(e.to_string())),
            None => Err(ClipboardError::NotAvailable),
        }
    }

    /// 클립보드 사용 가능 여부
    pub fn is_available(&self) -> bool {
        self.clipboard.is_some()
    }
}

impl Default for ClipboardManager {
    fn default() -> Self {
        Self::new()
    }
}

/// 클립보드 에러
#[derive(Debug, Clone)]
pub enum ClipboardError {
    NotAvailable,
    SetFailed(String),
    GetFailed(String),
}

impl std::fmt::Display for ClipboardError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ClipboardError::NotAvailable => write!(f, "Clipboard not available"),
            ClipboardError::SetFailed(e) => write!(f, "Failed to set clipboard: {}", e),
            ClipboardError::GetFailed(e) => write!(f, "Failed to get clipboard: {}", e),
        }
    }
}

impl std::error::Error for ClipboardError {}

/// 코드 블록 복사 헬퍼
pub fn copy_code_block(code: &str) -> Result<(), ClipboardError> {
    let mut mgr = ClipboardManager::global()
        .lock()
        .map_err(|_| ClipboardError::NotAvailable)?;
    mgr.copy(code)
}

/// 붙여넣기 헬퍼
pub fn paste_text() -> Result<String, ClipboardError> {
    let mut mgr = ClipboardManager::global()
        .lock()
        .map_err(|_| ClipboardError::NotAvailable)?;
    mgr.paste()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clipboard_creation() {
        let mgr = ClipboardManager::new();
        // 클립보드 사용 가능 여부는 환경에 따라 다름
        // CI 환경에서는 사용 불가능할 수 있음
        let _ = mgr.is_available();
    }

    #[test]
    fn test_copy_paste() {
        let mut mgr = ClipboardManager::new();
        
        if mgr.is_available() {
            let test_text = "Hello, clipboard!";
            assert!(mgr.copy(test_text).is_ok());
            
            let pasted = mgr.paste();
            assert!(pasted.is_ok());
            assert_eq!(pasted.unwrap(), test_text);
        }
    }
}
