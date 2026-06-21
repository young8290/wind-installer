#Requires -Version 5.1
<#
.SYNOPSIS
    通用安装程序打包脚本（由 app.toml 驱动）
.DESCRIPTION
    编译 stub 二进制 → 注入卸载器到源目录 → 调用 wind-packer build 生成安装程序。
    应用身份、源目录、压缩、品牌（logo/icon）全部由 app.toml 描述，与本脚本无关。
.PARAMETER Config
    app.toml 路径，默认项目根目录下的 app.toml
.PARAMETER SkipBuild
    跳过 cargo 编译，使用已有 release 二进制
.EXAMPLE
    .\scripts\pack.ps1
    .\scripts\pack.ps1 -Config app.toml -SkipBuild
#>

param(
    [string]$Config = "",
    [switch]$SkipBuild
)

$ErrorActionPreference = "Stop"
$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Definition
$ProjectRoot = Split-Path -Parent $ScriptDir
if ($Config -eq "") { $Config = Join-Path $ProjectRoot "app.toml" }

function Write-Step { param([string]$m) Write-Host "`n>>> $m" -ForegroundColor Cyan }
function Write-OK   { param([string]$m) Write-Host "    [OK] $m" -ForegroundColor Green }
function Write-Err  { param([string]$m) Write-Host "    [ERROR] $m" -ForegroundColor Red }

Write-Host "============================================" -ForegroundColor Cyan
Write-Host "  Wind Installer 打包（app.toml 驱动）" -ForegroundColor Cyan
Write-Host "============================================" -ForegroundColor Cyan

if (-not (Test-Path $Config)) { Write-Err "配置不存在: $Config"; exit 1 }

# --- 从 app.toml 解析 source_dir（用于注入卸载器；相对 app.toml 所在目录）---
$ConfigDir = Split-Path -Parent (Resolve-Path $Config)
$tomlText = Get-Content $Config -Raw
if ($tomlText -notmatch 'source_dir\s*=\s*"([^"]+)"') {
    Write-Err "app.toml 缺少 [package] source_dir"; exit 1
}
$SourceDir = $Matches[1]
if (-not [System.IO.Path]::IsPathRooted($SourceDir)) {
    $SourceDir = Join-Path $ConfigDir $SourceDir
}
if (-not (Test-Path $SourceDir)) { Write-Err "源目录不存在: $SourceDir"; exit 1 }
Write-OK "源目录: $SourceDir"

# --- Step 1: 编译 stub 二进制 ---
$StubExe        = Join-Path $ProjectRoot "target\release\wind-installer.exe"
$PackerExe      = Join-Path $ProjectRoot "target\release\wind-packer.exe"
$UninstallerExe = Join-Path $ProjectRoot "target\release\wind-uninstaller.exe"

if (-not $SkipBuild) {
    Write-Step "编译 wind-installer / wind-uninstaller / wind-packer..."
    Push-Location $ProjectRoot
    try {
        cargo build --release --bin wind-installer --bin wind-uninstaller
        if ($LASTEXITCODE -ne 0) { Write-Err "stub 编译失败"; exit 1 }
        # wind-packer 需要 packer feature（启用 editpe 写图标）
        cargo build --release --bin wind-packer --features packer
        if ($LASTEXITCODE -ne 0) { Write-Err "packer 编译失败"; exit 1 }
        Write-OK "编译完成"
    }
    finally { Pop-Location }
}

foreach ($exe in @($StubExe, $PackerExe, $UninstallerExe)) {
    if (-not (Test-Path $exe)) { Write-Err "缺少二进制: $exe（去掉 -SkipBuild 重新编译）"; exit 1 }
}

# --- Step 2: 注入卸载器到源目录（打包时一并压缩，安装后解压到安装目录）---
$UninstallerDest = Join-Path $SourceDir "uninstall.exe"
Copy-Item -Path $UninstallerExe -Destination $UninstallerDest -Force
Write-OK "已注入卸载器: $UninstallerDest"

try {
    # --- Step 3: pack + bundle（含写图标）---
    Write-Step "打包（wind-packer build）..."
    & $PackerExe build --config $Config --stub $StubExe
    if ($LASTEXITCODE -ne 0) { Write-Err "打包失败"; exit 1 }
}
finally {
    # 保持源目录干净
    Remove-Item -Path $UninstallerDest -Force -ErrorAction SilentlyContinue
}

Write-Host "`n============================================" -ForegroundColor Green
Write-Host "  打包完成!" -ForegroundColor Green
Write-Host "============================================" -ForegroundColor Green
