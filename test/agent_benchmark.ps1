# ForgeCode Agent Benchmark Script
# 2026-02-05

$ForgePath = "..\target\release\forge.exe"
$Provider = "ollama"
$Model = "qwen3:8b"

Write-Host "🧪 ForgeCode Agent Benchmark" -ForegroundColor Cyan
Write-Host "=============================" -ForegroundColor Cyan
Write-Host ""

# Test 1: Simple Response
Write-Host "📝 Test 1: Simple Response" -ForegroundColor Yellow
$start = Get-Date
$result = & $ForgePath --provider $Provider --model $Model --prompt "Say 'Hello ForgeCode' in one line" 2>&1
$duration = (Get-Date) - $start
Write-Host "  Duration: $($duration.TotalSeconds.ToString('F2'))s" -ForegroundColor Green
Write-Host ""

# Test 2: Tool Use - Read File
Write-Host "📝 Test 2: Tool Use - Read File" -ForegroundColor Yellow
$start = Get-Date
$result = & $ForgePath --provider $Provider --model $Model --prompt "Read the Cargo.toml file and tell me the version" 2>&1
$duration = (Get-Date) - $start
Write-Host "  Duration: $($duration.TotalSeconds.ToString('F2'))s" -ForegroundColor Green
Write-Host ""

# Test 3: Tool Use - Bash Command
Write-Host "📝 Test 3: Tool Use - Bash Command" -ForegroundColor Yellow
$start = Get-Date
$result = & $ForgePath --provider $Provider --model $Model --prompt "Run 'cargo --version' and show me the output" 2>&1
$duration = (Get-Date) - $start
Write-Host "  Duration: $($duration.TotalSeconds.ToString('F2'))s" -ForegroundColor Green
Write-Host ""

# Test 4: Multiple Tool Calls (Parallel potential)
Write-Host "📝 Test 4: Multiple Tool Calls" -ForegroundColor Yellow
$start = Get-Date
$result = & $ForgePath --provider $Provider --model $Model --prompt "Read both Cargo.toml and README.md, then summarize them together" 2>&1
$duration = (Get-Date) - $start
Write-Host "  Duration: $($duration.TotalSeconds.ToString('F2'))s" -ForegroundColor Green
Write-Host ""

# Test 5: Code Analysis
Write-Host "📝 Test 5: Code Analysis" -ForegroundColor Yellow
$start = Get-Date
$result = & $ForgePath --provider $Provider --model $Model --prompt "Find all Rust files in crates/Layer4-cli/src that contain 'pub fn' and count them" 2>&1
$duration = (Get-Date) - $start
Write-Host "  Duration: $($duration.TotalSeconds.ToString('F2'))s" -ForegroundColor Green
Write-Host ""

Write-Host "✅ Benchmark Complete!" -ForegroundColor Cyan
