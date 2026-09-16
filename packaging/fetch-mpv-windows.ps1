# Baixa o SDK de desenvolvimento do libmpv (build do shinchiro,
# github.com/shinchiro/mpv-winbuild-cmake) para linkar e rodar no Windows.
#
# O libmpv2-sys so pede `cargo:rustc-link-lib=mpv`, ou seja: o linker do MSVC
# procura "mpv.lib". O pacote do shinchiro NAO traz esse import lib pronto
# (verificado no release "mpv-dev-x86_64-*.7z" atual: so vem libmpv-2.dll,
# libmpv.dll.a — import lib do mingw, que o link.exe do MSVC nao le — e os
# headers). Entao:
#   1. Se um .def vier incluso num release futuro, usamos ele direto (`lib
#      /def:`).
#   2. Senao (caso de hoje), extraimos a tabela de exports da propria DLL
#      com o dumpbin (que acompanha o lib.exe no mesmo toolchain do MSVC) e
#      montamos um .def na mao.
# Em ambos os casos o resultado e o mesmo: um mpv.lib que o MSVC entende.
#
# A DLL vai pro recurso do bundle (src-tauri/resources/windows/libmpv-2.dll,
# ver tauri.windows.conf.json); o resto (mpv.lib, headers, cache do
# download) fica fora do git, so interessa durante o build.
#
# Requer PowerShell 7+ (pwsh), 7z no PATH (o runner windows-latest do GitHub
# Actions ja vem com ele) e as "Desktop development with C++" do Visual
# Studio / Build Tools (para o lib.exe e o dumpbin.exe).
$ErrorActionPreference = 'Stop'

$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$RootDir = Split-Path -Parent $ScriptDir

$CacheDir = if ($env:MPV_WINBUILD_CACHE) { $env:MPV_WINBUILD_CACHE } else { Join-Path $env:LOCALAPPDATA 'ytcl\mpv-winbuild' }
$ResourcesDir = Join-Path $RootDir 'src-tauri/resources/windows'

New-Item -ItemType Directory -Force -Path $CacheDir | Out-Null
New-Item -ItemType Directory -Force -Path $ResourcesDir | Out-Null

function Get-VCTool {
    # Acha uma ferramenta do toolchain MSVC (lib.exe, dumpbin.exe, ...):
    # primeiro no PATH (caso já estejamos num "Developer Prompt"), senão via
    # vswhere, que o instalador do Visual Studio / Build Tools sempre deixa
    # nesse caminho fixo.
    param([string]$Name)

    $cmd = Get-Command $Name -ErrorAction SilentlyContinue
    if ($cmd) {
        return $cmd.Source
    }

    $VsWhere = 'C:\Program Files (x86)\Microsoft Visual Studio\Installer\vswhere.exe'
    if (-not (Test-Path $VsWhere)) {
        throw "erro: $Name nao esta no PATH e vswhere.exe nao foi encontrado"
    }
    $vsInstall = & $VsWhere -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
    if (-not $vsInstall) {
        throw 'erro: nenhuma instalacao do Visual Studio com as ferramentas C++ foi encontrada'
    }
    $found = Get-ChildItem -Path (Join-Path $vsInstall 'VC\Tools\MSVC') -Recurse -Filter $Name |
        Where-Object { $_.FullName -match '\\Hostx64\\x64\\' } |
        Select-Object -First 1
    if (-not $found) {
        throw "erro: $Name nao encontrado dentro da instalacao do Visual Studio"
    }
    return $found.FullName
}

# --- Acha o asset "mpv-dev-x86_64-*.7z" (sem "-v3") na ultima release ---
Write-Host 'Consultando a ultima release do mpv-winbuild-cmake...'
$release = Invoke-RestMethod -Uri 'https://api.github.com/repos/shinchiro/mpv-winbuild-cmake/releases/latest' -Headers @{ 'User-Agent' = 'ytcl-packaging' }
# "-v3" e a variante otimizada pra x86-64-v3 (AVX2); queremos a generica.
$asset = $release.assets |
    Where-Object { $_.name -match '^mpv-dev-x86_64-.*\.7z$' -and $_.name -notmatch '-v3' } |
    Select-Object -First 1
if (-not $asset) {
    throw 'erro: nao achei o asset mpv-dev-x86_64-*.7z (sem -v3) na ultima release'
}

$ArchivePath = Join-Path $CacheDir $asset.name
if (-not (Test-Path $ArchivePath)) {
    Write-Host "Baixando $($asset.name)..."
    Invoke-WebRequest -Uri $asset.browser_download_url -OutFile $ArchivePath
}
else {
    Write-Host "Ja em cache: $ArchivePath"
}

# --- Extrai ---
$ExtractDir = Join-Path $CacheDir 'extracted'
if (Test-Path $ExtractDir) {
    Remove-Item -Recurse -Force $ExtractDir
}
New-Item -ItemType Directory -Force -Path $ExtractDir | Out-Null

$SevenZip = Get-Command '7z' -ErrorAction SilentlyContinue
if (-not $SevenZip) {
    throw 'erro: 7z nao encontrado no PATH (o runner windows-latest do GitHub Actions ja vem com ele)'
}
& $SevenZip.Source x $ArchivePath "-o$ExtractDir" -y | Out-Null

$Dll = Get-ChildItem -Path $ExtractDir -Recurse -Filter 'libmpv-2.dll' | Select-Object -First 1
if (-not $Dll) {
    throw 'erro: libmpv-2.dll nao encontrada no pacote extraido'
}
Copy-Item -Force -Path $Dll.FullName -Destination (Join-Path $ResourcesDir 'libmpv-2.dll')

# --- Gera o import library (mpv.lib) pro MSVC ---
$LibExe = Get-VCTool -Name 'lib.exe'

$DefFile = Get-ChildItem -Path $ExtractDir -Recurse -Filter '*.def' | Select-Object -First 1
if ($DefFile) {
    $DefPath = $DefFile.FullName
}
else {
    Write-Host 'Nenhum .def no pacote - gerando a partir dos exports da DLL...'
    $DumpbinExe = Get-VCTool -Name 'dumpbin.exe'
    $dumpOutput = & $DumpbinExe '/exports' $Dll.FullName

    # Linhas da tabela de exports do dumpbin: "  <ordinal>  <hint(hex)>  <RVA(hex)>  <nome>"
    $exports = foreach ($line in $dumpOutput) {
        if ($line -match '^\s*\d+\s+[0-9A-Fa-f]+\s+[0-9A-Fa-f]+\s+(\S+)\s*$') {
            $Matches[1]
        }
    }
    if (-not $exports -or $exports.Count -eq 0) {
        throw 'erro: nao consegui extrair nenhum simbolo exportado de libmpv-2.dll (dumpbin /exports mudou de formato?)'
    }

    $GeneratedDef = Join-Path $CacheDir 'libmpv-2.def'
    Set-Content -Path $GeneratedDef -Encoding ascii -Value 'LIBRARY libmpv-2'
    Add-Content -Path $GeneratedDef -Encoding ascii -Value 'EXPORTS'
    $exports | ForEach-Object { "    $_" } | Add-Content -Path $GeneratedDef -Encoding ascii

    $DefPath = $GeneratedDef
    Write-Host "$($exports.Count) simbolos exportados encontrados"
}

$LibOut = Join-Path $CacheDir 'mpv.lib'
Push-Location $CacheDir
try {
    & $LibExe "/def:$DefPath" '/machine:x64' "/out:$LibOut" | Out-Null
}
finally {
    Pop-Location
}
if (-not (Test-Path $LibOut)) {
    throw 'erro: geracao do mpv.lib falhou'
}

# --- Exporta as variaveis pro resto do build ---
$env:MPV_LIB_DIR = $CacheDir
$env:PATH = "$ResourcesDir;$env:PATH"

# Passos seguintes do GitHub Actions rodam em processos novos; sem isso, so
# esta sessao do pwsh enxergaria as variaveis.
if ($env:GITHUB_ENV) {
    Add-Content -Path $env:GITHUB_ENV -Value "MPV_LIB_DIR=$CacheDir"
}
if ($env:GITHUB_PATH) {
    Add-Content -Path $env:GITHUB_PATH -Value $ResourcesDir
}

Write-Host "MPV_LIB_DIR=$CacheDir"
Write-Host "DLL copiada para $ResourcesDir"
