[CmdletBinding()]
param(
  [Parameter(Mandatory = $true)][string]$ClaudeMemSource,
  [string]$OutputRoot = ""
)

$ErrorActionPreference = "Stop"
$RepoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path
$ClaudeRoot = (Resolve-Path $ClaudeMemSource).Path
$OutputRoot = if ($OutputRoot) { $OutputRoot } else { Join-Path $RepoRoot "src-tauri\resources\suite" }
$OutputRoot = [System.IO.Path]::GetFullPath($OutputRoot)
$Lock = Get-Content (Join-Path $RepoRoot "suite-lock.json") -Raw | ConvertFrom-Json

& node (Join-Path $PSScriptRoot "validate-lock.mjs")
if ($LASTEXITCODE -ne 0) { throw "Suite lock validation failed." }

$ClaudeVersion = (Get-Content (Join-Path $ClaudeRoot "package.json") -Raw | ConvertFrom-Json).version
if ($ClaudeVersion -ne $Lock.claudeMem.version) { throw "Claude-Mem version mismatch: $ClaudeVersion" }

$TempRoot = Join-Path ([System.IO.Path]::GetTempPath()) "cc-switch-mem-suite-$PID"
try {
  New-Item -ItemType Directory -Force -Path $TempRoot | Out-Null
  Push-Location $ClaudeRoot
  try {
    & npm run build
    if ($LASTEXITCODE -ne 0) { throw "Claude-Mem build failed." }
    & node "scripts/release/build-local-assets.mjs" (Join-Path $TempRoot "claude-assets")
    if ($LASTEXITCODE -ne 0) { throw "Claude-Mem asset build failed." }
  } finally { Pop-Location }

  if (Test-Path -LiteralPath $OutputRoot) { Remove-Item -LiteralPath $OutputRoot -Recurse -Force }
  $ClaudeOutput = Join-Path $OutputRoot "claude-mem"
  $NodeOutput = Join-Path $OutputRoot "node"
  New-Item -ItemType Directory -Force -Path $ClaudeOutput, $NodeOutput | Out-Null
  Expand-Archive -LiteralPath (Join-Path $TempRoot "claude-assets\claude-mem-local-$ClaudeVersion.zip") -DestinationPath $ClaudeOutput -Force

  $NodeZip = Join-Path $TempRoot "node.zip"
  Invoke-WebRequest -UseBasicParsing -Uri $Lock.node.url -OutFile $NodeZip
  $ActualHash = (Get-FileHash -LiteralPath $NodeZip -Algorithm SHA256).Hash.ToLowerInvariant()
  if ($ActualHash -ne $Lock.node.sha256) { throw "Node runtime SHA-256 mismatch." }
  $ExpandedNode = Join-Path $TempRoot "node-expanded"
  Expand-Archive -LiteralPath $NodeZip -DestinationPath $ExpandedNode -Force
  $NodeDirectory = Get-ChildItem -LiteralPath $ExpandedNode -Directory | Select-Object -First 1
  Copy-Item -Path (Join-Path $NodeDirectory.FullName "*") -Destination $NodeOutput -Recurse -Force

  Copy-Item -LiteralPath (Join-Path $PSScriptRoot "install-suite.mjs") -Destination (Join-Path $OutputRoot "install-suite.mjs") -Force
  $ManifestJson = [ordered]@{
    schemaVersion = 1
    ccSwitchVersion = $Lock.ccSwitch.version
    claudeMemVersion = $Lock.claudeMem.version
    nodeVersion = $Lock.node.version
    nodeSha256 = $Lock.node.sha256
  } | ConvertTo-Json
  [System.IO.File]::WriteAllText(
    (Join-Path $OutputRoot "manifest.json"),
    $ManifestJson,
    [System.Text.UTF8Encoding]::new($false)
  )
  Write-Host "Prepared pinned suite resources at $OutputRoot"
} finally {
  Remove-Item -LiteralPath $TempRoot -Recurse -Force -ErrorAction SilentlyContinue
}
