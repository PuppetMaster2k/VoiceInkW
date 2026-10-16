<#
.SYNOPSIS
Bumps the semantic version in VoiceInk.App.csproj for the current feature/fix branch.
  -Kind feature  -> MINOR + 1 (PATCH reset); reaching MINOR 20 rolls over to MAJOR + 1, MINOR 0
  -Kind fix      -> PATCH + 1
  -Kind breaking -> MAJOR + 1
The new version is computed from main's version, so running it again on the same branch never bumps twice
(a branch with both a feature and a fix ends up at the larger bump). Packages built off main get an "-rc" suffix.
#>
param(
    [Parameter(Mandatory)]
    [ValidateSet('feature', 'fix', 'breaking')]
    [string]$Kind
)

$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'Version.ps1')

$branch = Get-CurrentBranch
if (Test-MainBranch $branch) { throw "Run this on a feature/fix branch, not '$branch'." }

$base = 'origin/main'
git -C $PSScriptRoot fetch origin main --quiet 2>$null
if ($LASTEXITCODE -ne 0) { Write-Warning 'Could not fetch origin/main; using the last known copy.' }
$mainXml = git -C $PSScriptRoot show "${base}:windows-dotnet/src/VoiceInk.App/VoiceInk.App.csproj" 2>$null
if ($LASTEXITCODE -ne 0) { $base = 'main'; $mainXml = git -C $PSScriptRoot show "${base}:windows-dotnet/src/VoiceInk.App/VoiceInk.App.csproj" }
$main = Get-ProjectVersion ($mainXml -join "`n")

$target = switch ($Kind) {
    'feature'  {
        if ($main.Minor + 1 -ge $script:MinorRollover) { [version]::new($main.Major + 1, 0, 0) }
        else { [version]::new($main.Major, $main.Minor + 1, 0) }
    }
    'fix'      { [version]::new($main.Major, $main.Minor, $main.Build + 1) }
    'breaking' { [version]::new($main.Major + 1, 0, 0) }
}

$text = Get-Content -LiteralPath $script:AppProject -Raw
$current = Get-ProjectVersion $text
if ($current -ge $target) {
    Write-Output "Version stays $current (already >= $target, based on $base at $main)."
    return
}
$text = [regex]::Replace($text, $script:VersionPattern, "`${1}$target`${3}")
Set-Content -LiteralPath $script:AppProject -Value $text -NoNewline
Write-Output "Version $current -> $target ($Kind, based on $base at $main). Packages from '$branch' will be $target-rc."
