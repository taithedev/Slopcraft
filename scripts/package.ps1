param(
    [string]$TargetDirectory = "target",
    [string]$OutputDirectory = "dist"
)

$ErrorActionPreference = 'Stop'
$project = Split-Path -Parent $PSScriptRoot
$target = [IO.Path]::GetFullPath((Join-Path $project $TargetDirectory))
$output = [IO.Path]::GetFullPath((Join-Path $project $OutputDirectory))
New-Item -ItemType Directory -Force $output | Out-Null
$stage = Join-Path $output ('package-' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory $stage | Out-Null

Copy-Item -LiteralPath (Join-Path $target 'release/slopcraft.exe') -Destination $stage
Copy-Item -LiteralPath (Join-Path $target 'release/launcher.exe') -Destination (Join-Path $stage 'SlopcraftLauncher.exe')
Copy-Item -LiteralPath (Join-Path $project 'README.md'),(Join-Path $project 'THIRD_PARTY_NOTICES.txt') -Destination $stage
Copy-Item -LiteralPath (Join-Path $project 'docs'),(Join-Path $project 'examples'),(Join-Path $project 'shaderpacks') -Destination $stage -Recurse

$archive = Join-Path $output 'Slopcraft-Windows-x64.zip'
if (Test-Path -LiteralPath $archive) {
    Remove-Item -LiteralPath $archive
}
Add-Type -AssemblyName System.IO.Compression.FileSystem
[IO.Compression.ZipFile]::CreateFromDirectory($stage, $archive, [IO.Compression.CompressionLevel]::Optimal, $false)
$checksum = (Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash.ToLowerInvariant()
[IO.File]::WriteAllText((Join-Path $output 'Slopcraft-Windows-x64.sha256'), "$checksum  Slopcraft-Windows-x64.zip`n", [Text.UTF8Encoding]::new($false))

# The staging directory was created directly within this output directory.
$resolvedStage = [IO.Path]::GetFullPath($stage)
if ([IO.Path]::GetDirectoryName($resolvedStage) -ne $output -or [IO.Path]::GetFileName($resolvedStage) -notlike 'package-*') {
    throw 'Unexpected packaging stage directory'
}
Remove-Item -LiteralPath $resolvedStage -Recurse
Get-Item -LiteralPath $archive,(Join-Path $output 'Slopcraft-Windows-x64.sha256')
