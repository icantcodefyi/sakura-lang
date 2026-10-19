param([Parameter(Mandatory)][string]$Binary)
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.IO.Compression.FileSystem
$Binary = (Resolve-Path -LiteralPath $Binary).Path
$Version = (& $Binary --version).Split(' ')[1]
$Installer = Join-Path $PSScriptRoot 'install.ps1'
$Work = Join-Path ([System.IO.Path]::GetTempPath()) ('sakura installer test ' + [guid]::NewGuid())
New-Item -ItemType Directory -Path $Work | Out-Null
function New-Release([string]$Name, [string]$ReportedVersion = $Version, [string]$Extra = '') {
    $Release = Join-Path $Work $Name
    New-Item -ItemType Directory -Path $Release | Out-Null
    $Top = "sakuralang-$ReportedVersion-x86_64-pc-windows-msvc"
    $Archive = Join-Path $Release "$Top.zip"
    $Zip = [System.IO.Compression.ZipFile]::Open($Archive, 'Create')
    try {
        [System.IO.Compression.ZipFileExtensions]::CreateEntryFromFile($Zip, $Binary, "$Top/sakura.exe") | Out-Null
        if ($Extra) {
            $Entry = $Zip.CreateEntry($Extra)
            $Stream = $Entry.Open()
            $Stream.WriteByte(1)
            $Stream.Dispose()
        }
    } finally { $Zip.Dispose() }
    $Hash = (Get-FileHash -Algorithm SHA256 -LiteralPath $Archive).Hash.ToLowerInvariant()
    Set-Content -LiteralPath (Join-Path $Release 'SHA256SUMS') -Value "$Hash  $Top.zip"
    return $Release
}
function Expect([string]$Name, [bool]$Pass, [string]$Release, [string]$RequestedVersion = $Version) {
    $Prefix = Join-Path $Work "prefix $Name"
    $Succeeded = $false
    try {
        & $Installer -Version $RequestedVersion -Prefix $Prefix -BaseUrl $Release
        $Succeeded = $true
    } catch {
        Write-Output "Refusal for $Name`: $($_.Exception.Message)"
    }
    if ($Succeeded -ne $Pass) { throw "Unexpected outcome for $Name" }
    $Installed = Join-Path $Prefix 'bin/sakura.exe'
    if (!$Pass -and (Test-Path -LiteralPath $Installed)) { throw "Refused but installed $Name" }
    if ($Pass) {
        & $Installed --version
        if ($LASTEXITCODE -ne 0) { throw "Installed binary failed for $Name" }
        & $Installer -Version $RequestedVersion -Prefix $Prefix -BaseUrl $Release
        if (@(Get-ChildItem -LiteralPath (Join-Path $Prefix 'bin')).Count -ne 1) { throw 'Staging files leaked' }
    }
    Write-Output "ok $Name"
}
try {
    $BeforePath = $env:PATH
    Expect 'good release with spaces' $true (New-Release 'good')
    $Bad = New-Release 'bad hash'
    Set-Content -LiteralPath (Join-Path $Bad 'SHA256SUMS') -Value ((('0' * 64) + "  sakuralang-$Version-x86_64-pc-windows-msvc.zip"))
    Expect 'bad hash' $false $Bad
    $Missing = New-Release 'missing sum'
    Set-Content -LiteralPath (Join-Path $Missing 'SHA256SUMS') -Value ''
    Expect 'missing sum' $false $Missing
    $Duplicate = New-Release 'duplicate sum'
    $Sums = Join-Path $Duplicate 'SHA256SUMS'
    Add-Content -LiteralPath $Sums -Value (Get-Content -LiteralPath $Sums)
    Expect 'duplicate sum' $false $Duplicate
    Expect 'traversal' $false (New-Release 'traversal' $Version '../escaped')
    Expect 'wrong version' $false (New-Release 'wrong version' '9.9.9') '9.9.9'
    Expect 'insecure URL' $false 'http://example.invalid'
    Expect 'missing release' $false (Join-Path $Work 'absent')
    if ($env:PATH -ne $BeforePath) { throw 'Installer changed PATH' }
    if (Test-Path -LiteralPath (Join-Path $Work 'escaped')) { throw 'Traversal wrote outside archive' }
    Write-Output 'all Windows installer tests passed'
} finally { Remove-Item -LiteralPath $Work -Recurse -Force }
