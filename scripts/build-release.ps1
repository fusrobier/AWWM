#Requires -Version 5.1
<#
.SYNOPSIS
    One-command clean release build of AWWM: checks, tests, installers, verification.

.DESCRIPTION
    Runs from the repository that contains this script and stops at the first failure:

       1. Prerequisites: cargo + rustc >= 1.90 with the MSVC toolchain, node >= 22.12, npm.
       2. npm ci                                        (clean, lockfile-exact node_modules)
       3. cargo fmt --all -- --check
       4. cargo clippy --workspace --all-targets -- -D warnings
       5. cargo test --workspace -- --test-threads=1   (see -SkipTests)
       6. npm test
       7. npm run typecheck
       8. npm run tauri -- build                        (NSIS setup + MSI; stale bundles are deleted first)
       9. scripts\verify-release.ps1                    (signatures, versions, imports, release\<version>\ + SHA256SUMS.txt)

    The Rust integration tests (crates\*\tests\*.rs) create and move real windows on the
    desktop, so do not use the computer while they run, or pass -SkipTests.

    Network access: npm ci downloads the locked npm packages; the first `tauri build` on a
    machine downloads NSIS / WiX (hash-checked by the Tauri CLI) and Microsoft's WebView2
    bootstrapper, whose Microsoft signature verify-release.ps1 then checks.

    Compatible with Windows PowerShell 5.1 and PowerShell 7.

.PARAMETER SkipTests
    Skip only the Rust integration tests. Rust unit tests and doc tests, the frontend tests
    and every other step still run.

.PARAMETER Clean
    Run `cargo clean` first for a full rebuild from scratch.

.EXAMPLE
    pwsh -File scripts\build-release.ps1

.EXAMPLE
    powershell -ExecutionPolicy Bypass -File scripts\build-release.ps1 -SkipTests
#>
[CmdletBinding()]
param(
    [switch]$SkipTests,
    [switch]$Clean
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$MinRust = [version]'1.90.0'
$MinNode = [version]'22.12.0'
$RepoRoot = Split-Path -Parent $PSScriptRoot

function Write-Step([string]$Text) { Write-Host ''; Write-Host "==> $Text" -ForegroundColor Cyan }

# Runs a native command, echoing it first; throws when it exits with a non-zero code.
# $ErrorActionPreference is relaxed locally because Windows PowerShell 5.1 turns native
# stderr output (cargo progress lines) into errors when output is redirected.
function Invoke-Native {
    param(
        [Parameter(Mandatory)][string]$Description,
        [Parameter(Mandatory)][string]$Command,
        [string[]]$Arguments = @()
    )
    Write-Step $Description
    Write-Host "    > $Command $($Arguments -join ' ')" -ForegroundColor DarkGray
    $ErrorActionPreference = 'Continue'
    & $Command @Arguments
    $exitCode = $LASTEXITCODE
    $ErrorActionPreference = 'Stop'
    if ($exitCode -ne 0) { throw "$Description failed (exit code $exitCode)." }
}

# Resolves an executable on PATH, or throws with installation guidance.
function Resolve-Tool([string[]]$Names, [string]$Help) {
    foreach ($name in $Names) {
        $command = Get-Command $name -CommandType Application -ErrorAction SilentlyContinue | Select-Object -First 1
        if ($null -ne $command) { return $command.Source }
    }
    throw "$($Names[0]) was not found on PATH. $Help"
}

# Runs a tool and returns its trimmed output, or throws.
function Get-ToolOutput([string]$Command, [string[]]$Arguments) {
    $ErrorActionPreference = 'Continue'
    $output = & $Command @Arguments 2>&1 | Out-String
    $exitCode = $LASTEXITCODE
    $ErrorActionPreference = 'Stop'
    if ($exitCode -ne 0) { throw "'$Command $($Arguments -join ' ')' failed (exit code $exitCode): $output" }
    return $output.Trim()
}

# '1.92.0-nightly' / 'v22.12.0' -> [version]
function ConvertTo-Version([string]$Text) {
    if ($Text -match '(\d+)\.(\d+)\.(\d+)') { return [version]('{0}.{1}.{2}' -f $Matches[1], $Matches[2], $Matches[3]) }
    throw "Cannot read a version number from '$Text'."
}

$started = Get-Date
Push-Location -LiteralPath $RepoRoot
try {
    # --- 1. Prerequisites -------------------------------------------------------
    Write-Step 'Checking prerequisites'
    $rustHelp = 'Install Rust from https://rustup.rs and select the MSVC toolchain: rustup default stable-x86_64-pc-windows-msvc'
    $cargo = Resolve-Tool @('cargo') $rustHelp
    $rustc = Resolve-Tool @('rustc') $rustHelp
    $rustInfo = Get-ToolOutput $rustc @('-vV')
    $rustVersionLine = if ($rustInfo -match '(?m)^release:\s*(\S+)') { $Matches[1] } else { throw "Unexpected 'rustc -vV' output: $rustInfo" }
    $rustHost = if ($rustInfo -match '(?m)^host:\s*(\S+)') { $Matches[1] } else { throw "Unexpected 'rustc -vV' output: $rustInfo" }
    $rustVersion = ConvertTo-Version $rustVersionLine
    if ($rustVersion -lt $MinRust) { throw "rustc $rustVersionLine is too old; AWWM needs Rust $MinRust or newer. Run: rustup update stable" }
    if ($rustHost -notmatch '-pc-windows-msvc$') { throw "rustc uses the '$rustHost' toolchain; AWWM must be built with MSVC. Run: rustup default stable-x86_64-pc-windows-msvc (requires the Visual Studio C++ Build Tools)." }
    $cargoVersion = Get-ToolOutput $cargo @('--version')
    Write-Host "    rustc $rustVersionLine ($rustHost), $cargoVersion"

    $nodeHelp = "Install Node.js $MinNode or newer from https://nodejs.org"
    $node = Resolve-Tool @('node') $nodeHelp
    $nodeVersionText = Get-ToolOutput $node @('--version')
    if ((ConvertTo-Version $nodeVersionText) -lt $MinNode) { throw "Node.js $nodeVersionText is too old; AWWM needs $MinNode or newer. $nodeHelp" }
    # npm.cmd is preferred over npm.ps1 so the exit code is always reported reliably.
    $npm = Resolve-Tool @('npm.cmd', 'npm') 'npm is installed together with Node.js.'
    $npmVersion = Get-ToolOutput $npm @('--version')
    Write-Host "    node $nodeVersionText, npm $npmVersion"

    $targetDir = if ($env:CARGO_TARGET_DIR) { $env:CARGO_TARGET_DIR } else { Join-Path $RepoRoot 'target' }
    if (-not [System.IO.Path]::IsPathRooted($targetDir)) { $targetDir = Join-Path $RepoRoot $targetDir }

    if ($Clean) {
        Invoke-Native 'cargo clean' $cargo @('clean')
    }
    # Never verify or publish installers left over from an earlier build.
    $staleBundles = Join-Path $targetDir 'release\bundle'
    if (Test-Path -LiteralPath $staleBundles) {
        Write-Step "Removing previous installers ($staleBundles)"
        Remove-Item -LiteralPath $staleBundles -Recurse -Force
    }

    # --- 2.-7. Dependencies, formatting, lints, tests ---------------------------
    Invoke-Native 'Installing npm dependencies (npm ci)' $npm @('ci')
    Invoke-Native 'Checking Rust formatting' $cargo @('fmt', '--all', '--', '--check')
    Invoke-Native 'Running clippy' $cargo @('clippy', '--workspace', '--all-targets', '--', '-D', 'warnings')
    if ($SkipTests) {
        Write-Host ''
        Write-Host '    -SkipTests: Rust integration tests (tests\*.rs) are skipped.' -ForegroundColor Yellow
        Invoke-Native 'Running Rust unit tests' $cargo @('test', '--workspace', '--lib', '--bins', '--', '--test-threads=1')
        Invoke-Native 'Running Rust doc tests' $cargo @('test', '--workspace', '--doc', '--', '--test-threads=1')
    } else {
        Invoke-Native 'Running all Rust tests (integration tests open real windows)' $cargo @('test', '--workspace', '--', '--test-threads=1')
    }
    Invoke-Native 'Running frontend tests' $npm @('test')
    Invoke-Native 'Type-checking the frontend' $npm @('run', 'typecheck')

    # --- 8. Installers ----------------------------------------------------------
    Invoke-Native 'Building the release executable and installers (tauri build)' $npm @('run', 'tauri', '--', 'build')

    # --- 9. Verification and staging --------------------------------------------
    Write-Step 'Verifying the release'
    & (Join-Path $PSScriptRoot 'verify-release.ps1') -RepoRoot $RepoRoot -TargetDir $targetDir
    if ($LASTEXITCODE -ne 0) { throw "Release verification failed (exit code $LASTEXITCODE)." }

    $elapsed = (Get-Date) - $started
    Write-Host ''
    Write-Host ('Release build finished in {0:hh\:mm\:ss}. Artifacts: {1}' -f $elapsed, (Join-Path $RepoRoot 'release')) -ForegroundColor Green
    exit 0
} catch {
    Write-Host ''
    Write-Host "RELEASE BUILD FAILED: $($_.Exception.Message)" -ForegroundColor Red
    exit 1
} finally {
    Pop-Location
}
