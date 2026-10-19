param(
    [string]$Version = "",
    [string]$Prefix = (Join-Path $env:LOCALAPPDATA "Programs\sakuralang"),
    [string]$BaseUrl = ""
)
$ErrorActionPreference = "Stop"
Add-Type -AssemblyName System.IO.Compression.FileSystem
[Net.ServicePointManager]::SecurityProtocol = [Net.ServicePointManager]::SecurityProtocol -bor [Net.SecurityProtocolType]::Tls12
$ProgressPreference = 'SilentlyContinue'
$DefaultVersion = ""
if (!$Version) { $Version = $DefaultVersion }
$Version = $Version -replace '^v', ''
if ($Version -notmatch '^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?$') { throw "Pass -Version with a pinned release version" }
if ($Prefix -notmatch '^(?:[A-Za-z]:[\\/]|\\\\[^\\]+\\[^\\]+)') { throw "-Prefix must be an absolute path" }
$Architecture = $env:PROCESSOR_ARCHITECTURE
if ($env:PROCESSOR_ARCHITEW6432) { $Architecture = $env:PROCESSOR_ARCHITEW6432 }
if ($Architecture -ne 'AMD64') { throw "Only Windows x64 is supported" }
if (!$BaseUrl) { $BaseUrl = "https://github.com/icantcodefyi/sakuralang/releases/download/v$Version" }
$BaseUrl = $BaseUrl.TrimEnd('/')
if ($BaseUrl -match '^\w+://' -and $BaseUrl -notmatch '^https://') { throw "Only HTTPS or a local release directory is accepted" }
$Target = 'x86_64-pc-windows-msvc'
$Top = "sakuralang-$Version-$Target"
$Archive = "$Top.zip"
$Temp = Join-Path ([System.IO.Path]::GetTempPath()) ([guid]::NewGuid().ToString())
$Staged = $null
New-Item -ItemType Directory -Path $Temp | Out-Null
function Get-ReleaseFile([string]$Name) {
    $Destination = Join-Path $Temp $Name
    if ($BaseUrl -match '^https://') {
        Invoke-WebRequest -UseBasicParsing -Uri "$BaseUrl/$Name" -OutFile $Destination
    } else {
        Copy-Item -LiteralPath (Join-Path $BaseUrl $Name) -Destination $Destination
    }
}
try {
    Get-ReleaseFile 'SHA256SUMS'
    $Entries = @(Get-Content -LiteralPath (Join-Path $Temp 'SHA256SUMS') | Where-Object {
        $_ -match ('^[0-9a-f]{64}  ' + [regex]::Escape($Archive) + '$')
    })
    if ($Entries.Count -ne 1) { throw "SHA256SUMS must contain exactly one checksum for $Archive; nothing installed" }
    $Expected = $Entries[0].Substring(0, 64)
    Get-ReleaseFile $Archive
    $Downloaded = Join-Path $Temp $Archive
    if ((Get-FileHash -Algorithm SHA256 -LiteralPath $Downloaded).Hash.ToLowerInvariant() -ne $Expected) {
        throw "SHA-256 mismatch for $Archive; nothing installed"
    }
    $Zip = [System.IO.Compression.ZipFile]::OpenRead($Downloaded)
    try {
        $Names = [System.Collections.Generic.HashSet[string]]::new([System.StringComparer]::OrdinalIgnoreCase)
        foreach ($Entry in $Zip.Entries) {
            $Name = $Entry.FullName
            $Mode = ($Entry.ExternalAttributes -shr 16) -band 0xF000
            if ($Name -notmatch ('^' + [regex]::Escape($Top) + '/[^/\\]+$') -or
                $Name.Contains('..') -or $Name -match '[:*?"<>|]' -or !$Names.Add($Name) -or $Mode -notin @(0, 0x8000)) {
                throw "Unsafe ZIP entry $Name; nothing installed"
            }
        }
    } finally { $Zip.Dispose() }
    Expand-Archive -LiteralPath $Downloaded -DestinationPath (Join-Path $Temp 'unpacked')
    $Binary = Join-Path $Temp "unpacked\$Top\sakura.exe"
    if (!(Test-Path -LiteralPath $Binary -PathType Leaf)) { throw "$Archive has no sakura.exe" }
    $Reported = & $Binary --version
    if ($LASTEXITCODE -ne 0 -or $Reported -notmatch ('^sakura ' + [regex]::Escape($Version) + '(?: |$)')) {
        throw "Downloaded binary does not report sakura $Version"
    }
    $Bin = Join-Path $Prefix 'bin'
    New-Item -ItemType Directory -Force -Path $Bin | Out-Null
    $Staged = Join-Path $Bin ('.sakura-install-' + [guid]::NewGuid().ToString() + '.exe')
    Copy-Item -LiteralPath $Binary -Destination $Staged
    Move-Item -LiteralPath $Staged -Destination (Join-Path $Bin 'sakura.exe') -Force
    $Staged = $null
    Write-Output "Installed $Reported to $Bin\sakura.exe"
    Write-Output "Add $Bin to PATH yourself if needed. Node 26 is required to run programs and tests."
} finally {
    if ($Staged -and (Test-Path -LiteralPath $Staged)) { Remove-Item -LiteralPath $Staged -Force }
    Remove-Item -LiteralPath $Temp -Recurse -Force
}
