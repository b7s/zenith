param(
    [Parameter(Position = 0, Mandatory = $true)]
    [string] $Version
)

$ErrorActionPreference = "Stop"

$files = @(
    "package.json",
    "src-tauri/tauri.conf.json",
    "src-tauri/Cargo.toml"
)

foreach ($f in $files) {
    if (-not (Test-Path $f)) { Write-Error "missing $f"; exit 1 }
    $content = Get-Content $f -Raw

    if ($f -eq "src-tauri/Cargo.toml") {
        # The crate version is the top-level `version = "x.y.z"` line.
        $new = $content -replace '(?m)^version\s*=\s*"[0-9]+\.[0-9]+\.[0-9]+"', "version = `"$Version`""
    }
    else {
        $new = $content -replace '"version"\s*:\s*"[0-9]+\.[0-9]+\.[0-9]+"', "`"version`": `"$Version`""
    }

    Set-Content $f $new -NoNewline
}

Write-Host "Bumped version to $Version"
