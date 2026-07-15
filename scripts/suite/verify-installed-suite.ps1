[CmdletBinding()]
param(
  [Parameter(Mandatory = $true)][string]$Installer,
  [switch]$RunIsolatedInstall
)

$ErrorActionPreference = "Stop"
$Installer = (Resolve-Path $Installer).Path
$RepoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path
$Lock = Get-Content (Join-Path $RepoRoot "suite-lock.json") -Raw | ConvertFrom-Json
$TempRoot = Join-Path ([System.IO.Path]::GetTempPath()) "cc-switch-mem-verify-$PID"
$ExtractRoot = Join-Path $TempRoot "extracted"
$Sandbox = Join-Path $TempRoot "user"
$WorkerPid = $null

function Expand-NsisInstaller([string]$Source, [string]$Destination) {
  $SevenZip = Get-Command 7z.exe -ErrorAction SilentlyContinue
  if ($SevenZip) {
    & $SevenZip.Source x -y "-o$Destination" $Source | Out-Null
    if ($LASTEXITCODE -ne 0) { throw "7-Zip failed to extract the NSIS installer." }
    return
  }
  $Bandizip = Get-Command bz.exe -ErrorAction SilentlyContinue
  if ($Bandizip) {
    & $Bandizip.Source x -aoa -fmt:exe "-o:$Destination" $Source | Out-Null
    if ($LASTEXITCODE -ne 0) { throw "Bandizip failed to extract the NSIS installer." }
    return
  }
  throw "7z.exe or bz.exe is required to inspect the NSIS installer."
}

try {
  New-Item -ItemType Directory -Force -Path $ExtractRoot, $Sandbox | Out-Null
  $InstallerHash = (Get-FileHash -LiteralPath $Installer -Algorithm SHA256).Hash.ToLowerInvariant()
  Expand-NsisInstaller $Installer $ExtractRoot

  $SuiteRoot = Join-Path $ExtractRoot "resources\suite"
  $Manifest = Get-Content (Join-Path $SuiteRoot "manifest.json") -Raw | ConvertFrom-Json
  if ($Manifest.ccSwitchVersion -ne $Lock.ccSwitch.version) { throw "CC Switch version mismatch." }
  if ($Manifest.claudeMemVersion -ne $Lock.claudeMem.version) { throw "Claude-Mem version mismatch." }
  if ($Manifest.nodeSha256 -ne $Lock.node.sha256) { throw "Node lock hash mismatch." }

  $Node = Join-Path $SuiteRoot "node\node.exe"
  $NodeVersion = & $Node -p "process.version"
  if ($NodeVersion -ne "v$($Lock.node.version)") { throw "Embedded Node version mismatch: $NodeVersion" }
  $ClaudeVersion = & $Node (Join-Path $SuiteRoot "claude-mem\dist\npx-cli\index.js") version
  if ($ClaudeVersion -ne $Lock.claudeMem.version) { throw "Embedded Claude-Mem version mismatch: $ClaudeVersion" }

  if ($RunIsolatedInstall) {
    New-Item -ItemType Directory -Force -Path (Join-Path $Sandbox "AppData\Local"),(Join-Path $Sandbox "AppData\Roaming") | Out-Null
    $env:USERPROFILE = $Sandbox
    $env:HOME = $Sandbox
    $env:LOCALAPPDATA = Join-Path $Sandbox "AppData\Local"
    $env:APPDATA = Join-Path $Sandbox "AppData\Roaming"
    $env:CLAUDE_CONFIG_DIR = Join-Path $Sandbox ".claude"
    $env:CODEX_HOME = Join-Path $Sandbox ".codex"
    & $Node (Join-Path $SuiteRoot "install-suite.mjs") --root $SuiteRoot
    if ($LASTEXITCODE -ne 0) { throw "Isolated suite install failed." }

    $Settings = Get-Content (Join-Path $Sandbox ".claude-mem\settings.json") -Raw | ConvertFrom-Json
    $Provider = $Settings.CLAUDE_MEM_PROVIDER_CONFIG | ConvertFrom-Json
    if ($Provider.providerMode -ne "cc-switch-auto") { throw "Provider mode is not cc-switch-auto." }
    if ($Provider.ccSwitch.modelPolicy -ne "follow-session") { throw "Model policy is not follow-session." }
    if ($Provider.ccSwitch.fixedModel) { throw "A fixed model was unexpectedly configured." }
    $Health = Invoke-RestMethod -Uri "http://127.0.0.1:37777/api/health" -TimeoutSec 5
    if ($Health.status -ne "ok") { throw "Worker health check failed." }

    $Connection = Get-NetTCPConnection -State Listen -LocalPort 37777 -ErrorAction SilentlyContinue | Select-Object -First 1
    if ($Connection) { $WorkerPid = $Connection.OwningProcess }
  }

  Write-Host "Verified suite: CC Switch $($Manifest.ccSwitchVersion), Claude-Mem $($Manifest.claudeMemVersion), Node $NodeVersion"
  Write-Host "Installer SHA256: $InstallerHash"
} finally {
  if ($WorkerPid) {
    $Process = Get-CimInstance Win32_Process -Filter "ProcessId=$WorkerPid" -ErrorAction SilentlyContinue
    if ($Process -and $Process.CommandLine -like "*$Sandbox*") {
      Stop-Process -Id $WorkerPid -Force -ErrorAction SilentlyContinue
    }
  }
  Remove-Item -LiteralPath $TempRoot -Recurse -Force -ErrorAction SilentlyContinue
}
