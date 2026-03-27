param(
    [string]$Root = "$(Split-Path -Parent $PSScriptRoot)",
    [switch]$SkipNodeInstall,
    [switch]$SkipRust,
    [switch]$SkipPython,
    [switch]$SkipFrontend
)

$ErrorActionPreference = "Stop"
$script:Failures = @()

function Step($msg) {
    Write-Host "`n==> $msg" -ForegroundColor Cyan
}

function Invoke-Cmd($cmd, $context) {
    Write-Host "   $cmd" -ForegroundColor DarkGray
    Invoke-Expression $cmd
    if ($LASTEXITCODE -ne 0) {
        $failure = [PSCustomObject]@{
            Context = $context
            Command = $cmd
            ExitCode = $LASTEXITCODE
        }
        $script:Failures += $failure
        Write-Warning "[$context] command exited with code ${LASTEXITCODE}: $cmd"
    }
}

function Test-CommandExists($name) {
    return $null -ne (Get-Command $name -ErrorAction SilentlyContinue)
}

function Invoke-RustChecks($servicePath) {
    if (-not (Test-Path (Join-Path $servicePath "Cargo.toml"))) { return }
    $context = "Rust $servicePath"
    Step "Rust checks: $servicePath"

    $svcTarget = Join-Path $servicePath 'target'
    if (-not (Test-Path $svcTarget)) { New-Item -ItemType Directory -Path $svcTarget | Out-Null }
    $svcDb = Join-Path $svcTarget 'test.db'
    $databaseUrl = $svcDb -replace '\\', '/'
    if ($databaseUrl -match '^[A-Za-z]:') { $databaseUrl = '/' + $databaseUrl }
    $env:TEST_DATABASE_URL = "sqlite:///$databaseUrl"

    Push-Location $servicePath
    try {
        Invoke-Cmd "cargo fmt --all" $context
        Invoke-Cmd "cargo clippy --all-targets --all-features -- -D warnings" $context
        Invoke-Cmd "cargo test -- --test-threads=1" $context
    }
    finally {
        Pop-Location
    }
}

function Invoke-PythonChecks($servicePath) {
    if (-not (Test-Path $servicePath)) { return }
    if (-not (Test-CommandExists "python")) {
        Write-Warning "python not found; skipping Python tests for $servicePath"
        return
    }
    if (-not (Test-CommandExists "pytest")) {
        Write-Warning "pytest not found; skipping Python tests for $servicePath"
        return
    }
    $context = "Python $servicePath"
    Step "Python checks: $servicePath"
    Push-Location $servicePath
    try {
        Invoke-Cmd "pytest" $context
    }
    finally {
        Pop-Location
    }
}

function Invoke-FrontendChecks($servicePath) {
    if (-not (Test-Path $servicePath)) { return }
    if (-not (Test-Path (Join-Path $servicePath "package.json"))) { return }
    if (-not (Test-CommandExists "npm")) {
        Write-Warning "npm not found; skipping frontend checks for $servicePath"
        return
    }

    $context = "Frontend $servicePath"
    Step "Frontend checks: $servicePath"
    Push-Location $servicePath
    try {
        if (-not $SkipNodeInstall) {
            Invoke-Cmd "npm install" $context
        }
        Invoke-Cmd "npm run build" $context
    }
    finally {
        Pop-Location
    }
}

Step "Workspace preflight"

$hasCargo = Test-CommandExists "cargo"
$hasPython = Test-CommandExists "python"
$hasNpm = Test-CommandExists "npm"

if (-not $SkipRust -and -not $hasCargo) {
    Write-Warning "cargo not found in PATH; skipping Rust checks"
    $SkipRust = $true
}

if (-not $SkipPython -and -not $hasPython) {
    Write-Warning "python not found in PATH; skipping Python checks"
    $SkipPython = $true
}

if (-not $SkipFrontend -and -not $hasNpm) {
    Write-Warning "npm not found in PATH; skipping frontend checks"
    $SkipFrontend = $true
}

if (-not $SkipRust) {
    $isWindowsHost = $env:OS -eq "Windows_NT"
    if ($isWindowsHost) {
        $rustcInfo = rustc -vV 2>$null
        $hostLine = ($rustcInfo | Where-Object { $_ -like "host:*" } | Select-Object -First 1)
        $isMsvcTarget = $null -ne $hostLine -and $hostLine -like "*msvc"
        $hasLinkExe = Test-CommandExists "link.exe"
        if ($isMsvcTarget -and -not $hasLinkExe) {
            Write-Warning "Rust host target is MSVC but link.exe is missing; skipping Rust checks. Install Visual Studio Build Tools (Desktop development with C++) or switch to GNU target."
            $SkipRust = $true
        }
    }
}

$rustServices = @(
    "accounts-service",
    "activities-service",
    "automation-service",
    "contacts-service",
    "integrations-service",
    "opportunities-service",
    "reporting-service",
    "search-service",
    "standalones\backend-service"
)

# Pass common environment variables into Rust service tests so SQLite tests do not fail unexpectedly.
if (-not $env:AUTH_JWT_SECRET) { $env:AUTH_JWT_SECRET = 'dev-insecure-secret-change-me' }
if (-not $env:TEST_DATABASE_URL) {
    $workDir = (Get-Location).Path
    $dbDir = Join-Path $workDir 'target'
    if (-not (Test-Path $dbDir)) { New-Item -ItemType Directory -Path $dbDir | Out-Null }
    $dbFile = Join-Path $dbDir 'tests.sqlite'
    $dbFilePath = $dbFile -replace '\\', '/'
    if ($dbFilePath -match '^[A-Za-z]:') { $dbFilePath = '/' + $dbFilePath }
    $env:TEST_DATABASE_URL = "sqlite:///$dbFilePath"
}

if (-not $SkipRust) {
    foreach ($svc in $rustServices) {
        Invoke-RustChecks (Join-Path $Root $svc)
    }
}

if (-not $SkipPython) {
    Invoke-PythonChecks (Join-Path $Root "ai-orchestrator-service")
    Invoke-PythonChecks (Join-Path $Root "auth-service")
}

if (-not $SkipFrontend) {
    Invoke-FrontendChecks (Join-Path $Root "frontend-service")
}

Step "All checks completed"

if ($script:Failures.Count -gt 0) {
    Write-Host "`n==> Failure summary" -ForegroundColor Yellow
    foreach ($failure in $script:Failures) {
        Write-Host " - [$($failure.Context)] exit=$($failure.ExitCode) cmd=$($failure.Command)" -ForegroundColor Yellow
    }
    exit 1
}
