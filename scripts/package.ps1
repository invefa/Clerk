#Requires -Version 7.0
param(
    [string]$OutputDirectory = ''
)

$ErrorActionPreference = 'Stop'
$sourceRoot = Split-Path -Parent $PSScriptRoot
Push-Location -LiteralPath $sourceRoot
try {
    if ($env:CLERK_BUILD_COMMIT -or $env:CLERK_BUILD_DIRTY) {
        throw 'Remove CLERK_BUILD_COMMIT and CLERK_BUILD_DIRTY before packaging.'
    }
    $sourceStatus = & git status --porcelain --untracked-files=normal
    if ($LASTEXITCODE -ne 0 -or $sourceStatus) {
        throw 'Packaging requires a clean Git checkout.'
    }
    $sourceCommit = & git rev-parse HEAD
    if ($LASTEXITCODE -ne 0) { throw 'Cannot read the source commit.' }
    $compiler = & rustc --version --verbose
    if ($LASTEXITCODE -ne 0 -or $compiler -notcontains 'host: x86_64-pc-windows-msvc') {
        throw 'This package requires the x86_64-pc-windows-msvc host toolchain.'
    }
    & cargo build --locked --release
    if ($LASTEXITCODE -ne 0) { throw 'Release build failed.' }
    $versionText = & ./target/release/clerk.exe version --json
    if ($LASTEXITCODE -ne 0) { throw 'Release binary failed its version check.' }
    $build = $versionText | ConvertFrom-Json
    if ($build.dirty -ne $false -or $build.commit -ne $sourceCommit) {
        throw 'The binary must identify the current clean source commit.'
    }
    if ($build.version -notmatch '^\d+\.\d+\.\d+$') { throw 'Invalid release version.' }
    $name = "clerk-$($build.version)-windows-x86_64"
    if (-not $OutputDirectory) {
        $OutputDirectory = Join-Path $sourceRoot "target/release-dist/$name"
    }
    $destination = [IO.Path]::GetFullPath($OutputDirectory, $sourceRoot)
    if (Test-Path -LiteralPath $destination) {
        throw "Output directory already exists: $destination"
    }
    $package = Join-Path $destination $name
    New-Item -ItemType Directory -Path $package -Force | Out-Null
    foreach ($file in @('clerk.exe')) {
        Copy-Item -LiteralPath (Join-Path $sourceRoot "target/release/$file") -Destination $package
    }
    foreach ($entry in @('README.md', 'CONTRIBUTING.md', 'LICENSE', 'NOTICE', 'docs', 'examples', 'src', 'tests', 'scripts', 'Cargo.toml', 'Cargo.lock',
        'build.rs', 'rust-toolchain.toml', 'rustfmt.toml')) {
        Copy-Item -LiteralPath (Join-Path $sourceRoot $entry) -Destination $package -Recurse
    }
    $metadata = [ordered]@{
        version = $build.version
        commit = $build.commit
        dirty = $build.dirty
        target = 'x86_64-pc-windows-msvc'
        compiler = $compiler
        formats = $build.formats
    } | ConvertTo-Json -Depth 5
    $utf8 = [Text.UTF8Encoding]::new($false)
    [IO.File]::WriteAllText((Join-Path $package 'build.json'), "$metadata`n", $utf8)
    $archive = Join-Path $destination "$name.zip"
    Compress-Archive -LiteralPath $package -DestinationPath $archive -CompressionLevel Optimal
    $digest = (Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash.ToLowerInvariant()
    [IO.File]::WriteAllText((Join-Path $destination 'SHA256SUMS'), "$digest  $name.zip`n", $utf8)
    $afterStatus = & git status --porcelain --untracked-files=normal
    if ($LASTEXITCODE -ne 0 -or $afterStatus) {
        throw 'Packaging changed the checkout; review it before publishing.'
    }
    Write-Output "Package: $archive"
    Write-Output "Commit: $($build.commit)"
    Write-Output "SHA256: $digest"
} finally {
    Pop-Location
}
