#!/usr/bin/env pwsh
<#
.SYNOPSIS
    Quick test script for Tachyon-Tex API using PowerShell

.DESCRIPTION
    Tests all endpoints without requiring Node.js

.EXAMPLE
    .\tests\quick-test.ps1
    .\tests\quick-test.ps1 -BaseUrl "https://latex.taptapp.xyz"
#>

param(
    [string]$BaseUrl = "http://localhost:8080"
)

$ErrorActionPreference = "Continue"

function Write-Test {
    param([string]$Status, [string]$Name, [string]$Details = "")
    $icon = switch ($Status) {
        "pass" { "✓" }
        "fail" { "✗" }
        default { "⏳" }
    }
    $color = switch ($Status) {
        "pass" { "Green" }
        "fail" { "Red" }
        default { "Yellow" }
    }
    Write-Host "  $icon " -ForegroundColor $color -NoNewline
    Write-Host $Name -NoNewline
    if ($Details) { Write-Host " $Details" -ForegroundColor Cyan } else { Write-Host "" }
}

$passed = 0
$failed = 0

Write-Host "`n🧪 Tachyon-Tex Quick Tests" -ForegroundColor White
Write-Host "   Target: $BaseUrl`n" -ForegroundColor Cyan

# ==============================================================================
# Health Check
# ==============================================================================
Write-Host "📡 Health Check" -ForegroundColor White

try {
    $res = Invoke-WebRequest -Uri "$BaseUrl/" -Method Get -UseBasicParsing
    if ($res.StatusCode -eq 200) {
        Write-Test "pass" "GET / returns 200"
        $passed++
    } else {
        Write-Test "fail" "GET / returns 200" "- Got $($res.StatusCode)"
        $failed++
    }
} catch {
    Write-Test "fail" "GET / returns 200" "- $($_.Exception.Message)"
    $failed++
}

# ==============================================================================
# GET /packages
# ==============================================================================
Write-Host "`n📦 GET /packages" -ForegroundColor White

try {
    $res = Invoke-RestMethod -Uri "$BaseUrl/packages" -Method Get
    if ($res.count -gt 0 -and $res.packages) {
        Write-Test "pass" "Returns package list" "($($res.count) packages)"
        $passed++
        
        $names = $res.packages | ForEach-Object { $_.name }
        if ($names -contains "amsmath" -and $names -contains "tikz") {
            Write-Test "pass" "Contains common packages"
            $passed++
        } else {
            Write-Test "fail" "Contains common packages"
            $failed++
        }
    } else {
        Write-Test "fail" "Returns package list"
        $failed++
    }
} catch {
    Write-Test "fail" "GET /packages" "- $($_.Exception.Message)"
    $failed++
}

# ==============================================================================
# POST /validate
# ==============================================================================
Write-Host "`n✅ POST /validate" -ForegroundColor White

$validTex = @"
\documentclass{article}
\begin{document}
Hello World!
\end{document}
"@

$invalidTex = @"
\documentclass{article}
\begin{document}
Hello World!
"@

# Test valid document
try {
    $boundary = [System.Guid]::NewGuid().ToString()
    $body = @"
--$boundary
Content-Disposition: form-data; name="file"; filename="test.tex"
Content-Type: text/plain

$validTex
--$boundary--
"@
    $res = Invoke-RestMethod -Uri "$BaseUrl/validate" -Method Post -ContentType "multipart/form-data; boundary=$boundary" -Body $body
    if ($res.valid -eq $true) {
        Write-Test "pass" "Valid LaTeX returns valid: true"
        $passed++
    } else {
        Write-Test "fail" "Valid LaTeX returns valid: true" "- Got $($res.valid)"
        $failed++
    }
} catch {
    Write-Test "fail" "Valid LaTeX returns valid: true" "- $($_.Exception.Message)"
    $failed++
}

# Test invalid document
try {
    $boundary = [System.Guid]::NewGuid().ToString()
    $body = @"
--$boundary
Content-Disposition: form-data; name="file"; filename="test.tex"
Content-Type: text/plain

$invalidTex
--$boundary--
"@
    $res = Invoke-RestMethod -Uri "$BaseUrl/validate" -Method Post -ContentType "multipart/form-data; boundary=$boundary" -Body $body
    if ($res.valid -eq $false -and $res.errors.Count -gt 0) {
        Write-Test "pass" "Invalid LaTeX returns errors"
        $passed++
    } else {
        Write-Test "fail" "Invalid LaTeX returns errors"
        $failed++
    }
} catch {
    Write-Test "fail" "Invalid LaTeX returns errors" "- $($_.Exception.Message)"
    $failed++
}

# ==============================================================================
# POST /compile
# ==============================================================================
Write-Host "`n📄 POST /compile" -ForegroundColor White

try {
    $boundary = [System.Guid]::NewGuid().ToString()
    $body = @"
--$boundary
Content-Disposition: form-data; name="file"; filename="test.tex"
Content-Type: text/plain

$validTex
--$boundary--
"@
    $start = Get-Date
    $res = Invoke-WebRequest -Uri "$BaseUrl/compile" -Method Post -ContentType "multipart/form-data; boundary=$boundary" -Body $body -UseBasicParsing
    $elapsed = ((Get-Date) - $start).TotalMilliseconds
    
    if ($res.StatusCode -eq 200 -and $res.Headers["Content-Type"] -eq "application/pdf") {
        Write-Test "pass" "Compiles to PDF" "(${elapsed}ms)"
        $passed++
        
        $compileTime = $res.Headers["X-Compile-Time-Ms"]
        if ($compileTime) {
            Write-Test "pass" "Returns compile time header" "(${compileTime}ms engine time)"
            $passed++
        }
    } else {
        Write-Test "fail" "Compiles to PDF" "- Status: $($res.StatusCode)"
        $failed++
    }
} catch {
    Write-Test "fail" "Compiles to PDF" "- $($_.Exception.Message)"
    $failed++
}

# ==============================================================================
# Summary
# ==============================================================================
Write-Host "`n━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━" -ForegroundColor White
Write-Host "Results: " -NoNewline -ForegroundColor White
Write-Host "$passed passed" -NoNewline -ForegroundColor Green
Write-Host ", " -NoNewline
if ($failed -gt 0) {
    Write-Host "$failed failed" -ForegroundColor Red
} else {
    Write-Host "$failed failed" -ForegroundColor Green
}
Write-Host "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━`n" -ForegroundColor White

exit $(if ($failed -gt 0) { 1 } else { 0 })
