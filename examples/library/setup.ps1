#Requires -Version 7.0
[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$Destination,
    [string]$Clerk = 'clerk'
)
$ErrorActionPreference = 'Stop'
$destinationPath = [IO.Path]::GetFullPath($Destination, (Get-Location).Path)
if (Test-Path -LiteralPath $destinationPath) {
    throw "Destination already exists: $destinationPath. Choose a new directory."
}
function Invoke-Checked([string]$Program, [string[]]$Arguments) {
    & $Program @Arguments
    if ($LASTEXITCODE -ne 0) { throw "$Program failed with exit code $LASTEXITCODE." }
}
New-Item -ItemType Directory -Path "$destinationPath/.clerk/config" -Force | Out-Null
Copy-Item -LiteralPath "$PSScriptRoot/workspace.toml" -Destination "$destinationPath/.clerk/workspace.toml"
foreach ($folder in @('registry', 'kinds', 'workflows')) {
    Copy-Item -LiteralPath "$PSScriptRoot/$folder" -Destination "$destinationPath/.clerk/config/$folder" -Recurse
}
Copy-Item -LiteralPath "$PSScriptRoot/templates" -Destination "$destinationPath/.clerk/templates" -Recurse
[IO.File]::WriteAllText("$destinationPath/.gitignore", ".work/`n", [Text.UTF8Encoding]::new($false))
[IO.File]::WriteAllText("$destinationPath/.gitattributes", "* text=auto eol=lf`n", [Text.UTF8Encoding]::new($false))
New-Item -ItemType Directory -Path "$destinationPath/contracts" -Force | Out-Null
$contractIndex = "# Contracts`n`n| File | Title |`n|---|---|`n"
[IO.File]::WriteAllText("$destinationPath/contracts/README.md", $contractIndex, [Text.UTF8Encoding]::new($false))
Push-Location -LiteralPath $destinationPath
try {
    Invoke-Checked git @('init', '-b', 'main')
    Invoke-Checked git @('config', 'user.name', 'Alice')
    Invoke-Checked git @('config', 'user.email', 'alice@example.com')
    Invoke-Checked $Clerk @('--root', '.', 'init')
    $status = "# Library reservation project`n`n"
    foreach ($region in @('tasks', 'assets', 'milestone', 'verify', 'limits')) {
        $status += "<!-- clerk:begin $region -->`n<!-- clerk:end $region -->`n`n"
    }
    [IO.File]::WriteAllText("$destinationPath/status.md", $status, [Text.UTF8Encoding]::new($false))
    Invoke-Checked git @('add', '--all')
    Invoke-Checked git @('commit', '-m', 'chore(example): configure library planning workspace')
    Invoke-Checked $Clerk @('--root', '.', 'batch', '--file', "$PSScriptRoot/seed.toml")
    Invoke-Checked $Clerk @('--root', '.', 'check')
    Invoke-Checked git @('add', '--all')
    Invoke-Checked git @('commit', '-m', 'docs(example): initialize library planning workspace')
} finally {
    Pop-Location
}
Write-Output "Example ready: $destinationPath"
