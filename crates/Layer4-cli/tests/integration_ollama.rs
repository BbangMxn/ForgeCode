//! Ollama Integration Test
//!
//! 실제 Ollama 서버와의 통합 테스트
//! 실행: cargo test -p forge-cli --test integration_ollama -- --ignored --nocapture

use std::process::Command;
use std::time::{Duration, Instant};

/// 시나리오 정의 (간단한 버전)
#[derive(Debug, Clone)]
pub struct QuickScenario {
    pub id: &'static str,
    pub prompt: &'static str,
    pub expected_keywords: Vec<&'static str>,
}

fn get_quick_scenarios() -> Vec<QuickScenario> {
    vec![
        QuickScenario {
            id: "basic-greeting",
            prompt: "Say hello in one word",
            expected_keywords: vec!["hello", "hi", "hey", "greetings"],
        },
        QuickScenario {
            id: "code-gen-python",
            prompt: "Write a Python function to add two numbers",
            expected_keywords: vec!["def", "return", "+"],
        },
        QuickScenario {
            id: "explain-git",
            prompt: "What does git status do?",
            expected_keywords: vec!["git", "status", "changes", "tracked", "files"],
        },
        QuickScenario {
            id: "list-files",
            prompt: "List files in current directory",
            expected_keywords: vec!["file", "directory", "ls", "cargo"],
        },
        QuickScenario {
            id: "rust-hello",
            prompt: "Write Rust hello world",
            expected_keywords: vec!["fn", "main", "println"],
        },
    ]
}

/// 환경 확인
fn check_ollama_available() -> bool {
    Command::new("ollama")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// ForgeCode CLI로 단일 프롬프트 실행
fn run_forge_prompt(prompt: &str) -> Result<String, String> {
    let output = Command::new("cargo")
        .args([
            "run", "-p", "forge-cli", "--release", "--",
            "--provider", "ollama",
            "--model", "qwen3:8b",
            "--no-init",
            "-p", prompt,
        ])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .map_err(|e| format!("Failed to execute: {}", e))?;

    if !output.status.success() {
        return Err(format!(
            "Exit code: {:?}\nstderr: {}",
            output.status.code(),
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

#[test]
#[ignore] // 실제 Ollama 필요 - 수동 실행
fn test_ollama_single() {
    if !check_ollama_available() {
        println!("⚠ Ollama not available, skipping");
        return;
    }

    println!("\n=== Single Ollama Test ===");

    match run_forge_prompt("Say hello") {
        Ok(resp) => {
            println!("✓ Response received:");
            println!("{}", &resp[..resp.len().min(500)]);
        }
        Err(e) => {
            println!("✗ Error: {}", e);
        }
    }
}

#[test]
#[ignore] // 실제 Ollama 필요 - 수동 실행
fn test_quick_5_scenarios() {
    if !check_ollama_available() {
        println!("⚠ Ollama not available, skipping");
        return;
    }

    let scenarios = get_quick_scenarios();
    println!("\n=== Quick Test ({} scenarios) ===\n", scenarios.len());

    let mut passed = 0;
    let mut failed = 0;

    for scenario in &scenarios {
        print!("[{}] {} ... ", scenario.id, scenario.prompt);

        let start = Instant::now();
        match run_forge_prompt(scenario.prompt) {
            Ok(response) => {
                let response_lower = response.to_lowercase();
                let has_keyword = scenario.expected_keywords.iter()
                    .any(|kw| response_lower.contains(*kw));

                if has_keyword {
                    println!("✓ PASS ({:.1}s)", start.elapsed().as_secs_f32());
                    passed += 1;
                } else {
                    println!("✗ FAIL - keywords not found");
                    println!("  Expected one of: {:?}", scenario.expected_keywords);
                    println!("  Response preview: {}", &response[..response.len().min(200)]);
                    failed += 1;
                }
            }
            Err(e) => {
                println!("✗ ERROR: {}", e);
                failed += 1;
            }
        }

        // Rate limit 방지
        std::thread::sleep(Duration::from_millis(500));
    }

    println!("\n=== Results ===");
    println!("Passed: {}/{}", passed, scenarios.len());
    println!("Failed: {}/{}", failed, scenarios.len());

    assert!(passed >= 3, "At least 3/5 scenarios should pass");
}

#[test]
#[ignore] // 실제 Ollama 필요 - 30개 전체
fn test_all_30_scenarios() {
    if !check_ollama_available() {
        println!("⚠ Ollama not available, skipping");
        return;
    }

    // 전체 30개 시나리오 (simulation_30.rs에서 가져옴)
    let scenarios = vec![
        ("conv-01", "Hello, introduce yourself briefly", vec!["hello", "assist"]),
        ("conv-02", "What can you help me with?", vec!["code", "help"]),
        ("conv-03", "Say hello in one word", vec!["hello", "hi"]),
        ("conv-04", "What programming languages do you know?", vec!["python", "rust"]),
        ("conv-05", "Explain what a function is in one sentence", vec!["function", "code"]),
        ("gen-01", "Write a Python function to check if a number is prime", vec!["def", "prime"]),
        ("gen-02", "Write a Rust function to reverse a string", vec!["fn", "string"]),
        ("gen-03", "Write a JavaScript function to filter even numbers", vec!["function", "filter"]),
        ("gen-04", "Write a Python class for a Stack", vec!["class", "push"]),
        ("gen-05", "Write a TypeScript interface for User", vec!["interface", "user"]),
        ("explain-01", "Explain Rust fn main println", vec!["main", "print"]),
        ("explain-02", "What does git status do?", vec!["git", "status"]),
        ("explain-03", "Explain async await briefly", vec!["async", "await"]),
        ("explain-04", "Difference between let and const in JS?", vec!["let", "const"]),
        ("explain-05", "What does cargo build do?", vec!["cargo", "build"]),
        ("debug-01", "How to fix null pointer exception?", vec!["null", "check"]),
        ("debug-02", "Why cargo build fail with missing deps?", vec!["cargo", "dependency"]),
        ("debug-03", "How to debug Python import error?", vec!["import", "path"]),
        ("debug-04", "What causes index out of bounds?", vec!["index", "array"]),
        ("debug-05", "How to fix permission denied?", vec!["permission", "chmod"]),
        ("arch-01", "What is MVC pattern?", vec!["model", "view"]),
        ("arch-02", "When use microservices vs monolith?", vec!["microservice", "scale"]),
        ("arch-03", "What is dependency injection?", vec!["dependency", "inject"]),
        ("arch-04", "Explain repository pattern", vec!["repository", "data"]),
        ("arch-05", "What is REST API?", vec!["rest", "api"]),
        ("tool-01", "List files in current directory", vec!["file", "directory"]),
        ("tool-02", "What is current working directory?", vec!["directory", "path"]),
        ("tool-03", "Search for main in Rust files", vec!["search", "main"]),
        ("tool-04", "Show first 5 lines of Cargo.toml", vec!["cargo", "toml"]),
        ("tool-05", "Check if git is installed", vec!["git", "version"]),
    ];

    println!("\n=== Running {} Scenarios ===\n", scenarios.len());

    let mut passed = 0;
    let mut failed = 0;
    let mut failures = Vec::new();

    for (i, (id, prompt, keywords)) in scenarios.iter().enumerate() {
        print!("[{}/{}] {} ... ", i + 1, scenarios.len(), id);

        let start = Instant::now();
        match run_forge_prompt(prompt) {
            Ok(response) => {
                let response_lower = response.to_lowercase();
                let has_keyword = keywords.iter().any(|kw| response_lower.contains(*kw));

                if has_keyword {
                    println!("✓ PASS ({:.1}s)", start.elapsed().as_secs_f32());
                    passed += 1;
                } else {
                    println!("✗ FAIL");
                    failures.push((*id, "Keywords not found".to_string()));
                    failed += 1;
                }
            }
            Err(e) => {
                println!("✗ ERROR");
                failures.push((*id, e));
                failed += 1;
            }
        }

        std::thread::sleep(Duration::from_millis(500));
    }

    println!("\n=== Summary ===");
    println!("Passed: {} ({:.1}%)", passed, (passed as f64 / scenarios.len() as f64) * 100.0);
    println!("Failed: {} ({:.1}%)", failed, (failed as f64 / scenarios.len() as f64) * 100.0);

    if !failures.is_empty() {
        println!("\n=== Failures ===");
        for (id, reason) in &failures {
            println!("- {}: {}", id, reason);
        }
    }

    let success_rate = passed as f64 / scenarios.len() as f64;
    assert!(success_rate >= 0.7, "Success rate {:.1}% below 70%", success_rate * 100.0);
}
