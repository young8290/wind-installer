#Requires -Version 5.1
<#
.SYNOPSIS
    打包 WindInput 安装程序
.DESCRIPTION
    使用 wind-packer 将 WindInput 构建产物打包成自解压安装程序
.PARAMETER Version
    版本号，默认从 Cargo.toml 读取
.PARAMETER Compression
    压缩算法: zstd 或 lzma，默认 zstd
.PARAMETER SkipBuild
    跳过 wind-packer 编译，使用已有的二进制
.PARAMETER NoStub
    只生成归档文件，不拼接 Stub (用于测试)
.EXAMPLE
    .\pack-windinput.ps1
    .\pack-windinput.ps1 -Version "0.2.0" -Compression lzma
#>

param(
    [string]$Version = "",
    [ValidateSet("zstd", "lzma")]
    [string]$Compression = "zstd",
    [switch]$SkipBuild,
    [switch]$NoStub
)

$ErrorActionPreference = "Stop"
$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Definition
$ProjectRoot = Split-Path -Parent $ScriptDir

# ============================================================
# 配置
# ============================================================

$PackerConfig = Join-Path $ProjectRoot "pack-windinput.toml"
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

# --- Step 1: 检查环境 ---
Write-Step "检查环境..."

if (-not (Test-Path $PackerConfig)) {
    Write-Err "配置文件不存在: $PackerConfig"
    exit 1
}

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

$RequiredFiles = @(
    "wind_tsf.dll",
    "wind_tsf_x86.dll",
    "wind_input.exe",
    "wind_setting.exe"
)

$MissingFiles = @()
foreach ($f in $RequiredFiles) {
    $path = Join-Path $BuildDir $f
    if (-not (Test-Path $path)) {
        $MissingFiles += $f
    }
}

if ($MissingFiles.Count -gt 0) {
    Write-Err "缺少以下文件:"
    foreach ($f in $MissingFiles) {
        Write-Host "    - $f" -ForegroundColor Red
    }
    Write-Host "`n请先运行 build_all.ps1 构建 WindInput" -ForegroundColor Yellow
    exit 1
}

# 检查可选文件
$OptionalFiles = @("wind_portable.exe")
foreach ($f in $OptionalFiles) {
    $path = Join-Path $BuildDir $f
    if (-not (Test-Path $path)) {
        Write-Warn "可选文件缺失: $f"
    }
}

# 检查数据目录
$DataDir = Join-Path $BuildDir "data"
if (-not (Test-Path $DataDir)) {
    Write-Err "数据目录不存在: $DataDir"
    exit 1
}

Write-OK "构建产物检查通过"

# --- Step 3: 获取版本号 ---
Write-Step "获取版本号..."

if ($Version -eq "") {
    # 从 Cargo.toml 读取
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

Write-OK "版本号: $Version"

# --- Step 4: 编译 wind-packer ---
if (-not $SkipBuild) {
    Write-Step "编译 wind-packer..."

    Push-Location $ProjectRoot
    try {
        $env:CARGO_TERM_COLOR = "always"
        cargo build --release --bin wind-packer 2>&1 | ForEach-Object {
            if ($_ -match "^error") {
                Write-Host $_ -ForegroundColor Red
            }
        }

        if ($LASTEXITCODE -ne 0) {
            Write-Err "编译 wind-packer 失败"
            exit 1
        }

        Write-OK "wind-packer 编译完成"
    }
    finally {
        Pop-Location
    }
}

$PackerExe = Join-Path $ProjectRoot "target\release\wind-packer.exe"
if (-not (Test-Path $PackerExe)) {
    Write-Err "wind-packer 不存在: $PackerExe"
    exit 1
}

# --- Step 5: 创建输出目录 ---
Write-Step "创建输出目录..."

if (-not (Test-Path $OutputDir)) {
    New-Item -ItemType Directory -Path $OutputDir -Force | Out-Null
}

Write-OK "输出目录: $OutputDir"

# --- Step 6: 编译安装器 Stub ---
if (-not $NoStub) {
    Write-Step "编译安装器 Stub..."

    if (-not $SkipBuild) {
        Push-Location $ProjectRoot
        try {
            cargo build --release --bin wind-installer 2>&1 | ForEach-Object {
                if ($_ -match "^error") {
                    Write-Host $_ -ForegroundColor Red
                }
            }

            if ($LASTEXITCODE -ne 0) {
                Write-Err "编译 wind-installer 失败"
                exit 1
            }

            Write-OK "Stub 编译完成"
        }
        finally {
            Pop-Location
        }
    }

    $StubExe = Join-Path $ProjectRoot "target\release\wind-installer.exe"
    if (-not (Test-Path $StubExe)) {
        Write-Err "Stub 不存在: $StubExe"
        exit 1
    }
}

# --- Step 7: 打包 ---
Write-Step "开始打包 (压缩算法: $Compression)..."

$OutputFile = Join-Path $OutputDir "WindInput-${Version}-Setup.exe"
$ArchiveFile = Join-Path $OutputDir "WindInput-${Version}.bin"

# 先生成归档文件
$packerArgs = @(
    "--source", $BuildDir,
    "--output", $ArchiveFile,
    "--compression", $Compression
)

Write-Host "    执行: wind-packer $($packerArgs -join ' ')" -ForegroundColor Gray

& $PackerExe @packerArgs

if ($LASTEXITCODE -ne 0) {
    Write-Err "打包失败"
    exit 1
}

Write-OK "归档文件生成完成"

# --- Step 8: 拼接 Stub + Archive ---
if (-not $NoStub) {
    Write-Step "拼接安装程序..."

    # 读取 Stub
    $stubData = [System.IO.File]::ReadAllBytes($StubExe)

    # 读取归档
    $archiveData = [System.IO.File]::ReadAllBytes($ArchiveFile)

    # 写入最终安装程序
    $outputStream = [System.IO.File]::Create($OutputFile)
    try {
        $outputStream.Write($stubData, 0, $stubData.Length)
        $outputStream.Write($archiveData, 0, $archiveData.Length)
    }
    finally {
        $outputStream.Close()
    }

    Write-OK "安装程序生成完成"

    # 清理临时归档文件
    Remove-Item -Path $ArchiveFile -Force -ErrorAction SilentlyContinue
}
else {
    # 只输出归档文件
    $OutputFile = $ArchiveFile
}

# --- Step 9: 统计信息 ---
Write-Step "打包完成!"

$outputSize = (Get-Item $OutputFile).Length
$outputSizeMB = [math]::Round($outputSize / 1MB, 2)

Write-Host ""
Write-Host "============================================" -ForegroundColor Green
Write-Host "  打包成功!" -ForegroundColor Green
Write-Host "============================================" -ForegroundColor Green
Write-Host ""
Write-Host "  输出文件: $OutputFile"
Write-Host "  文件大小: $outputSizeMB MB"
Write-Host "  版本号:   $Version"
Write-Host "  压缩算法: $Compression"
Write-Host ""

# 打开输出目录
if (Test-Path $OutputFile) {
    $outputDir = Split-Path -Parent $OutputFile
    Write-Host "输出目录: $outputDir" -ForegroundColor Cyan
}
