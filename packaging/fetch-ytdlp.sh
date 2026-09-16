#!/usr/bin/env bash
# Baixa o binário standalone do yt-dlp (release "latest" do GitHub) e grava
# em src-tauri/resources/yt-dlp/yt-dlp, conferindo o checksum publicado pelo
# próprio projeto. O bundler do Tauri (tauri.conf.json > bundle.resources)
# instala esse arquivo em resource_dir()/yt-dlp/yt-dlp no pacote final — ver
# CLAUDE.md e docs/plano.md > Fase 5.
#
# Usado localmente (build de release) e pelo CI do Linux. Funciona a partir
# de qualquer diretório de trabalho: resolve a raiz do repo pelo caminho do
# próprio script, não pelo cwd.
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" &>/dev/null && pwd)"
ROOT_DIR="$(cd -- "$SCRIPT_DIR/.." &>/dev/null && pwd)"

DEST_DIR="$ROOT_DIR/src-tauri/resources/yt-dlp"
DEST_BIN="$DEST_DIR/yt-dlp"
BASE_URL="https://github.com/yt-dlp/yt-dlp/releases/latest/download"
ASSET="yt-dlp_linux"

mkdir -p "$DEST_DIR"

TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT

echo "Baixando $ASSET..." >&2
curl -fL --retry 3 --retry-connrefused -o "$TMP_DIR/$ASSET" "$BASE_URL/$ASSET"
curl -fL --retry 3 --retry-connrefused -o "$TMP_DIR/SHA2-256SUMS" "$BASE_URL/SHA2-256SUMS"

# O SHA2-256SUMS lista vários assets da release, um por linha
# ("<hash>  <nome>"). Pegamos só a linha do arquivo que baixamos, ancorada no
# fim da linha — sem o "$" no fim, "yt-dlp_linux" também bateria com
# "yt-dlp_linux_aarch64" ou "yt-dlp_linux.zip".
EXPECTED_LINE="$(grep -E "[[:space:]]${ASSET}\$" "$TMP_DIR/SHA2-256SUMS" || true)"
if [[ -z "$EXPECTED_LINE" ]]; then
    echo "erro: '$ASSET' não encontrado em SHA2-256SUMS" >&2
    exit 1
fi

echo "Conferindo checksum..." >&2
if ! (cd "$TMP_DIR" && echo "$EXPECTED_LINE" | sha256sum --check --strict -); then
    echo "erro: checksum do $ASSET não bateu" >&2
    exit 1
fi

mv "$TMP_DIR/$ASSET" "$DEST_BIN"
chmod 0755 "$DEST_BIN"

echo "yt-dlp gravado em $DEST_BIN"
