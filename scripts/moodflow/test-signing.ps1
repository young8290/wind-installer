# Pure policy tests: no private key access, certificate installation or actual signing.
$ErrorActionPreference = 'Stop'
$signPath = Join-Path (Split-Path $PSScriptRoot -Parent) 'sign.ps1'
$tokens = $null; $errors = $null
$ast = [System.Management.Automation.Language.Parser]::ParseFile($signPath, [ref]$tokens, [ref]$errors)
if ($errors.Count) { throw ($errors | Out-String) }
$wanted = @('Test-SigningCertificate', 'Test-SignatureIdentity', 'Test-AlreadySigned', 'Invoke-VerifyOne', 'Invoke-SignOne')
foreach ($definition in $ast.FindAll({ param($n) $n -is [System.Management.Automation.Language.FunctionDefinitionAst] -and $n.Name -in $wanted }, $false)) {
    . ([scriptblock]::Create($definition.Extent.Text))
}
function Assert ([bool]$condition, [string]$message) { if (-not $condition) { throw $message } }
function ErrMsg ($message) { $script:lastError = $message }
function Warn ($message) { $script:lastError = $message }
$now = [datetime]'2026-11-08T00:00:00Z'
$cert = [pscustomobject]@{
    HasPrivateKey = $true; NotBefore = $now.AddDays(-1); NotAfter = $now.AddDays(1)
    EnhancedKeyUsageList = @([pscustomobject]@{ ObjectId = '1.3.6.1.5.5.7.3.3' })
}
Assert (Test-SigningCertificate $cert $now) 'Valid signing certificate was rejected'
$cert.NotAfter = $now
Assert (-not (Test-SigningCertificate $cert $now)) 'Expired certificate was accepted'
$cert.NotAfter = $now.AddDays(1); $cert.NotBefore = $now.AddDays(1)
Assert (-not (Test-SigningCertificate $cert $now)) 'Future certificate was accepted'
$cert.NotBefore = $now.AddDays(-1); $cert.HasPrivateKey = $false
Assert (-not (Test-SigningCertificate $cert $now)) 'Missing private key was accepted'
$cert.HasPrivateKey = $true; $cert.EnhancedKeyUsageList = @()
Assert (-not (Test-SigningCertificate $cert $now)) 'Missing code signing EKU was accepted'
$MOODFLOW_SIGN_THUMBPRINT = 'A' * 40
$MOODFLOW_SIGN_STORE = 'LocalMachine'
$MOODFLOW_SIGN_TIMESTAMP_URL = 'https://timestamp.example.invalid'
$MOODFLOW_SIGN_RETRY = 1
$script:signature = [pscustomobject]@{
    Status = 'Valid'; SignerCertificate = [pscustomobject]@{ Thumbprint = $MOODFLOW_SIGN_THUMBPRINT }
    TimeStamperCertificate = [pscustomobject]@{ Thumbprint = 'B' * 40 }
}
function Get-AuthenticodeSignature { param($LiteralPath, $ErrorAction) $script:signature }
function FakeSignTool {
    $script:toolArgs = @($args)
    $global:LASTEXITCODE = 0
}
Assert (Test-AlreadySigned 'fixture.exe' $MOODFLOW_SIGN_THUMBPRINT) 'Valid signature was rejected'
Assert (-not (Test-AlreadySigned 'fixture.exe' ('C' * 40))) 'Wrong publisher was accepted'
foreach ($status in @('HashMismatch', 'NotTrusted', 'NotSigned', 'UnknownError')) {
    $script:signature.Status = $status
    Assert (-not (Test-AlreadySigned 'fixture.exe' $MOODFLOW_SIGN_THUMBPRINT)) "Unsafe signature accepted: $status"
}
$script:signature.Status = 'Valid'; $stamp = $script:signature.TimeStamperCertificate
$script:signature.TimeStamperCertificate = $null
Assert (-not (Invoke-VerifyOne 'FakeSignTool' 'fixture.exe')) 'Missing timestamp was accepted'
$script:signature.TimeStamperCertificate = $stamp
Assert (Invoke-VerifyOne 'FakeSignTool' 'fixture.exe') 'Verification failed'
Assert (($script:toolArgs -join ' ') -eq 'verify /pa /all /tw /q fixture.exe') 'Verification policy changed'
Assert (Invoke-SignOne 'FakeSignTool' $MOODFLOW_SIGN_THUMBPRINT 'fixture.exe') 'Signing did not verify immediately'
$script:signature.Status = 'HashMismatch'
Assert (-not (Invoke-SignOne 'FakeSignTool' $MOODFLOW_SIGN_THUMBPRINT 'fixture.exe')) 'Signing reported success without verification'
Write-Host 'Signing policy: 16 checks passed; no certificates or files modified.'
