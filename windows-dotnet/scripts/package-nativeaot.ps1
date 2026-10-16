param(
    [ValidateSet('x64', 'arm64')]
    [string]$Architecture = 'x64'
)

$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path -Parent $PSScriptRoot
$appProject = Join-Path $projectRoot 'src/VoiceInk.App/VoiceInk.App.csproj'
$artifacts = Join-Path $projectRoot 'artifacts'

. (Join-Path $PSScriptRoot 'Version.ps1')

# The plain version comes from VoiceInk.App.csproj; builds outside main are release candidates ("-rc").
$base = Get-ProjectVersion (Get-Content -LiteralPath $appProject -Raw)
$branch = Get-CurrentBranch
$version = if (Test-MainBranch $branch) { "$base" } else { "$base-rc" }

$platform = $Architecture
$runtime = "win-$Architecture"
$tag = "v$version"
$releaseDir = Join-Path (Split-Path -Parent $projectRoot) "release/windows/$tag"
New-Item -ItemType Directory -Path $releaseDir -Force | Out-Null

# Use a fresh directory so a package can never include stale publish files.
$publishDir = Join-Path $artifacts ("VoiceInk-NativeAOT-$runtime-" + [guid]::NewGuid().ToString('N').Substring(0, 8))
dotnet publish $appProject -p:PublishProfile=NativeAOT -p:Platform=$platform -p:RuntimeIdentifier=$runtime -p:Version=$version -o $publishDir
if ($LASTEXITCODE -ne 0) { throw 'NativeAOT publish failed; no archive was created.' }

$releaseArchive = Join-Path $releaseDir "VoiceInk-$Architecture-$version.zip"
Compress-Archive -Path (Join-Path $publishDir '*') -DestinationPath $releaseArchive -Force
Get-FileHash -LiteralPath $releaseArchive -Algorithm SHA256
Write-Output "Version: $version (branch: $branch)"
Write-Output "Release package: $releaseArchive"
