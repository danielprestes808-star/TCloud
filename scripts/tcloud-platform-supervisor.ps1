param([string]$Root = "D:\TCloud")
$ErrorActionPreference = "Continue"
$ProgressPreference = "SilentlyContinue"

$mutex = New-Object System.Threading.Mutex($false, "Global\TCloudPlatformSupervisorV7")
$acquired = $false
try { $acquired = $mutex.WaitOne(0, $false) } catch {}
if (-not $acquired) { exit 0 }

$Root = [IO.Path]::GetFullPath($Root)
$logDir = Join-Path $Root "logs"
$logPath = Join-Path $logDir "platform-supervisor.log"
New-Item -ItemType Directory -Path $logDir -Force | Out-Null

function Log([string]$Text) {
    $line = "{0} {1}" -f (Get-Date -Format "yyyy-MM-dd HH:mm:ss.fff"), $Text
    try { Add-Content -Path $logPath -Value $line -Encoding UTF8 } catch {}
}

function Test-Port([int]$Port) {
    try {
        $client = New-Object Net.Sockets.TcpClient
        $iar = $client.BeginConnect("127.0.0.1", $Port, $null, $null)
        if (-not $iar.AsyncWaitHandle.WaitOne(500)) { $client.Close(); return $false }
        $client.EndConnect($iar); $client.Close(); return $true
    } catch { return $false }
}

function Start-HiddenProcess([string]$FilePath,[string[]]$Arguments,[string]$WorkingDirectory) {
    try {
        $psi = New-Object Diagnostics.ProcessStartInfo
        $psi.FileName = $FilePath
        $psi.WorkingDirectory = $WorkingDirectory
        $psi.UseShellExecute = $false
        $psi.CreateNoWindow = $true
        $psi.WindowStyle = [Diagnostics.ProcessWindowStyle]::Hidden
        $psi.Arguments = ($Arguments | ForEach-Object { if ($_ -match '[\s"]') { '"' + ($_ -replace '"','\"') + '"' } else { $_ } }) -join ' '
        [Diagnostics.Process]::Start($psi) | Out-Null
        return $true
    } catch { Log "spawn failed: $FilePath :: $($_.Exception.Message)"; return $false }
}

function Ensure-Docker {
    $now = Get-Date
    if ($script:LastDockerFailure -and (($now - $script:LastDockerFailure).TotalMinutes -lt 10)) {
        return $false
    }
    try { & docker info *> $null; if ($LASTEXITCODE -eq 0) { return $true } } catch {}
    $dockerCommand = Get-Command docker -ErrorAction SilentlyContinue
    $desktopFound = $false
    foreach ($exe in @("$env:ProgramFiles\Docker\Docker\Docker Desktop.exe","$env:LOCALAPPDATA\Docker\Docker Desktop.exe")) {
        if (Test-Path $exe -PathType Leaf) {
            $desktopFound = $true
            Log "starting Docker Desktop"
            Start-Process -FilePath $exe -WindowStyle Hidden | Out-Null
            break
        }
    }
    if (-not $dockerCommand -and -not $desktopFound) {
        $script:LastDockerFailure = $now
        Log "Docker unavailable; retry postponed for 10 minutes"
        return $false
    }
    for ($i=0; $i -lt 20; $i++) {
        Start-Sleep -Seconds 2
        try { & docker info *> $null; if ($LASTEXITCODE -eq 0) { Log "Docker online"; return $true } } catch {}
    }
    $script:LastDockerFailure = Get-Date
    Log "Docker unavailable; Core will use fallback mode"; return $false
}

function Find-ComposeFile {
    foreach ($file in @(
        (Join-Path $Root "docker-compose.yml"),(Join-Path $Root "docker-compose.yaml"),(Join-Path $Root "compose.yml"),(Join-Path $Root "compose.yaml"),
        (Join-Path $Root "core\docker-compose.yml"),(Join-Path $Root "core\compose.yml")
    )) {
        if (Test-Path $file -PathType Leaf) { try { if ((Get-Content $file -Raw) -match 'postgres') { return $file } } catch {} }
    }
    $found = Get-ChildItem $Root -Recurse -File -ErrorAction SilentlyContinue |
        Where-Object { $_.Name -in @('docker-compose.yml','docker-compose.yaml','compose.yml','compose.yaml') -and $_.FullName -notmatch '\\node_modules\\|\\target\\|\\\.next\\|\\backups?\\' } |
        Select-Object -First 12
    foreach ($item in $found) { try { if ((Get-Content $item.FullName -Raw) -match 'postgres') { return $item.FullName } } catch {} }
    return $null
}

function Ensure-Postgres {
    if (Test-Port 54329) { return $true }
    if (-not (Ensure-Docker)) { return $false }
    $compose = Find-ComposeFile
    if (-not $compose) { Log "compose with postgres not found"; return $false }
    Log "docker compose up: $compose"
    try { & docker compose -f $compose up -d *> $null } catch { Log "docker compose failed: $($_.Exception.Message)" }
    for ($i=0; $i -lt 45; $i++) { if (Test-Port 54329) { Log "PostgreSQL online :54329"; return $true }; Start-Sleep -Seconds 1 }
    Log "PostgreSQL unavailable :54329"; return $false
}

function Find-CoreExe {
    foreach ($file in @((Join-Path $Root "core\target\release\tcloud-core.exe"),(Join-Path $Root "core\target\debug\tcloud-core.exe"))) {
        if (Test-Path $file -PathType Leaf) { return $file }
    }
    return $null
}

function Use-NativePostgresIfAvailable {
    if (-not (Test-Port 5432)) { return $false }
    $envFile = Join-Path $Root 'core\.env'
    if (-not (Test-Path $envFile -PathType Leaf)) { return $false }
    $line = Get-Content $envFile -ErrorAction SilentlyContinue |
        Where-Object { $_ -match '^TCLOUD_DATABASE_URL=' } |
        Select-Object -First 1
    if ([string]::IsNullOrWhiteSpace($line)) { return $false }
    $url = $line.Substring($line.IndexOf('=') + 1).Trim()
    if ([string]::IsNullOrWhiteSpace($url)) { return $false }
    $nativeUrl = $url -replace '(@(?:127\.0\.0\.1|localhost)):54329/', '$1:5432/'
    if ($nativeUrl -eq $url -and $url -notmatch '@(?:127\.0\.0\.1|localhost):5432/') {
        return $false
    }
    $env:TCLOUD_DATABASE_URL = $nativeUrl
    Log "native PostgreSQL selected :5432"
    return $true
}

function Ensure-Core {
    if (Test-Port 8787) { return $true }
    $postgresOk = Ensure-Postgres
    if (-not $postgresOk) {
        $postgresOk = Use-NativePostgresIfAvailable
    }
    if (-not $postgresOk) { Log "starting Core without PostgreSQL (fallback mode)" }
    $coreExe = Find-CoreExe
    if ($coreExe) { Log "starting Core: $coreExe"; Start-HiddenProcess $coreExe @() (Join-Path $Root "core") | Out-Null }
    else {
        $cargoCmd = Get-Command cargo -ErrorAction SilentlyContinue
        if ($cargoCmd) { Log "starting Core through cargo run"; Start-HiddenProcess $cargoCmd.Source @('run','--manifest-path',(Join-Path $Root 'core\Cargo.toml')) (Join-Path $Root "core") | Out-Null }
    }
    for ($i=0; $i -lt 40; $i++) { if (Test-Port 8787) { Log "Core online :8787"; return $true }; Start-Sleep -Milliseconds 500 }
    Log "Core unavailable :8787"; return $false
}

function Ensure-Web {
    if (Test-Port 3000) { return $true }
    $webRoot = Join-Path $Root "web"
    if (-not (Test-Path (Join-Path $webRoot "package.json") -PathType Leaf)) { return $false }
    $npmCmd = Get-Command npm.cmd -ErrorAction SilentlyContinue
    if (-not $npmCmd) { $npmCmd = Get-Command npm -ErrorAction SilentlyContinue }
    if (-not $npmCmd) { Log "npm not found for Web"; return $false }
    if (-not (Test-Path (Join-Path $webRoot ".next") -PathType Container)) { Log "Web .next missing; build required"; return $false }
    Log "starting Web :3000"
    Start-HiddenProcess $npmCmd.Source @('run','start','--','-p','3000') $webRoot | Out-Null
    for ($i=0; $i -lt 30; $i++) { if (Test-Port 3000) { Log "Web online :3000"; return $true }; Start-Sleep -Milliseconds 500 }
    Log "Web unavailable :3000"; return $false
}

Log "platform supervisor started root=$Root pid=$PID"
$tick = 0
while ($true) {
    try {
        $coreOk = Ensure-Core
        $webOk = Ensure-Web
        if (($tick % 6) -eq 0) {
            $postgresOk = (Test-Port 54329) -or (Test-Port 5432)
            Log "health postgres=$postgresOk core=$coreOk web=$webOk"
        }
    } catch { Log "supervisor iteration failed: $($_.Exception.Message)" }
    $tick++
    Start-Sleep -Seconds 10
}
