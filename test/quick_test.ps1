# ForgeCode Quick Test Script
# Usage: .\test\quick_test.ps1

$ErrorActionPreference = "Continue"
$ForgePath = ".\target\release\forge.exe"

Write-Host "`n🔧 ForgeCode Quick Test" -ForegroundColor Cyan
Write-Host "========================`n" -ForegroundColor Cyan

# Check if binary exists
if (-not (Test-Path $ForgePath)) {
    Write-Host "❌ forge.exe not found. Run: cargo build --release" -ForegroundColor Red
    exit 1
}

# Test 1: Version
Write-Host "1️⃣ Version Check..." -ForegroundColor Yellow
& $ForgePath --version
Write-Host ""

# Test 2: Help
Write-Host "2️⃣ Help Check..." -ForegroundColor Yellow
& $ForgePath --help | Select-Object -First 10
Write-Host ""

# Test 3: Simple prompt (if Ollama is running)
Write-Host "3️⃣ Simple Prompt Test..." -ForegroundColor Yellow
try {
    $result = Invoke-RestMethod -Uri "http://localhost:11434/api/tags" -TimeoutSec 2 -ErrorAction Stop
    Write-Host "   Ollama detected! Running test..." -ForegroundColor Green
    & $ForgePath --provider ollama --model "qwen3:8b" --prompt "Say 'ForgeCode works!' in exactly 3 words"
} catch {
    Write-Host "   Ollama not running. Skipping prompt test." -ForegroundColor Gray
}
Write-Host ""

# Test 4: File operations
Write-Host "4️⃣ File Operations Test..." -ForegroundColor Yellow
$testFile = "test/quick_test_output.txt"

# Create test file
"Test content from ForgeCode" | Out-File -FilePath $testFile -Encoding utf8
if (Test-Path $testFile) {
    Write-Host "   ✅ File created: $testFile" -ForegroundColor Green
    Get-Content $testFile
    Remove-Item $testFile -Force
    Write-Host "   ✅ File deleted" -ForegroundColor Green
} else {
    Write-Host "   ❌ File creation failed" -ForegroundColor Red
}
Write-Host ""

# Summary
Write-Host "✅ Quick Test Complete!" -ForegroundColor Cyan
Write-Host "`nFor full TUI test, run: $ForgePath" -ForegroundColor Gray
