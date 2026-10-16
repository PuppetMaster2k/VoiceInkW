# Shared helpers for versioning. The <Version> in VoiceInk.App.csproj holds the plain MAJOR.MINOR.PATCH release version;
# the "-rc" suffix is never stored, it is added to packages built outside main.
$script:AppProject = Join-Path (Split-Path -Parent $PSScriptRoot) 'src/VoiceInk.App/VoiceInk.App.csproj'
# A feature bump that would reach this MINOR rolls over to the next MAJOR (X.19.y + feature -> X+1.0.0).
$script:MinorRollover = 20
$script:VersionPattern = '(<Version>)(\d+\.\d+\.\d+)(</Version>)'

function Get-ProjectVersion([string]$Xml) {
    if ($Xml -notmatch $script:VersionPattern) {
        throw "No plain MAJOR.MINOR.PATCH <Version> found in $script:AppProject."
    }
    [version]$Matches[2]
}

function Get-CurrentBranch {
    $branch = (git -C $PSScriptRoot rev-parse --abbrev-ref HEAD).Trim()
    if ($LASTEXITCODE -ne 0) { throw 'Not inside a git repository.' }
    $branch
}

function Test-MainBranch([string]$Branch) { $Branch -in 'main', 'master' }
