$ErrorActionPreference = 'Stop'
$PSNativeCommandUseErrorActionPreference = $true
if (-not $env:TAURI_SIGNING_PRIVATE_KEY) { throw 'Configure TAURI_SIGNING_PRIVATE_KEY before publishing updates.' }
cargo build --release --locked -p remote-codex
$key = (Get-Content crates/client/src/updates/public.key -Raw).Trim()
$config = Join-Path $env:RUNNER_TEMP 'updater-config.json'
@{ bundle = @{ createUpdaterArtifacts = $true }; plugins = @{ updater = @{ pubkey = $key } } } |
    ConvertTo-Json -Depth 5 | Set-Content -Encoding utf8NoBOM $config
npm --prefix apps/desktop run tauri -- build --bundles nsis --config $config
$out = 'target/release-artifacts'
New-Item -ItemType Directory -Force $out | Out-Null
$cli = Join-Path $out 'remote-codex-cli-windows-x64.zip'
Compress-Archive -Path target/release/remote-codex.exe -DestinationPath $cli -Force
npm --prefix apps/desktop run tauri -- signer sign (Resolve-Path $cli).Path
$installers = @(Get-ChildItem target/release/bundle/nsis/*-setup.exe)
if ($installers.Count -ne 1) { throw 'Expected one NSIS installer.' }
$app = Join-Path $out 'remote-codex-app-windows-x64.exe'
Copy-Item $installers[0].FullName $app
Copy-Item ($installers[0].FullName + '.sig') ($app + '.sig')
$version = $env:RELEASE_VERSION.TrimStart('v')
$url = "https://github.com/$env:GITHUB_REPOSITORY/releases/download/$env:RELEASE_VERSION/remote-codex-app-windows-x64.exe"
@{ version = $version; platforms = @{ 'windows-x86_64' = @{ url = $url; signature = (Get-Content ($app + '.sig') -Raw).Trim() } } } |
    ConvertTo-Json -Depth 5 | Set-Content -Encoding utf8NoBOM (Join-Path $out 'latest.json')
Get-ChildItem $out -File | ForEach-Object {
    $hash = (Get-FileHash $_.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
    "$hash  $($_.Name)"
} | Set-Content -Encoding utf8NoBOM (Join-Path $out 'SHA256SUMS-windows')
