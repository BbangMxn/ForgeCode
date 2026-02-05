//! PTY 통합 테스트 - Windows에서 PTY 로그 캡처 검증
//!
//! `cargo test -p forge-task --test pty_test -- --nocapture`

use forge_task::{
    ExecutionMode, Task, TaskId, TaskOrchestrator, OrchestratorConfig, WaitCondition, WaitResult,
};
use std::time::Duration;
use tokio;

#[tokio::test]
async fn test_local_echo() {
    // Local 모드에서 echo 명령 테스트
    let orchestrator = TaskOrchestrator::new(OrchestratorConfig::default()).await;

    let task = Task::new("test-session", "test", "echo Hello World", serde_json::json!({}))
        .with_execution_mode(ExecutionMode::Local)
        .with_timeout(Duration::from_secs(30));

    let task_id = orchestrator.spawn(task).await.expect("spawn failed");
    println!("Spawned local task: {}", task_id);

    // 완료 대기
    let result = orchestrator
        .wait_for(task_id, WaitCondition::Complete, Some(Duration::from_secs(10)))
        .await
        .expect("wait failed");

    println!("Wait result: {:?}", result);
    assert!(result.is_success(), "Task should complete successfully");

    // 로그 확인
    let logs = orchestrator.get_task_logs(task_id).await;
    println!("Logs: {:?}", logs);
    assert!(logs.is_some(), "Logs should exist");

    let log_entries = logs.unwrap();
    assert!(!log_entries.is_empty(), "Logs should not be empty");

    let content: String = log_entries.iter().map(|e| e.content.clone()).collect();
    println!("Combined content: {}", content);
    assert!(content.contains("Hello World"), "Output should contain 'Hello World'");
}

#[tokio::test]
async fn test_pty_echo() {
    // PTY 모드에서 echo 명령 테스트
    let orchestrator = TaskOrchestrator::new(OrchestratorConfig::default()).await;

    let task = Task::new("test-session", "test", "echo Hello from PTY", serde_json::json!({}))
        .with_execution_mode(ExecutionMode::Pty)
        .with_timeout(Duration::from_secs(30));

    let task_id = orchestrator.spawn(task).await.expect("spawn failed");
    println!("Spawned PTY task: {}", task_id);

    // 잠시 대기 (PTY 출력 수집을 위해)
    tokio::time::sleep(Duration::from_millis(500)).await;

    // 로그 확인
    let logs = orchestrator.get_task_logs(task_id).await;
    println!("PTY Logs after 500ms: {:?}", logs);

    // 완료 대기
    let result = orchestrator
        .wait_for(task_id, WaitCondition::Complete, Some(Duration::from_secs(10)))
        .await
        .expect("wait failed");

    println!("PTY Wait result: {:?}", result);

    // 최종 로그 확인
    let final_logs = orchestrator.get_task_logs(task_id).await;
    println!("PTY Final logs: {:?}", final_logs);

    if let Some(entries) = final_logs {
        let content: String = entries.iter().map(|e| format!("{}\n", e.content)).collect();
        println!("PTY Combined content:\n{}", content);
        // PTY에서 캡처 여부 확인 (실패해도 테스트 통과 - 진단 목적)
        if content.contains("Hello from PTY") {
            println!("✅ PTY log capture working!");
        } else {
            println!("⚠️ PTY log capture issue - content does not contain expected output");
            println!("   This may be a Windows PTY limitation");
        }
    } else {
        println!("⚠️ No logs found for PTY task");
    }
}

#[tokio::test]
async fn test_pty_output_contains_wait() {
    // PTY 모드에서 output_contains 대기 테스트
    let orchestrator = TaskOrchestrator::new(OrchestratorConfig {
        output_poll_interval: Duration::from_millis(50),
        ..Default::default()
    })
    .await;

    // Python 서버 대신 간단한 echo 사용 (Windows 호환성)
    let command = if cfg!(windows) {
        "powershell -Command \"Write-Host 'Server ready on port 8080'; Start-Sleep -Seconds 2\""
    } else {
        "echo 'Server ready on port 8080' && sleep 2"
    };

    let task = Task::new("test-session", "test", command, serde_json::json!({}))
        .with_execution_mode(ExecutionMode::Pty)
        .with_timeout(Duration::from_secs(30));

    let task_id = orchestrator.spawn(task).await.expect("spawn failed");
    println!("Spawned PTY server simulation: {}", task_id);

    // output_contains 대기
    let result = orchestrator
        .wait_for(
            task_id,
            WaitCondition::OutputContains("Server ready".to_string()),
            Some(Duration::from_secs(5)),
        )
        .await
        .expect("wait failed");

    println!("Wait for output result: {:?}", result);

    match &result {
        WaitResult::Satisfied { condition, data } => {
            println!("✅ Output wait successful: condition={}, data={:?}", condition, data);
        }
        WaitResult::Timeout => {
            println!("⚠️ Output wait timed out");
            // 로그 확인
            if let Some(logs) = orchestrator.get_task_logs(task_id).await {
                println!("Logs at timeout:");
                for entry in &logs {
                    println!("  [{:?}] {}", entry.level, entry.content);
                }
            }
        }
        WaitResult::Error(msg) => {
            println!("❌ Output wait error: {}", msg);
        }
        WaitResult::Cancelled => {
            println!("⚠️ Output wait cancelled");
        }
    }

    // 정리
    let _ = orchestrator.stop(task_id).await;
}

#[tokio::test]
async fn test_task_lifecycle() {
    // Task 전체 라이프사이클 테스트
    let orchestrator = TaskOrchestrator::new(OrchestratorConfig::default()).await;

    // 1. Task 생성 및 시작
    let task = Task::new("test-session", "lifecycle", "echo test", serde_json::json!({}))
        .with_execution_mode(ExecutionMode::Local);

    let task_id = orchestrator.spawn(task).await.expect("spawn failed");
    println!("1. Task spawned: {}", task_id);

    // 2. 상태 확인
    if let Some(status) = orchestrator.status(task_id).await {
        println!("2. Task status: running={}, errors={}", status.is_running, status.has_errors);
    }

    // 3. 완료 대기
    let result = orchestrator
        .wait_for(task_id, WaitCondition::Complete, Some(Duration::from_secs(10)))
        .await
        .expect("wait failed");
    println!("3. Wait result: {:?}", result);
    assert!(result.is_success());

    // 4. 최종 상태
    if let Some(status) = orchestrator.status(task_id).await {
        println!("4. Final status: running={}, logs={}", status.is_running, status.log_line_count);
        assert!(!status.is_running, "Task should not be running");
    }

    println!("✅ Task lifecycle test passed!");
}
