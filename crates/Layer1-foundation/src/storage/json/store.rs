//! JSON 파일 저장소

use crate::{Error, Result};
use serde::{de::DeserializeOwned, Serialize};
use std::path::{Path, PathBuf};

/// JSON 설정 저장소
#[derive(Debug, Clone)]
pub struct JsonStore {
    base_dir: PathBuf,
}

impl JsonStore {
    pub fn new(base_dir: impl Into<PathBuf>) -> Self {
        Self {
            base_dir: base_dir.into(),
        }
    }

    /// 글로벌 설정 (~/.forgecode/)
    pub fn global() -> Result<Self> {
        let dir = dirs::config_dir()
            .ok_or_else(|| Error::Config("Cannot find config directory".to_string()))?
            .join("forgecode");
        Ok(Self::new(dir))
    }

    /// 프로젝트 설정 (.forgecode/)
    pub fn project(root: impl Into<PathBuf>) -> Self {
        Self::new(root.into().join(".forgecode"))
    }

    /// 현재 디렉토리 프로젝트 설정 (.forgecode/ 만 사용)
    pub fn current_project() -> Result<Self> {
        let cwd = std::env::current_dir()
            .map_err(|e| Error::Config(format!("Cannot get current directory: {}", e)))?;
        Ok(Self::project(cwd))
    }

    pub fn base_dir(&self) -> &Path {
        &self.base_dir
    }

    pub fn file_path(&self, filename: &str) -> PathBuf {
        self.base_dir.join(filename)
    }

    fn ensure_dir(&self) -> Result<()> {
        if !self.base_dir.exists() {
            std::fs::create_dir_all(&self.base_dir)
                .map_err(|e| Error::Config(format!("Failed to create directory: {}", e)))?;
        }
        Ok(())
    }

    /// JSON 로드
    pub fn load<T: DeserializeOwned>(&self, filename: &str) -> Result<T> {
        let path = self.file_path(filename);
        let content = std::fs::read_to_string(&path)
            .map_err(|e| Error::Config(format!("Failed to read {}: {}", path.display(), e)))?;
        serde_json::from_str(&content)
            .map_err(|e| Error::Config(format!("Failed to parse {}: {}", path.display(), e)))
    }

    /// JSON 로드 (기본값)
    pub fn load_or_default<T: DeserializeOwned + Default>(&self, filename: &str) -> T {
        self.load(filename).unwrap_or_default()
    }

    /// JSON 로드 (Optional)
    pub fn load_optional<T: DeserializeOwned>(&self, filename: &str) -> Result<Option<T>> {
        let path = self.file_path(filename);
        if !path.exists() {
            return Ok(None);
        }
        self.load(filename).map(Some)
    }

    /// JSON 저장
    pub fn save<T: Serialize>(&self, filename: &str, data: &T) -> Result<()> {
        self.ensure_dir()?;
        let path = self.file_path(filename);
        let content = serde_json::to_string_pretty(data)
            .map_err(|e| Error::Config(format!("Failed to serialize: {}", e)))?;
        std::fs::write(&path, content)
            .map_err(|e| Error::Config(format!("Failed to write {}: {}", path.display(), e)))
    }

    /// 파일 존재 여부
    pub fn exists(&self, filename: &str) -> bool {
        self.file_path(filename).exists()
    }

    /// 파일 삭제
    pub fn remove(&self, filename: &str) -> Result<()> {
        let path = self.file_path(filename);
        if path.exists() {
            std::fs::remove_file(&path).map_err(|e| {
                Error::Config(format!("Failed to remove {}: {}", path.display(), e))
            })?;
        }
        Ok(())
    }
}
