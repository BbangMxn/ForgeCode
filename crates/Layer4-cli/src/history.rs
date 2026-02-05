//! History Manager - 입력 히스토리 영구 저장
//!
//! 쉘처럼 입력 히스토리를 파일에 저장하고 복원

use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;

/// 히스토리 매니저
pub struct HistoryManager {
    /// 히스토리 파일 경로
    path: PathBuf,
    /// 메모리 내 히스토리
    entries: Vec<String>,
    /// 현재 인덱스 (탐색용)
    index: Option<usize>,
    /// 최대 항목 수
    max_entries: usize,
    /// 임시 입력 (히스토리 탐색 중 현재 입력 저장)
    temp_input: Option<String>,
}

impl HistoryManager {
    /// 새 히스토리 매니저 생성
    pub fn new() -> Self {
        let path = dirs::data_local_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".forgecode")
            .join("history.txt");

        let mut mgr = Self {
            path,
            entries: Vec::new(),
            index: None,
            max_entries: 1000,
            temp_input: None,
        };

        mgr.load();
        mgr
    }

    /// 커스텀 경로로 생성
    pub fn with_path(path: PathBuf) -> Self {
        let mut mgr = Self {
            path,
            entries: Vec::new(),
            index: None,
            max_entries: 1000,
            temp_input: None,
        };

        mgr.load();
        mgr
    }

    /// 최대 항목 수 설정
    pub fn with_max_entries(mut self, max: usize) -> Self {
        self.max_entries = max;
        self.truncate();
        self
    }

    /// 파일에서 히스토리 로드
    pub fn load(&mut self) {
        if let Ok(file) = File::open(&self.path) {
            let reader = BufReader::new(file);
            self.entries = reader
                .lines()
                .filter_map(|l| l.ok())
                .filter(|l| !l.is_empty())
                .collect();
            self.truncate();
        }
    }

    /// 파일에 저장
    pub fn save(&self) {
        // 디렉토리 생성
        if let Some(parent) = self.path.parent() {
            let _ = fs::create_dir_all(parent);
        }

        if let Ok(mut file) = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&self.path)
        {
            for entry in &self.entries {
                let _ = writeln!(file, "{}", entry);
            }
        }
    }

    /// 항목 추가
    pub fn add(&mut self, entry: impl Into<String>) {
        let entry = entry.into();
        
        // 빈 문자열 무시
        if entry.trim().is_empty() {
            return;
        }

        // 중복 제거 (마지막 항목과 같으면 추가 안 함)
        if self.entries.last().map(|e| e == &entry).unwrap_or(false) {
            return;
        }

        // 동일 항목이 있으면 제거하고 끝에 추가 (최근 우선)
        self.entries.retain(|e| e != &entry);
        self.entries.push(entry);

        self.truncate();
        self.save();
        self.reset_navigation();
    }

    /// 최대 항목 수 유지
    fn truncate(&mut self) {
        if self.entries.len() > self.max_entries {
            let excess = self.entries.len() - self.max_entries;
            self.entries.drain(0..excess);
        }
    }

    /// 히스토리 탐색 시작 (현재 입력 저장)
    pub fn start_navigation(&mut self, current_input: &str) {
        if self.index.is_none() {
            self.temp_input = Some(current_input.to_string());
        }
    }

    /// 이전 항목 (위 화살표)
    pub fn previous(&mut self, current_input: &str) -> Option<&str> {
        if self.entries.is_empty() {
            return None;
        }

        self.start_navigation(current_input);

        let new_index = match self.index {
            Some(0) => return Some(&self.entries[0]),
            Some(i) => i - 1,
            None => self.entries.len() - 1,
        };

        self.index = Some(new_index);
        self.entries.get(new_index).map(|s| s.as_str())
    }

    /// 다음 항목 (아래 화살표)
    pub fn next(&mut self) -> Option<&str> {
        match self.index {
            Some(i) if i < self.entries.len() - 1 => {
                self.index = Some(i + 1);
                self.entries.get(i + 1).map(|s| s.as_str())
            }
            Some(_) => {
                // 마지막 항목에서 더 내려가면 임시 입력 복원
                self.index = None;
                self.temp_input.as_deref()
            }
            None => None,
        }
    }

    /// 탐색 초기화
    pub fn reset_navigation(&mut self) {
        self.index = None;
        self.temp_input = None;
    }

    /// 전체 항목 수
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// 비어있는지
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// 모든 항목 (최신순)
    pub fn entries(&self) -> impl Iterator<Item = &str> {
        self.entries.iter().rev().map(|s| s.as_str())
    }

    /// 검색
    pub fn search(&self, query: &str) -> Vec<&str> {
        let query_lower = query.to_lowercase();
        self.entries
            .iter()
            .rev()
            .filter(|e| e.to_lowercase().contains(&query_lower))
            .map(|s| s.as_str())
            .collect()
    }

    /// 히스토리 삭제
    pub fn clear(&mut self) {
        self.entries.clear();
        self.reset_navigation();
        self.save();
    }

    /// 현재 탐색 인덱스
    pub fn current_index(&self) -> Option<usize> {
        self.index
    }

    /// 탐색 중인지
    pub fn is_navigating(&self) -> bool {
        self.index.is_some()
    }
}

impl Default for HistoryManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env::temp_dir;

    fn temp_history() -> HistoryManager {
        let path = temp_dir().join(format!("forgecode_history_{}.txt", uuid::Uuid::new_v4()));
        HistoryManager::with_path(path)
    }

    #[test]
    fn test_add_and_retrieve() {
        let mut history = temp_history();
        history.add("first command");
        history.add("second command");

        assert_eq!(history.len(), 2);
        
        let entries: Vec<_> = history.entries().collect();
        assert_eq!(entries, vec!["second command", "first command"]);
    }

    #[test]
    fn test_navigation() {
        let mut history = temp_history();
        history.add("cmd1");
        history.add("cmd2");
        history.add("cmd3");

        // 위로 탐색
        assert_eq!(history.previous("current"), Some("cmd3"));
        assert_eq!(history.previous("current"), Some("cmd2"));
        assert_eq!(history.previous("current"), Some("cmd1"));

        // 아래로 탐색
        assert_eq!(history.next(), Some("cmd2"));
        assert_eq!(history.next(), Some("cmd3"));
        assert_eq!(history.next(), Some("current")); // temp_input 복원
    }

    #[test]
    fn test_no_duplicates() {
        let mut history = temp_history();
        history.add("same");
        history.add("different");
        history.add("same"); // 중복

        assert_eq!(history.len(), 2);
        
        let entries: Vec<_> = history.entries().collect();
        assert_eq!(entries, vec!["same", "different"]);
    }

    #[test]
    fn test_persistence() {
        let path = temp_dir().join(format!("forgecode_history_{}.txt", uuid::Uuid::new_v4()));
        
        // 저장
        {
            let mut history = HistoryManager::with_path(path.clone());
            history.add("persistent cmd");
        }

        // 다시 로드
        {
            let history = HistoryManager::with_path(path);
            assert_eq!(history.len(), 1);
            assert_eq!(history.entries().next(), Some("persistent cmd"));
        }
    }

    #[test]
    fn test_search() {
        let mut history = temp_history();
        history.add("git commit -m 'test'");
        history.add("cargo build");
        history.add("git push");

        let results = history.search("git");
        assert_eq!(results.len(), 2);
        assert!(results.contains(&"git push"));
        assert!(results.contains(&"git commit -m 'test'"));
    }

    #[test]
    fn test_max_entries() {
        let mut history = temp_history().with_max_entries(3);
        history.add("cmd1");
        history.add("cmd2");
        history.add("cmd3");
        history.add("cmd4"); // cmd1 삭제됨

        assert_eq!(history.len(), 3);
        let entries: Vec<_> = history.entries().collect();
        assert!(!entries.contains(&"cmd1"));
    }
}
