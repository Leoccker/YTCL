# Baixa o binario standalone do yt-dlp (release "latest" do GitHub) para
# Windows e grava em src-tauri/resources/yt-dlp/yt-dlp.exe, conferindo o
# checksum publicado pelo proprio projeto. Espelha fetch-ytdlp.sh (Linux) -
# ver esse script para o porque do formato do SHA2-256SUMS.
#
# Requer PowerShell 7+ (pwsh). Usado localmente e pelo CI do Windows
# (windows-latest, ver .github/workflows).
$ErrorActionPreference = 'Stop'

$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$RootDir = Split-Path -Parent $ScriptDir

$DestDir = Join-Path $RootDir 'src-tauri/resources/yt-dlp'
$DestBin = Join-Path $DestDir 'yt-dlp.exe'
$BaseUrl = 'https://github.com/yt-dlp/yt-dlp/releases/latest/download'
$Asset = 'yt-dlp.exe'

New-Item -ItemType Directory -Force -Path $DestDir | Out-Null

$TmpDir = Join-Path ([System.IO.Path]::GetTempPath()) ([System.Guid]::NewGuid())
New-Item -ItemType Directory -Force -Path $TmpDir | Out-Null

try {
    $TmpBin = Join-Path $TmpDir $Asset
    $TmpSums = Join-Path $TmpDir 'SHA2-256SUMS'

    Write-Host "Baixando $Asset..."
    Invoke-WebRequest -Uri "$BaseUrl/$Asset" -OutFile $TmpBin
    Invoke-WebRequest -Uri "$BaseUrl/SHA2-256SUMS" -OutFile $TmpSums

    # O SHA2-256SUMS lista varios assets da release ("<hash>  <nome>"), um
    # por linha. Ancorado no fim da linha para nao casar "yt-dlp.exe" com um
    # nome parecido por acidente.
    $pattern = '\s' + [regex]::Escape($Asset) + '$'
    $matchLine = Select-String -Path $TmpSums -Pattern $pattern | Select-Object -First 1
    if (-not $matchLine) {
        throw "erro: '$Asset' nao encontrado em SHA2-256SUMS"
    }
    $expectedHash = ($matchLine.Line -split '\s+')[0].ToLowerInvariant()

    Write-Host 'Conferindo checksum...'
    $actualHash = (Get-FileHash -Path $TmpBin -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($actualHash -ne $expectedHash) {
        throw "erro: checksum do $Asset nao bateu (esperado $expectedHash, obtido $actualHash)"
    }

    Move-Item -Force -Path $TmpBin -Destination $DestBin
    Write-Host "yt-dlp gravado em $DestBin"
}
finally {
    Remove-Item -Recurse -Force -Path $TmpDir -ErrorAction SilentlyContinue
}
