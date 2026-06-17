#Requires -Version 5.1
<#
.SYNOPSIS
    打包 WindInput 安装程序
.DESCRIPTION
    使用 wind-packer 将 WindInput 构建产物打包成自解压安装程序。
    支持分阶段打包：
      pack  - 压缩源文件为 .bin（慢，仅需一次）
      bundle - 拼接 stub + .bin 为 .exe（快，可反复测试）
.PARAMETER Version
    版本号，默认从 Cargo.toml 读取
.PARAMETER Compression
    压缩算法: zstd 或 lzma，默认 zstd
.PARAMETER SkipBuild
    跳过编译，使用已有的二进制
.PARAMETER PackOnly
    只生成 .bin 归档，不拼接 Stub（用于分阶段打包）
.PARAMETER BundleOnly
    只执行拼接，跳过压缩（需要已有 .bin 文件）
.PARAMETER ArchivePath
    指定已有的 .bin 归档文件路径（与 -BundleOnly 配合使用）
.EXAMPLE
    # 完整打包
    .\pack-windinput.ps1

    # 第一阶段：压缩（慢）
    .\pack-windinput.ps1 -PackOnly

    # 第二阶段：拼接（快）
    .\pack-windinput.ps1 -BundleOnly -ArchivePath dist\WindInput-0.1.0.bin

    # 使用 LZMA 压缩
    .\pack-windinput.ps1 -Compression lzma
#>

param(
    [string]$Version = "",
    [ValidateSet("zstd", "lzma")]
    [string]$Compression = "lzma",
    [switch]$SkipBuild,
    [switch]$PackOnly,
    [switch]$BundleOnly,
    [string]$ArchivePath = ""
)

$ErrorActionPreference = "Stop"
$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Definition
$ProjectRoot = Split-Path -Parent $ScriptDir

# ============================================================
# 配置
# ============================================================

$WindInputRoot = Join-Path $ProjectRoot "..\WindInput"
$BuildDir = Join-Path $WindInputRoot "build"
$OutputDir = Join-Path $ProjectRoot "dist"

# ============================================================
# 辅助函数
# ============================================================

function Write-Step {
    param([string]$Message)
    Write-Host "`n>>> $Message" -ForegroundColor Cyan
}

function Write-OK {
    param([string]$Message)
    Write-Host "    [OK] $Message" -ForegroundColor Green
}

function Write-Warn {
    param([string]$Message)
    Write-Host "    [WARN] $Message" -ForegroundColor Yellow
}

function Write-Err {
    param([string]$Message)
    Write-Host "    [ERROR] $Message" -ForegroundColor Red
}

# ============================================================
# 主流程
# ============================================================

Write-Host "============================================" -ForegroundColor Cyan
Write-Host "  WindInput 安装程序打包工具" -ForegroundColor Cyan
Write-Host "============================================" -ForegroundColor Cyan

# --- 获取版本号 ---
if ($Version -eq "") {
    $CargoToml = Join-Path $ProjectRoot "Cargo.toml"
    if (Test-Path $CargoToml) {
        $content = Get-Content $CargoToml -Raw
        if ($content -match 'version\s*=\s*"([^"]+)"') {
            $Version = $Matches[1]
        }
    }
    if ($Version -eq "") {
        $Version = "0.1.0"
    }
}

$PackerExe = Join-Path $ProjectRoot "target\release\wind-packer.exe"
$StubExe = Join-Path $ProjectRoot "target\release\wind-installer.exe"
$ArchiveFile = if ($ArchivePath -ne "") { $ArchivePath } else { Join-Path $OutputDir "WindInput-${Version}.bin" }
$OutputFile = Join-Path $OutputDir "WindInput-${Version}-Setup.exe"

# ============================================================
# BundleOnly: 只拼接（快）
# ============================================================
if ($BundleOnly) {
    Write-Step "快速拼接模式 (bundle)"

    if (-not (Test-Path $StubExe)) {
        Write-Err "Stub 不存在: $StubExe"
        Write-Host "请先编译: cargo build --release --bin wind-installer" -ForegroundColor Yellow
        exit 1
    }
    if (-not (Test-Path $ArchiveFile)) {
        Write-Err "归档文件不存在: $ArchiveFile"
        Write-Host "请先运行: .\pack-windinput.ps1 -PackOnly" -ForegroundColor Yellow
        exit 1
    }

    & $PackerExe bundle --stub $StubExe --archive $ArchiveFile --output $OutputFile
    if ($LASTEXITCODE -ne 0) {
        Write-Err "拼接失败"
        exit 1
    }

    Write-OK "安装程序已生成: $OutputFile"
    $outputSize = (Get-Item $OutputFile).Length
    Write-Host "  文件大小: $([math]::Round($outputSize / 1MB, 2)) MB" -ForegroundColor Cyan
    exit 0
}

# ============================================================
# 完整流程 / PackOnly
# ============================================================

# --- Step 1: 检查环境 ---
Write-Step "检查环境..."

if (-not (Test-Path $WindInputRoot)) {
    Write-Err "WindInput 项目目录不存在: $WindInputRoot"
    exit 1
}

if (-not (Test-Path $BuildDir)) {
    Write-Err "构建目录不存在: $BuildDir"
    Write-Host "请先运行: cd ..\WindInput && .\build_all.ps1" -ForegroundColor Yellow
    exit 1
}

Write-OK "环境检查通过"

# --- Step 2: 检查构建产物 ---
Write-Step "检查构建产物..."

$RequiredFiles = @("wind_tsf.dll", "wind_tsf_x86.dll", "wind_input.exe", "wind_setting.exe")
$MissingFiles = @()
foreach ($f in $RequiredFiles) {
    if (-not (Test-Path (Join-Path $BuildDir $f))) {
        $MissingFiles += $f
    }
}

if ($MissingFiles.Count -gt 0) {
    Write-Err "缺少以下文件:"
    foreach ($f in $MissingFiles) { Write-Host "    - $f" -ForegroundColor Red }
    Write-Host "`n请先运行 build_all.ps1 构建 WindInput" -ForegroundColor Yellow
    exit 1
}

$DataDir = Join-Path $BuildDir "data"
if (-not (Test-Path $DataDir)) {
    Write-Err "数据目录不存在: $DataDir"
    exit 1
}

Write-OK "构建产物检查通过"
Write-OK "版本号: $Version"

# --- Step 3: 编译工具 ---
if (-not $SkipBuild) {
    Write-Step "编译 wind-packer 和 wind-installer..."

    Push-Location $ProjectRoot
    try {
        $env:CARGO_TERM_COLOR = "always"
        cargo build --release --bin wind-packer --bin wind-installer --bin wind-uninstaller 2>&1 | ForEach-Object {
            if ($_ -match "^error") { Write-Host $_ -ForegroundColor Red }
        }
        if ($LASTEXITCODE -ne 0) {
            Write-Err "编译失败"
            exit 1
        }
        Write-OK "编译完成"
    }
    finally {
        Pop-Location
    }
}

if (-not (Test-Path $PackerExe)) {
    Write-Err "wind-packer 不存在: $PackerExe"
    exit 1
}

# --- Step 4: 创建输出目录 ---
if (-not (Test-Path $OutputDir)) {
    New-Item -ItemType Directory -Path $OutputDir -Force | Out-Null
}

# --- Step 5: 将 wind-uninstaller.exe 注入构建目录 ---
$UninstallerExe = Join-Path $ProjectRoot "target\release\wind-uninstaller.exe"
$UninstallerDest = Join-Path $BuildDir "uninstall.exe"
$UninstallerInjected = $false

if (-not (Test-Path $UninstallerExe)) {
    Write-Warn "wind-uninstaller.exe 不存在，归档中将不含卸载程序"
    Write-Warn "路径: $UninstallerExe"
} else {
    Copy-Item -Path $UninstallerExe -Destination $UninstallerDest -Force
    $UninstallerInjected = $true
    Write-OK "已将卸载程序注入构建目录 ($([math]::Round((Get-Item $UninstallerDest).Length / 1KB)) KB)"
}

# --- Step 6: 压缩打包 ---
Write-Step "阶段一：压缩打包 (算法: $Compression)..."

& $PackerExe pack --source $BuildDir --output $ArchiveFile --compression $Compression
if ($LASTEXITCODE -ne 0) {
    # 打包失败时清理注入的文件
    if ($UninstallerInjected) { Remove-Item -Path $UninstallerDest -Force -ErrorAction SilentlyContinue }
    Write-Err "压缩打包失败"
    exit 1
}

# 打包完成后从构建目录移除，保持构建目录干净
if ($UninstallerInjected) {
    Remove-Item -Path $UninstallerDest -Force -ErrorAction SilentlyContinue
}

Write-OK "归档文件: $ArchiveFile"

# --- Step 7: 拼接安装程序 ---
if (-not $PackOnly) {
    Write-Step "阶段二：拼接安装程序..."

    & $PackerExe bundle --stub $StubExe --archive $ArchiveFile --output $OutputFile
    if ($LASTEXITCODE -ne 0) {
        Write-Err "拼接失败"
        exit 1
    }

    Write-OK "安装程序: $OutputFile"

    # 清理中间归档
    Remove-Item -Path $ArchiveFile -Force -ErrorAction SilentlyContinue
}
else {
    Write-Host ""
    Write-Host "  归档文件已生成: $ArchiveFile" -ForegroundColor Yellow
    Write-Host "  使用以下命令快速拼接:" -ForegroundColor Yellow
    Write-Host "    .\pack-windinput.ps1 -BundleOnly" -ForegroundColor Cyan
    Write-Host ""
}

# --- 完成 ---
Write-Host ""
Write-Host "============================================" -ForegroundColor Green
Write-Host "  打包完成!" -ForegroundColor Green
Write-Host "============================================" -ForegroundColor Green
Write-Host ""

$finalFile = if ($PackOnly) { $ArchiveFile } else { $OutputFile }
if (Test-Path $finalFile) {
    $size = (Get-Item $finalFile).Length
    Write-Host "  文件: $finalFile"
    Write-Host "  大小: $([math]::Round($size / 1MB, 2)) MB"
    Write-Host "  版本: $Version"
    Write-Host "  压缩: $Compression"
    Write-Host ""
}
