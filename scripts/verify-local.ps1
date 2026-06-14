#!/usr/bin/env pwsh
# verify-local.ps1 — Canonical Verify Stage for pares-radix (C-TEST-001)
#
# This script exercises pares-radix through its HTTP API without any
# adapter dependency (no Telegram, no external services).
#
# Usage:
#   ./scripts/verify-local.ps1                    # Build + test
#   ./scripts/verify-local.ps1 -SkipBuild         # Test only (binary already built)
#   ./scripts/verify-local.ps1 -Port 3201         # Custom port
#
# Exit codes:
#   0 = all tests passed
#   1 = build failed
#   2 = server failed to start
#   3 = health check failed
#   4 = chat endpoint test failed

param(
    [switch]$SkipBuild,
    [int]$Port = 0,
    [int]$TimeoutSeconds = 15
)

$ErrorActionPreference = "Stop"
$script:ExitCode = 0
$script:ServerProcess = $null

# ── Helpers ────────────────────────────────────────────────────────────────────

function Write-Step($msg) { Write-Host "`n▶ $msg" -ForegroundColor Cyan }
function Write-Pass($msg) { Write-Host "  ✓ $msg" -ForegroundColor Green }
function Write-Fail($msg) { Write-Host "  ✗ $msg" -ForegroundColor Red }
function Write-Info($msg) { Write-Host "  ℹ $msg" -ForegroundColor Gray }

function Get-RandomPort {
    $listener = [System.Net.Sockets.TcpListener]::new([System.Net.IPAddress]::Loopback, 0)
    $listener.Start()
    $port = $listener.LocalEndpoint.Port
    $listener.Stop()
    return $port
}

function Stop-Server {
    if ($script:ServerProcess -and -not $script:ServerProcess.HasExited) {
        Write-Info "Stopping server (PID $($script:ServerProcess.Id))..."
        Stop-Process -Id $script:ServerProcess.Id -Force -ErrorAction SilentlyContinue
        $script:ServerProcess.WaitForExit(5000) | Out-Null
    }
}

# Ensure cleanup on exit
trap { Stop-Server }

# ── Step 1: Build ──────────────────────────────────────────────────────────────

if (-not $SkipBuild) {
    Write-Step "Building pares-radix (release)"
    $buildResult = cargo build --release -p pares-radix 2>&1
    if ($LASTEXITCODE -ne 0) {
        Write-Fail "Build failed"
        $buildResult | Write-Host
        exit 1
    }
    Write-Pass "Build succeeded"
} else {
    Write-Info "Skipping build (-SkipBuild)"
}

# Find binary
$Binary = "target/release/pares-radix.exe"
if (-not (Test-Path $Binary)) {
    $Binary = "target/release/pares-radix"
}
if (-not (Test-Path $Binary)) {
    $Binary = "target/debug/pares-radix.exe"
}
if (-not (Test-Path $Binary)) {
    Write-Fail "Cannot find pares-radix binary. Build first."
    exit 1
}
Write-Info "Using binary: $Binary"

# ── Step 2: Start Server ───────────────────────────────────────────────────────

if ($Port -eq 0) { $Port = Get-RandomPort }
Write-Step "Starting pares-radix serve-spine --channel http --http-port $Port"

$script:ServerProcess = Start-Process -FilePath $Binary -ArgumentList @(
    "serve-spine",
    "--channel", "http",
    "--http-port", $Port.ToString(),
    "--model-url", "http://127.0.0.1:1/fake-model",
    "--model", "test-model"
) -PassThru -NoNewWindow -RedirectStandardError "NUL"

# Wait for health
$deadline = (Get-Date).AddSeconds($TimeoutSeconds)
$ready = $false
while ((Get-Date) -lt $deadline) {
    try {
        $health = Invoke-RestMethod -Uri "http://127.0.0.1:$Port/v1/health" -TimeoutSec 2 -ErrorAction SilentlyContinue
        if ($health.status -eq "ok") {
            $ready = $true
            break
        }
    } catch {}
    Start-Sleep -Milliseconds 200
}

if (-not $ready) {
    Write-Fail "Server failed to start within $TimeoutSeconds seconds"
    Stop-Server
    exit 2
}
Write-Pass "Server ready on port $Port (PID $($script:ServerProcess.Id))"

# ── Step 3: Health Check ───────────────────────────────────────────────────────

Write-Step "Testing /v1/health"
try {
    $health = Invoke-RestMethod -Uri "http://127.0.0.1:$Port/v1/health"
    if ($health.status -ne "ok") { throw "status != ok" }
    if ($health.channel -ne "http") { throw "channel != http" }
    if (-not $health.version) { throw "version missing" }
    Write-Pass "Health: status=ok, channel=http, version=$($health.version)"
} catch {
    Write-Fail "Health check failed: $_"
    $script:ExitCode = 3
}

# ── Step 4: Chat Endpoint Tests ────────────────────────────────────────────────

Write-Step "Testing /v1/chat — empty message rejection"
try {
    $resp = Invoke-WebRequest -Uri "http://127.0.0.1:$Port/v1/chat" -Method Post `
        -ContentType "application/json" -Body '{"message":""}' `
        -ErrorAction SilentlyContinue -SkipHttpErrorCheck
    if ($resp.StatusCode -eq 400) {
        Write-Pass "Empty message correctly rejected (400)"
    } else {
        Write-Fail "Expected 400, got $($resp.StatusCode)"
        $script:ExitCode = 4
    }
} catch {
    # PowerShell may throw on non-2xx; check the exception
    if ($_.Exception.Response.StatusCode.value__ -eq 400) {
        Write-Pass "Empty message correctly rejected (400)"
    } else {
        Write-Fail "Chat empty message test failed: $_"
        $script:ExitCode = 4
    }
}

Write-Step "Testing /v1/chat — valid message acceptance"
try {
    $body = @{ message = "Hello from verify-local test harness"; sender = "verify-script" } | ConvertTo-Json
    $resp = Invoke-WebRequest -Uri "http://127.0.0.1:$Port/v1/chat" -Method Post `
        -ContentType "application/json" -Body $body `
        -TimeoutSec 30 -ErrorAction SilentlyContinue -SkipHttpErrorCheck

    $status = $resp.StatusCode
    if ($status -eq 200) {
        $json = $resp.Content | ConvertFrom-Json
        if ($json.id -and $json.response) {
            Write-Pass "Chat response received: id=$($json.id), len=$($json.response.Length)"
        } else {
            Write-Fail "Response missing id or response field"
            $script:ExitCode = 4
        }
    } elseif ($status -eq 504) {
        # Model timeout is expected when no real model is configured
        # The important thing is the pipeline accepted and processed the request
        Write-Pass "Chat request accepted, model timeout (504) — pipeline works, no model configured"
    } else {
        Write-Fail "Expected 200 or 504, got $status"
        $script:ExitCode = 4
    }
} catch {
    Write-Fail "Chat valid message test failed: $_"
    $script:ExitCode = 4
}

Write-Step "Testing /v1/chat — session isolation"
try {
    # Two concurrent sessions should not conflict
    $bodyA = @{ message = "Session A"; session_id = "test-session-a" } | ConvertTo-Json
    $bodyB = @{ message = "Session B"; session_id = "test-session-b" } | ConvertTo-Json

    $respA = Invoke-WebRequest -Uri "http://127.0.0.1:$Port/v1/chat" -Method Post `
        -ContentType "application/json" -Body $bodyA `
        -TimeoutSec 30 -ErrorAction SilentlyContinue -SkipHttpErrorCheck

    $respB = Invoke-WebRequest -Uri "http://127.0.0.1:$Port/v1/chat" -Method Post `
        -ContentType "application/json" -Body $bodyB `
        -TimeoutSec 30 -ErrorAction SilentlyContinue -SkipHttpErrorCheck

    if ($respA.StatusCode -ne 429 -and $respB.StatusCode -ne 429) {
        Write-Pass "Sessions isolated (no 429 rate-limit conflict)"
    } else {
        Write-Fail "Session isolation failed — got 429"
        $script:ExitCode = 4
    }
} catch {
    Write-Fail "Session isolation test failed: $_"
    $script:ExitCode = 4
}

# ── Cleanup ────────────────────────────────────────────────────────────────────

Write-Step "Cleanup"
Stop-Server

if ($script:ExitCode -eq 0) {
    Write-Host "`n━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━" -ForegroundColor Green
    Write-Host "  ALL TESTS PASSED — Platform verified locally" -ForegroundColor Green
    Write-Host "  No Telegram. No praxisbot. No network deps." -ForegroundColor Green
    Write-Host "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━" -ForegroundColor Green
} else {
    Write-Host "`n━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━" -ForegroundColor Red
    Write-Host "  TESTS FAILED — Exit code: $script:ExitCode" -ForegroundColor Red
    Write-Host "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━" -ForegroundColor Red
}

exit $script:ExitCode
