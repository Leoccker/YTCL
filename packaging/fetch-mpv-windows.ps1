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
# O pacote e fixado por tag, nome e SHA-256 (ver "Pino" abaixo): esta DLL e
# carregada dentro do processo do app, entao o build do mesmo commit tem que
# produzir sempre o mesmo binario, e nada e extraido antes do hash bater.
#
# Requer PowerShell 7+ (pwsh), 7z no PATH (o runner windows-latest do GitHub
# Actions ja vem com ele) e as "Desktop development with C++" do Visual
# Studio / Build Tools (para o lib.exe e o dumpbin.exe).
$ErrorActionPreference = 'Stop'

# --- Pino da dependencia -------------------------------------------------
# Os tres valores mudam juntos, num PR a parte, depois de conferir o hash.
# Como atualizar (a variante "-v3" e otimizada pra AVX2; queremos a generica):
#
#   gh api repos/shinchiro/mpv-winbuild-cmake/releases/latest --jq `
#     '.tag_name, (.assets[] | select(.name | test("^mpv-dev-x86_64-[^v].*\\.7z$")) | .name, .digest)'
#
# O `digest` vem da propria API do GitHub; confirme baixando o arquivo e
# rodando `(Get-FileHash <arquivo> -Algorithm SHA256).Hash.ToLower()`.
$MpvTag = '20260920'
$MpvAsset = 'mpv-dev-x86_64-20260920-git-e76a35ec95.7z'
$MpvSha256 = '60f9102db46aea8cef9bfb4345ee6a106f34fdbd1df9587e38f0660688039341'

$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$RootDir = Split-Path -Parent $ScriptDir

$CacheDir = if ($env:MPV_WINBUILD_CACHE) { $env:MPV_WINBUILD_CACHE } else { Join-Path $env:LOCALAPPDATA 'ytcl\mpv-winbuild' }
$ResourcesDir = Join-Path $RootDir 'src-tauri/resources/windows'

New-Item -ItemType Directory -Force -Path $CacheDir | Out-Null
New-Item -ItemType Directory -Force -Path $ResourcesDir | Out-Null

function Get-Sha256 {
    param([string]$Path)

    return (Get-FileHash -Path $Path -Algorithm SHA256).Hash.ToLowerInvariant()
}

function Invoke-Checked {
    # Roda um executavel e transforma codigo de saida diferente de zero em
    # erro. Sem isto, um 7z ou um lib.exe que falha passa despercebido e o
    # build segue com o que sobrou da rodada anterior.
    param(
        [string]$Exe,
        [string[]]$Arguments,
        [string]$What
    )

    # Sem `2>&1`: com $ErrorActionPreference='Stop' a mistura dos fluxos de
    # um executavel externo vira erro terminante por qualquer aviso. O stderr
    # segue visivel no log; o que decide aqui e o codigo de saida.
    $output = & $Exe @Arguments
    if ($LASTEXITCODE -ne 0) {
        throw "erro: $What falhou (codigo $LASTEXITCODE)"
    }
    return $output
}

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

# --- Baixa o asset fixado e confere o SHA-256 ---
$DownloadUrl = "https://github.com/shinchiro/mpv-winbuild-cmake/releases/download/$MpvTag/$MpvAsset"
$ArchivePath = Join-Path $CacheDir $MpvAsset

if (Test-Path $ArchivePath) {
    # O cache e um diretorio comum do usuario, gravavel por qualquer processo
    # dele: conferir o hash aqui e o que impede um .7z trocado virar uma DLL
    # carregada dentro do app. Nada e extraido antes desta checagem.
    $cached = Get-Sha256 -Path $ArchivePath
    if ($cached -ne $MpvSha256) {
        Remove-Item -Force $ArchivePath
        throw ("erro: o $MpvAsset em cache nao confere com o pino.`n" +
               "  esperado: $MpvSha256`n" +
               "  obtido:   $cached`n" +
               'O arquivo foi removido do cache. Rode o script de novo para baixar outra vez; se repetir, revise o pino.')
    }
    Write-Host "Ja em cache e conferido: $ArchivePath"
}
else {
    Write-Host "Baixando $MpvAsset ($MpvTag)..."
    # Baixa para um temporario exclusivo e so promove depois de conferir:
    # assim o cache nunca chega a guardar um arquivo nao verificado, nem
    # duas execucoes simultaneas escrevem no mesmo destino.
    $Partial = Join-Path $CacheDir ('{0}.{1}.part' -f $MpvAsset, [guid]::NewGuid().ToString('n'))
    try {
        Invoke-WebRequest -Uri $DownloadUrl -OutFile $Partial -Headers @{ 'User-Agent' = 'ytcl-packaging' }
        $got = Get-Sha256 -Path $Partial
        if ($got -ne $MpvSha256) {
            throw ("erro: o download de $MpvAsset nao confere com o pino.`n" +
                   "  esperado: $MpvSha256`n" +
                   "  obtido:   $got")
        }
        Move-Item -Force -Path $Partial -Destination $ArchivePath
    }
    finally {
        if (Test-Path $Partial) {
            Remove-Item -Force $Partial
        }
    }
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
Invoke-Checked -Exe $SevenZip.Source -Arguments @('x', $ArchivePath, "-o$ExtractDir", '-y') -What '7z x' | Out-Null

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
    $dumpOutput = Invoke-Checked -Exe $DumpbinExe -Arguments @('/exports', $Dll.FullName) -What 'dumpbin /exports'

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
# Apaga antes: senao um lib.exe que falhou deixaria o mpv.lib da rodada
# anterior no lugar, e o link usaria uma biblioteca de outra versao.
if (Test-Path $LibOut) {
    Remove-Item -Force $LibOut
}
Push-Location $CacheDir
try {
    Invoke-Checked -Exe $LibExe -Arguments @("/def:$DefPath", '/machine:x64', "/out:$LibOut") -What 'lib.exe /def' | Out-Null
}
finally {
    Pop-Location
}
if (-not (Test-Path $LibOut)) {
    throw 'erro: lib.exe terminou sem erro mas nao produziu o mpv.lib'
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
Write-Host "DLL copiada para $ResourcesDir (libmpv $MpvTag)"
