//! Resource Monitor - Task별 CPU/Memory 사용량 모니터링
//!
//! Local/PTY executor에서 실행되는 프로세스의 리소스 사용량을 추적합니다.
//!
//! ## 기능
//! - CPU 사용률 추적
//! - 메모리 사용량 추적
//! - 리소스 제한 초과 시 경고/종료
//! - 리소스 사용 기록 (히스토리)
//!
//! ## 플랫폼 지원
//! - Windows: WMI/PDH API를 통한 프로세스 모니터링
//! - Unix: /proc 파일시스템 또는 sysinfo를 통한 모니터링

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tracing::{debug, warn};

/// 리소스 사용량 스냅샷
#[derive(Debug, Clone)]
pub struct ResourceSnapshot {
    /// CPU 사용률 (0.0 - 100.0+, 멀티코어 시 100% 초과 가능)
    pub cpu_percent: f64,

    /// 메모리 사용량 (bytes)
    pub memory_bytes: u64,

    /// 가상 메모리 사용량 (bytes)
    pub virtual_memory_bytes: u64,

    /// 디스크 읽기 (bytes)
    pub disk_read_bytes: u64,

    /// 디스크 쓰기 (bytes)
    pub disk_write_bytes: u64,

    /// 네트워크 수신 (bytes)
    pub network_recv_bytes: u64,

    /// 네트워크 송신 (bytes)
    pub network_sent_bytes: u64,

    /// 스레드 수
    pub thread_count: u32,

    /// 측정 시간
    pub timestamp: Instant,
}

impl Default for ResourceSnapshot {
    fn default() -> Self {
        Self {
            cpu_percent: 0.0,
            memory_bytes: 0,
            virtual_memory_bytes: 0,
            disk_read_bytes: 0,
            disk_write_bytes: 0,
            network_recv_bytes: 0,
            network_sent_bytes: 0,
            thread_count: 0,
            timestamp: Instant::now(),
        }
    }
}

impl ResourceSnapshot {
    pub fn new() -> Self {
        Self {
            timestamp: Instant::now(),
            ..Default::default()
        }
    }

    /// 메모리를 사람이 읽기 좋은 형식으로 포맷
    pub fn memory_human(&self) -> String {
        format_bytes(self.memory_bytes)
    }

    /// 가상 메모리를 사람이 읽기 좋은 형식으로 포맷
    pub fn virtual_memory_human(&self) -> String {
        format_bytes(self.virtual_memory_bytes)
    }
}

/// 바이트를 사람이 읽기 좋은 형식으로 변환
fn format_bytes(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;

    if bytes >= GB {
        format!("{:.2} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.2} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.2} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}

/// 프로세스 리소스 제한
#[derive(Debug, Clone)]
pub struct ProcessResourceLimits {
    /// 최대 CPU 사용률 (%)
    pub max_cpu_percent: Option<f64>,

    /// 최대 메모리 (bytes)
    pub max_memory_bytes: Option<u64>,

    /// 최대 가상 메모리 (bytes)
    pub max_virtual_memory_bytes: Option<u64>,

    /// 최대 실행 시간
    pub max_duration: Option<Duration>,

    /// 제한 초과 시 동작
    pub on_limit_exceeded: LimitExceededAction,
}

impl Default for ProcessResourceLimits {
    fn default() -> Self {
        Self {
            max_cpu_percent: None,
            max_memory_bytes: Some(4 * 1024 * 1024 * 1024), // 4GB
            max_virtual_memory_bytes: None,
            max_duration: Some(Duration::from_secs(600)), // 10분
            on_limit_exceeded: LimitExceededAction::Warn,
        }
    }
}

impl ProcessResourceLimits {
    /// 제한 없음
    pub fn unlimited() -> Self {
        Self {
            max_cpu_percent: None,
            max_memory_bytes: None,
            max_virtual_memory_bytes: None,
            max_duration: None,
            on_limit_exceeded: LimitExceededAction::Warn,
        }
    }

    /// 엄격한 제한
    pub fn strict() -> Self {
        Self {
            max_cpu_percent: Some(100.0), // 단일 코어 100%
            max_memory_bytes: Some(1024 * 1024 * 1024), // 1GB
            max_virtual_memory_bytes: Some(2 * 1024 * 1024 * 1024), // 2GB
            max_duration: Some(Duration::from_secs(120)), // 2분
            on_limit_exceeded: LimitExceededAction::Kill,
        }
    }

    /// 메모리 제한을 파싱 (예: "512m", "2g")
    pub fn with_memory_limit(mut self, limit: &str) -> Self {
        self.max_memory_bytes = parse_memory_string(limit);
        self
    }

    /// CPU 제한 설정
    pub fn with_cpu_limit(mut self, percent: f64) -> Self {
        self.max_cpu_percent = Some(percent);
        self
    }

    /// 시간 제한 설정
    pub fn with_duration_limit(mut self, duration: Duration) -> Self {
        self.max_duration = Some(duration);
        self
    }

    /// 제한 초과 동작 설정
    pub fn with_action(mut self, action: LimitExceededAction) -> Self {
        self.on_limit_exceeded = action;
        self
    }
}

/// 메모리 문자열 파싱 (예: "512m", "2g", "1024k")
fn parse_memory_string(s: &str) -> Option<u64> {
    let s = s.trim().to_lowercase();

    if s.is_empty() {
        return None;
    }

    let (num_str, unit) = if s.ends_with("gb") || s.ends_with('g') {
        let num_part = s.trim_end_matches("gb").trim_end_matches('g');
        (num_part, 1024 * 1024 * 1024u64)
    } else if s.ends_with("mb") || s.ends_with('m') {
        let num_part = s.trim_end_matches("mb").trim_end_matches('m');
        (num_part, 1024 * 1024u64)
    } else if s.ends_with("kb") || s.ends_with('k') {
        let num_part = s.trim_end_matches("kb").trim_end_matches('k');
        (num_part, 1024u64)
    } else if s.ends_with('b') {
        let num_part = s.trim_end_matches('b');
        (num_part, 1u64)
    } else {
        // 숫자만 있으면 바이트로 간주
        (s.as_str(), 1u64)
    };

    num_str.parse::<f64>().ok().map(|n| (n * unit as f64) as u64)
}

/// 제한 초과 시 동작
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LimitExceededAction {
    /// 경고만 로깅
    Warn,
    /// 프로세스 일시 중지 (SIGSTOP)
    Pause,
    /// 프로세스 종료 (SIGTERM)
    Terminate,
    /// 강제 종료 (SIGKILL)
    Kill,
}

/// 리소스 위반 이벤트
#[derive(Debug, Clone)]
pub struct ResourceViolation {
    /// 위반 종류
    pub violation_type: ViolationType,
    /// 현재 값
    pub current_value: f64,
    /// 제한 값
    pub limit_value: f64,
    /// 발생 시간
    pub timestamp: Instant,
    /// 취한 동작
    pub action_taken: LimitExceededAction,
}

/// 위반 종류
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViolationType {
    CpuExceeded,
    MemoryExceeded,
    VirtualMemoryExceeded,
    DurationExceeded,
}

impl std::fmt::Display for ViolationType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::CpuExceeded => write!(f, "CPU limit exceeded"),
            Self::MemoryExceeded => write!(f, "Memory limit exceeded"),
            Self::VirtualMemoryExceeded => write!(f, "Virtual memory limit exceeded"),
            Self::DurationExceeded => write!(f, "Duration limit exceeded"),
        }
    }
}

/// 프로세스 리소스 추적기
#[derive(Debug)]
pub struct ProcessResourceTracker {
    /// 프로세스 ID
    pub pid: u32,

    /// 시작 시간
    pub started_at: Instant,

    /// 리소스 제한
    pub limits: ProcessResourceLimits,

    /// 리소스 히스토리 (최근 N개)
    history: Vec<ResourceSnapshot>,

    /// 히스토리 최대 크기
    history_max_size: usize,

    /// 위반 기록
    violations: Vec<ResourceViolation>,

    /// 피크 CPU
    pub peak_cpu_percent: f64,

    /// 피크 메모리
    pub peak_memory_bytes: u64,
}

impl ProcessResourceTracker {
    pub fn new(pid: u32, limits: ProcessResourceLimits) -> Self {
        Self {
            pid,
            started_at: Instant::now(),
            limits,
            history: Vec::with_capacity(100),
            history_max_size: 100,
            violations: Vec::new(),
            peak_cpu_percent: 0.0,
            peak_memory_bytes: 0,
        }
    }

    /// 리소스 스냅샷 기록
    pub fn record_snapshot(&mut self, snapshot: ResourceSnapshot) {
        // 피크 업데이트
        if snapshot.cpu_percent > self.peak_cpu_percent {
            self.peak_cpu_percent = snapshot.cpu_percent;
        }
        if snapshot.memory_bytes > self.peak_memory_bytes {
            self.peak_memory_bytes = snapshot.memory_bytes;
        }

        // 히스토리에 추가
        if self.history.len() >= self.history_max_size {
            self.history.remove(0);
        }
        self.history.push(snapshot);
    }

    /// 제한 확인 및 위반 기록
    pub fn check_limits(&mut self, snapshot: &ResourceSnapshot) -> Option<ResourceViolation> {
        // CPU 확인
        if let Some(max_cpu) = self.limits.max_cpu_percent {
            if snapshot.cpu_percent > max_cpu {
                let violation = ResourceViolation {
                    violation_type: ViolationType::CpuExceeded,
                    current_value: snapshot.cpu_percent,
                    limit_value: max_cpu,
                    timestamp: Instant::now(),
                    action_taken: self.limits.on_limit_exceeded,
                };
                warn!(
                    "PID {}: CPU usage {:.1}% exceeds limit {:.1}%",
                    self.pid, snapshot.cpu_percent, max_cpu
                );
                self.violations.push(violation.clone());
                return Some(violation);
            }
        }

        // 메모리 확인
        if let Some(max_mem) = self.limits.max_memory_bytes {
            if snapshot.memory_bytes > max_mem {
                let violation = ResourceViolation {
                    violation_type: ViolationType::MemoryExceeded,
                    current_value: snapshot.memory_bytes as f64,
                    limit_value: max_mem as f64,
                    timestamp: Instant::now(),
                    action_taken: self.limits.on_limit_exceeded,
                };
                warn!(
                    "PID {}: Memory {} exceeds limit {}",
                    self.pid,
                    format_bytes(snapshot.memory_bytes),
                    format_bytes(max_mem)
                );
                self.violations.push(violation.clone());
                return Some(violation);
            }
        }

        // 가상 메모리 확인
        if let Some(max_vmem) = self.limits.max_virtual_memory_bytes {
            if snapshot.virtual_memory_bytes > max_vmem {
                let violation = ResourceViolation {
                    violation_type: ViolationType::VirtualMemoryExceeded,
                    current_value: snapshot.virtual_memory_bytes as f64,
                    limit_value: max_vmem as f64,
                    timestamp: Instant::now(),
                    action_taken: self.limits.on_limit_exceeded,
                };
                warn!(
                    "PID {}: Virtual memory {} exceeds limit {}",
                    self.pid,
                    format_bytes(snapshot.virtual_memory_bytes),
                    format_bytes(max_vmem)
                );
                self.violations.push(violation.clone());
                return Some(violation);
            }
        }

        // 실행 시간 확인
        if let Some(max_dur) = self.limits.max_duration {
            let elapsed = self.started_at.elapsed();
            if elapsed > max_dur {
                let violation = ResourceViolation {
                    violation_type: ViolationType::DurationExceeded,
                    current_value: elapsed.as_secs_f64(),
                    limit_value: max_dur.as_secs_f64(),
                    timestamp: Instant::now(),
                    action_taken: self.limits.on_limit_exceeded,
                };
                warn!(
                    "PID {}: Duration {:.1}s exceeds limit {:.1}s",
                    self.pid,
                    elapsed.as_secs_f64(),
                    max_dur.as_secs_f64()
                );
                self.violations.push(violation.clone());
                return Some(violation);
            }
        }

        None
    }

    /// 평균 CPU 사용률
    pub fn average_cpu(&self) -> f64 {
        if self.history.is_empty() {
            return 0.0;
        }
        let sum: f64 = self.history.iter().map(|s| s.cpu_percent).sum();
        sum / self.history.len() as f64
    }

    /// 평균 메모리 사용량
    pub fn average_memory(&self) -> u64 {
        if self.history.is_empty() {
            return 0;
        }
        let sum: u64 = self.history.iter().map(|s| s.memory_bytes).sum();
        sum / self.history.len() as u64
    }

    /// 최근 스냅샷
    pub fn latest_snapshot(&self) -> Option<&ResourceSnapshot> {
        self.history.last()
    }

    /// 위반 기록
    pub fn violations(&self) -> &[ResourceViolation] {
        &self.violations
    }

    /// 실행 시간
    pub fn elapsed(&self) -> Duration {
        self.started_at.elapsed()
    }

    /// 요약 리포트 생성
    pub fn summary_report(&self) -> String {
        format!(
            "PID {}: Duration {:.1}s, Peak CPU {:.1}%, Peak Mem {}, Avg CPU {:.1}%, Avg Mem {}, Violations: {}",
            self.pid,
            self.elapsed().as_secs_f64(),
            self.peak_cpu_percent,
            format_bytes(self.peak_memory_bytes),
            self.average_cpu(),
            format_bytes(self.average_memory()),
            self.violations.len()
        )
    }
}

/// 리소스 모니터 (여러 프로세스 관리)
pub struct ResourceMonitor {
    /// 추적 중인 프로세스들
    trackers: Arc<RwLock<HashMap<u32, ProcessResourceTracker>>>,

    /// 기본 리소스 제한
    default_limits: ProcessResourceLimits,

    /// 모니터링 간격
    poll_interval: Duration,
}

impl Default for ResourceMonitor {
    fn default() -> Self {
        Self::new(ProcessResourceLimits::default())
    }
}

impl ResourceMonitor {
    pub fn new(default_limits: ProcessResourceLimits) -> Self {
        Self {
            trackers: Arc::new(RwLock::new(HashMap::new())),
            default_limits,
            poll_interval: Duration::from_millis(500),
        }
    }

    /// 모니터링 간격 설정
    pub fn with_poll_interval(mut self, interval: Duration) -> Self {
        self.poll_interval = interval;
        self
    }

    /// 프로세스 추가
    pub async fn track(&self, pid: u32, limits: Option<ProcessResourceLimits>) {
        let limits = limits.unwrap_or_else(|| self.default_limits.clone());
        let tracker = ProcessResourceTracker::new(pid, limits);

        let mut trackers = self.trackers.write().await;
        trackers.insert(pid, tracker);

        debug!("Started tracking PID {}", pid);
    }

    /// 프로세스 제거
    pub async fn untrack(&self, pid: u32) -> Option<ProcessResourceTracker> {
        let mut trackers = self.trackers.write().await;
        let tracker = trackers.remove(&pid);

        if tracker.is_some() {
            debug!("Stopped tracking PID {}", pid);
        }

        tracker
    }

    /// 모든 프로세스 스냅샷 수집 및 제한 확인
    ///
    /// 실제 구현은 플랫폼별로 달라야 하지만, 여기서는 기본 구조만 제공
    pub async fn collect_snapshots(&self) -> Vec<(u32, ResourceSnapshot)> {
        let trackers = self.trackers.read().await;
        let mut snapshots = Vec::new();

        for pid in trackers.keys() {
            // 실제 구현: sysinfo 크레이트 또는 플랫폼별 API 사용
            // 여기서는 더미 스냅샷 생성
            let snapshot = ResourceSnapshot::new();
            snapshots.push((*pid, snapshot));
        }

        snapshots
    }

    /// 모든 프로세스에 대해 제한 확인
    pub async fn check_all_limits(&self) -> Vec<(u32, ResourceViolation)> {
        let snapshots = self.collect_snapshots().await;
        let mut violations = Vec::new();

        let mut trackers = self.trackers.write().await;

        for (pid, snapshot) in snapshots {
            if let Some(tracker) = trackers.get_mut(&pid) {
                tracker.record_snapshot(snapshot.clone());

                if let Some(violation) = tracker.check_limits(&snapshot) {
                    violations.push((pid, violation));
                }
            }
        }

        violations
    }

    /// 특정 프로세스의 현재 상태
    pub async fn get_status(&self, pid: u32) -> Option<ResourceSnapshot> {
        let trackers = self.trackers.read().await;
        trackers.get(&pid).and_then(|t| t.latest_snapshot().cloned())
    }

    /// 특정 프로세스의 요약 리포트
    pub async fn get_summary(&self, pid: u32) -> Option<String> {
        let trackers = self.trackers.read().await;
        trackers.get(&pid).map(|t| t.summary_report())
    }

    /// 추적 중인 모든 프로세스 ID
    pub async fn tracked_pids(&self) -> Vec<u32> {
        let trackers = self.trackers.read().await;
        trackers.keys().copied().collect()
    }

    /// 모든 프로세스 요약
    pub async fn all_summaries(&self) -> Vec<(u32, String)> {
        let trackers = self.trackers.read().await;
        trackers
            .iter()
            .map(|(pid, tracker)| (*pid, tracker.summary_report()))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_bytes() {
        assert_eq!(format_bytes(512), "512 B");
        assert_eq!(format_bytes(1024), "1.00 KB");
        assert_eq!(format_bytes(1536), "1.50 KB");
        assert_eq!(format_bytes(1024 * 1024), "1.00 MB");
        assert_eq!(format_bytes(1024 * 1024 * 1024), "1.00 GB");
    }

    #[test]
    fn test_parse_memory_string() {
        assert_eq!(parse_memory_string("512m"), Some(512 * 1024 * 1024));
        assert_eq!(parse_memory_string("2g"), Some(2 * 1024 * 1024 * 1024));
        assert_eq!(parse_memory_string("1024k"), Some(1024 * 1024));
        assert_eq!(parse_memory_string("1024"), Some(1024));
        assert_eq!(parse_memory_string("2.5g"), Some((2.5 * 1024.0 * 1024.0 * 1024.0) as u64));
    }

    #[test]
    fn test_resource_limits() {
        let limits = ProcessResourceLimits::default();
        assert!(limits.max_memory_bytes.is_some());
        assert!(limits.max_duration.is_some());

        let strict = ProcessResourceLimits::strict();
        assert!(strict.max_cpu_percent.is_some());
        assert_eq!(strict.on_limit_exceeded, LimitExceededAction::Kill);

        let unlimited = ProcessResourceLimits::unlimited();
        assert!(unlimited.max_memory_bytes.is_none());
        assert!(unlimited.max_duration.is_none());
    }

    #[test]
    fn test_resource_tracker() {
        let limits = ProcessResourceLimits::default()
            .with_memory_limit("100m")
            .with_cpu_limit(50.0);

        let mut tracker = ProcessResourceTracker::new(1234, limits);

        // 정상 스냅샷
        let snapshot = ResourceSnapshot {
            cpu_percent: 25.0,
            memory_bytes: 50 * 1024 * 1024, // 50MB
            ..Default::default()
        };
        tracker.record_snapshot(snapshot.clone());
        assert!(tracker.check_limits(&snapshot).is_none());

        // 메모리 초과 스냅샷
        let violation_snapshot = ResourceSnapshot {
            cpu_percent: 25.0,
            memory_bytes: 150 * 1024 * 1024, // 150MB > 100MB
            ..Default::default()
        };
        tracker.record_snapshot(violation_snapshot.clone());
        let violation = tracker.check_limits(&violation_snapshot);
        assert!(violation.is_some());
        assert_eq!(violation.unwrap().violation_type, ViolationType::MemoryExceeded);
    }

    #[tokio::test]
    async fn test_resource_monitor() {
        let monitor = ResourceMonitor::default();

        // 프로세스 추가
        monitor.track(1234, None).await;
        monitor.track(5678, Some(ProcessResourceLimits::strict())).await;

        let pids = monitor.tracked_pids().await;
        assert_eq!(pids.len(), 2);
        assert!(pids.contains(&1234));
        assert!(pids.contains(&5678));

        // 프로세스 제거
        let removed = monitor.untrack(1234).await;
        assert!(removed.is_some());

        let pids = monitor.tracked_pids().await;
        assert_eq!(pids.len(), 1);
    }
}
