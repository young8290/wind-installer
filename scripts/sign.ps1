# Moodflow 代码签名 (Windows / Authenticode)
#
# 可单独运行, 也被 dev.ps1 的 8/9 出包流程自动调用。
#
#   .\scripts\sign.ps1 -Status               会话与配置体检 (不签任何文件)
#   .\scripts\sign.ps1 build\moodflow_tsf.dll    签指定文件
#   .\scripts\sign.ps1 -Verify dist\*.exe    只验签
#
# Supports certificates exposed to the Windows CurrentUser/My or LocalMachine/My store
# by a hardware token, cloud provider or other Windows cryptographic provider.
# Private keys and provider credentials are never accepted as script arguments.
# A trusted publisher identity requires a purchased code-signing certificate.
# See docs/design/code-signing.md for configuration and the immutable signing order.

param(
    # 待签文件 (支持通配)。留空则按 -Status / -Verify 的语义处理。
    [Parameter(Position = 0, ValueFromRemainingArguments)] [string[]]$Path = @(),
    # 只验签不签名
    [switch]$Verify,
    # 已经有有效签名的文件也重签 (默认跳过, 便于反复打包时省时间)
    [switch]$Force,
    # 签名不可用时报错退出, 而不是警告后跳过。正式发版走这个。
    [switch]$RequireSigned,
    # 只报告配置与会话状态
    [switch]$Status
)

$ErrorActionPreference = "Stop"

$ScriptDir   = $PSScriptRoot
$ProductRoot = Split-Path $ScriptDir -Parent

function Say  ([string]$m) { Write-Host $m -ForegroundColor Green }
function Warn ([string]$m) { Write-Host $m -ForegroundColor Yellow }
function ErrMsg ([string]$m) { Write-Host $m -ForegroundColor Red }
function Gray ([string]$m) { Write-Host $m -ForegroundColor DarkGray }

# ---------- 配置 ----------
# 证书指纹。留空 = 签名整体关闭 (本机日常开发的默认状态)。
$MOODFLOW_SIGN_THUMBPRINT = $env:MOODFLOW_SIGN_THUMBPRINT
$MOODFLOW_SIGN_STORE = if ($env:MOODFLOW_SIGN_STORE) { $env:MOODFLOW_SIGN_STORE } else { 'CurrentUser' }

# RFC3161 timestamp; the provider can override this public endpoint.
$MOODFLOW_SIGN_TIMESTAMP_URL = "http://timestamp.digicert.com"

# 单个文件的签名尝试上限 (含首次)。覆盖两类瞬时故障: 时间戳服务器抽风、以及打包器
# 刚写完文件时的占用 —— 判据与退避见 Invoke-SignOne。
# 重试而非直接放弃, 但【绝不】降级成「不打时间戳也算成功」。
$MOODFLOW_SIGN_RETRY = 3

# 会话不可用时打印的排障提示。默认是中性文案; 在 sign.local.ps1 里填上你那家服务商的
# 具体步骤 (客户端叫什么、会话怎么开), 排障时能省一次翻文档 —— 而仓库里保持中性。
$MOODFLOW_SIGN_SESSION_HINT = "打开签名服务的本地客户端并登录, 建立签名会话后重试"

# 可在 scripts\sign.local.ps1 覆盖以上变量 (该文件 gitignore; 模板见 .example)。
$signCfg = "$ScriptDir\sign.local.ps1"
if (Test-Path $signCfg) { . $signCfg }
if ($MOODFLOW_SIGN_STORE -notin @('CurrentUser', 'LocalMachine')) {
    throw 'MOODFLOW_SIGN_STORE must be CurrentUser or LocalMachine'
}
$MOODFLOW_SIGN_THUMBPRINT = ($MOODFLOW_SIGN_THUMBPRINT -replace '[\s\u200e\u200f]', '').ToUpperInvariant()
if ($MOODFLOW_SIGN_THUMBPRINT -and $MOODFLOW_SIGN_THUMBPRINT -notmatch '^[A-F0-9]{40}$') {
    throw 'MOODFLOW_SIGN_THUMBPRINT must be the 40-character certificate fingerprint'
}

# ---------- signtool 定位 ----------
# Windows SDK 按版本号建目录, 取版本号最大的那个 x64 版。
function Resolve-SignTool {
    if ($env:MOODFLOW_SIGNTOOL -and (Test-Path $env:MOODFLOW_SIGNTOOL)) { return $env:MOODFLOW_SIGNTOOL }
    $roots = @(
        "${env:ProgramFiles(x86)}\Windows Kits\10\bin",
        "$env:ProgramFiles\Windows Kits\10\bin"
    ) | Where-Object { Test-Path $_ }

    $found = foreach ($r in $roots) {
        Get-ChildItem $r -Directory -ErrorAction SilentlyContinue |
            Where-Object { $_.Name -match '^10\.' } |
            ForEach-Object {
                $p = Join-Path $_.FullName "x64\signtool.exe"
                if (Test-Path $p) { [pscustomobject]@{ Ver = [version]$_.Name; Path = $p } }
            }
    }
    # SDK 目录名是四段版本号, 字符串排序会把 10.0.9xxxx 排在 10.0.26100 前面, 必须按
    # [version] 比较。
    ($found | Sort-Object Ver -Descending | Select-Object -First 1).Path
}

# ---------- 证书 / 会话 ----------
# 返回可用于签名的证书对象; 会话未开启或未配置则返回 $null。
#
# 判据是【私钥可用】而不仅仅是「证书在存储里」: 签名会话结束后, 证书条目可能还残留在
# Cert:\CurrentUser\My, 但 HasPrivateKey 为假 —— 此时 signtool 会在真正签名那一刻才
# 失败, 而那时安装包已经打好了。提前拦在这里。
function Test-SigningCertificate ($cert, [datetime]$now = (Get-Date)) {
    if (-not $cert -or -not $cert.HasPrivateKey) { return $false }
    if ($cert.NotBefore -gt $now -or $cert.NotAfter -le $now) { return $false }
    return (@($cert.EnhancedKeyUsageList | ForEach-Object { [string]$_.ObjectId }) -contains '1.3.6.1.5.5.7.3.3')
}

function Resolve-SigningCert {
    if (-not $MOODFLOW_SIGN_THUMBPRINT) { return $null }
    $cert = Get-ChildItem "Cert:\$MOODFLOW_SIGN_STORE\My" -ErrorAction SilentlyContinue |
        Where-Object { $_.Thumbprint -eq $MOODFLOW_SIGN_THUMBPRINT } | Select-Object -First 1
    if (-not (Test-SigningCertificate $cert)) { return $null }
    return $cert
}

function Show-Status {
    Say "`n========== 代码签名状态 =========="
    $st = Resolve-SignTool
    if ($st) { Gray "  signtool : $st" } else { ErrMsg "  signtool : 未找到 (装 Windows SDK)" }

    if (-not $MOODFLOW_SIGN_THUMBPRINT) {
        Warn "  证书     : 未配置 —— 签名关闭"
        Gray "             配置方法: 复制 scripts\sign.local.ps1.example 为 sign.local.ps1"
        return $false
    }
    Gray "  指纹     : $MOODFLOW_SIGN_THUMBPRINT"

    $cert = Resolve-SigningCert
    if (-not $cert) {
        Warn "  会话     : 不可用 —— 证书不可用（检查存储位置、私钥、有效期与代码签名用途）"
        Gray "             多半是签名会话没开或已过期:"
        Gray "             $MOODFLOW_SIGN_SESSION_HINT"
        Gray "             建立会话后再跑一次 -Status"
        return $false
    }
    Say  "  会话     : 可用"
    Gray "  主体     : $($cert.Subject)"
    Gray "  颁发者   : $($cert.Issuer)"
    Gray "  有效期至 : $($cert.NotAfter)"
    $days = ($cert.NotAfter - (Get-Date)).Days
    if ($days -lt 30) { Warn "  ⚠️ 证书 $days 天后到期" }
    Gray "  时间戳   : $MOODFLOW_SIGN_TIMESTAMP_URL"
    return $true
}

# ---------- 签名 ----------
# Only a valid, timestamped signature from the configured identity may be reused.
# A matching fingerprint alone can also accompany a tampered file.
function Test-SignatureIdentity ($signature, [string]$thumbprint) {
    if (-not $signature -or "$($signature.Status)" -ne 'Valid' -or
        -not $signature.SignerCertificate -or -not $signature.TimeStamperCertificate) { return $false }
    return (-not $thumbprint -or $signature.SignerCertificate.Thumbprint -eq $thumbprint)
}

function Test-AlreadySigned ([string]$file, [string]$thumbprint) {
    try {
        return (Test-SignatureIdentity (Get-AuthenticodeSignature -LiteralPath $file -ErrorAction Stop) $thumbprint)
    } catch { return $false }
}

# 两类瞬时故障值得重试, 其余 (证书没了、指纹不对) 重试多少次都是一样的结果, 立即失败。
#
#   时间戳服务器抽风   —— 5xx / 超时, 签名流水线最常见的 flake。
#   文件被占用         —— 打包器刚写完 exe, 句柄未必已经释放; 实时防护也常在这一刻
#                        扫描新生成的文件。【实测踩过】: pack.ps1 出包后紧接着签名,
#                        报 "The file is being used by another process" ——
#                        而且是【间歇性】的, 同一条命令上一次还好好的。
#
# ⚠️ 判据不能只匹配英文: signtool 的错误信息随系统显示语言变化, 中文系统上是
#    「正由另一进程使用」。只匹配英文的话, 中文机器上这个可重试故障会被当成硬失败。
#    错误码 0x80070020 (ERROR_SHARING_VIOLATION) 与语言无关, 作为兜底一并匹配。
function Invoke-SignOne ([string]$signtool, [string]$thumbprint, [string]$file) {
    for ($i = 1; $i -le $MOODFLOW_SIGN_RETRY; $i++) {
        # /fd SHA256      文件摘要算法
        # /tr + /td       RFC3161 时间戳 (/t 是老式 Authenticode 时间戳, 不要用)
        # /sha1           按指纹选证书 —— 比 /n 主体名精确, 存储里有多张证书时不会选错
        $storeArgs = @()
        if ($MOODFLOW_SIGN_STORE -eq 'LocalMachine') { $storeArgs = @('/sm') }
        $out = & $signtool sign /fd SHA256 /sha1 $thumbprint @storeArgs `
            /tr $MOODFLOW_SIGN_TIMESTAMP_URL /td SHA256 $file 2>&1
        if ($LASTEXITCODE -eq 0) {
            if (Invoke-VerifyOne $signtool $file) { return $true }
            ErrMsg "签名后验证失败（发布者、信任链或时间戳不符）: $file"
            return $false
        }

        $text = ($out | Out-String)
        $why = $null; $wait = 0
        if ($text -match 'being used by another process|0x80070020|sharing violation|正由另一(个)?进程使用|另一个程序正在使用') {
            # 占用是短暂的, 等一两秒通常就好, 不必像时间戳那样退避到十几秒。
            $why = "文件被占用"; $wait = [math]::Min(2 * $i, 6)
        }
        elseif ($text -match 'timestamp|Timestamp|0x80070002|The specified timestamp server') {
            $why = "时间戳失败"; $wait = [math]::Min(5 * $i, 15)
        }

        if ($why -and $i -lt $MOODFLOW_SIGN_RETRY) {
            Warn "  $why, ${wait}s 后重试 ($i/$($MOODFLOW_SIGN_RETRY - 1)) ..."
            Start-Sleep -Seconds $wait
            continue
        }
        ErrMsg "  签名失败$(if ($why) { " ($why, 重试 $($MOODFLOW_SIGN_RETRY - 1) 次仍失败)" }): $file"
        Write-Host $text.TrimEnd()
        return $false
    }
    return $false   # 循环只能从上面的 return 退出, 到这里说明 $MOODFLOW_SIGN_RETRY < 1
}

function Invoke-VerifyOne ([string]$signtool, [string]$file) {
    # /pa = 用「Authenticode 代码签名」策略验证。不加它走的是默认驱动策略, 正常的
    # 应用程序签名会被判不合格 —— 这是验签环节最容易得出错误结论的地方。
    if (-not (Test-AlreadySigned $file $MOODFLOW_SIGN_THUMBPRINT)) { return $false }
    & $signtool verify /pa /all /tw /q $file 2>&1 | Out-Null
    return ($LASTEXITCODE -eq 0)
}

# ---------- 主流程 ----------
if ($Status) { if (Show-Status) { exit 0 } else { exit 1 } }

$signtool = Resolve-SignTool
if (-not $signtool) {
    ErrMsg "未找到 signtool.exe —— 请安装 Windows SDK。"
    exit 1
}

# 展开通配并过滤成真实的 PE 文件。目录参数只取【根层】的 exe/dll: 递归会把 data\ 下的
# 字体等一并卷进来, 那些不是我们的产物, 签它们既无意义也会拖慢出包。
$files = @()
foreach ($p in $Path) {
    $items = Get-Item -Path $p -ErrorAction SilentlyContinue
    foreach ($it in $items) {
        if ($it.PSIsContainer) {
            $files += Get-ChildItem $it.FullName -File |
                Where-Object { $_.Extension -in '.exe', '.dll' } |
                ForEach-Object { $_.FullName }
        } elseif ($it.Extension -in '.exe', '.dll') {
            $files += $it.FullName
        }
    }
}
$files = @($files | Select-Object -Unique)

if ($files.Count -eq 0) {
    Warn "没有可签名的文件 (参数: $($Path -join ', '))"
    if ($Verify -or $RequireSigned) { exit 1 }
    exit 0
}

if ($Verify) {
    Say "`n========== 验签 ($($files.Count) 个文件) =========="
    $bad = 0
    foreach ($f in $files) {
        if (Invoke-VerifyOne $signtool $f) {
            Gray "  ✓ $(Split-Path $f -Leaf)"
        } else {
            ErrMsg "  ✗ $(Split-Path $f -Leaf) —— 无有效签名"
            $bad++
        }
    }
    if ($bad -gt 0) { ErrMsg "`n$bad 个文件验签未通过"; exit 1 }
    Say "`n全部 $($files.Count) 个文件签名有效"
    exit 0
}

$cert = Resolve-SigningCert
if (-not $cert) {
    if ($RequireSigned) {
        ErrMsg "`n签名不可用, 但本次要求必须签名 —— 中止。"
        Show-Status | Out-Null
        exit 1
    }
    if ($MOODFLOW_SIGN_THUMBPRINT) {
        Warn "`n⚠️ 已配置证书但签名会话不可用 (未登录或已过期), 本次产物【未签名】。"
        Gray "   $MOODFLOW_SIGN_SESSION_HINT"
        Gray "   或用 .\scripts\sign.ps1 -Status 体检。"
    } else {
        Gray "`n未配置代码签名, 跳过 (模板: scripts\sign.local.ps1.example)"
    }
    exit 0
}

Say "`n========== 签名 ($($files.Count) 个文件) =========="
Gray "  证书: $($cert.Subject)"
$signed = 0; $skipped = 0
foreach ($f in $files) {
    $name = Split-Path $f -Leaf
    if (-not $Force -and (Test-AlreadySigned $f $cert.Thumbprint)) {
        Gray "  - $name (已签名, 跳过)"
        $skipped++
        continue
    }
    if (-not (Invoke-SignOne $signtool $cert.Thumbprint $f)) { exit 1 }
    Gray "  ✓ $name"
    $signed++
}
Say "`n签名完成: $signed 个已签, $skipped 个跳过"
exit 0
