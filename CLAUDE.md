# CLAUDE.md

Orientação para agentes trabalhando neste repositório.

## O que é

**YTCL** — cliente nativo de desktop (Linux/Windows) para o YouTube Music.
Fala direto com a API interna do YouTube (InnerTube); sem Chromium embutido.
UI em Svelte 5 sobre o webview do sistema, núcleo em Rust, áudio via libmpv.

O plano completo, o estado de cada fase e os problemas conhecidos estão em
**`docs/plano.md`** — leia antes de mudanças grandes. É a fonte da verdade sobre
o "porquê" das decisões.

## Layout

```
crates/ytcl-core/     # núcleo, sem Tauri: InnerTube, resolução de stream,
                      # cache SQLite, cofre de contas, config, cache de capas
crates/ytcl-player/   # reprodução: libmpv, proxy de stream, fila, orquestrador
src-tauri/            # ponte Tauri: comandos, eventos, protocolo ytmart://,
                      # janela de login, capabilities
src/                  # frontend Svelte 5 (runes) + Vite, sem framework de UI
docs/plano.md         # plano e problemas conhecidos
```

Regra de ouro dos crates: **`ytcl-core` e `ytcl-player` não dependem do Tauri.**
Dá para exercitar tudo por teste e por `cargo run --example`.

## Verificação (rode antes de commitar)

```sh
cargo test --workspace
cargo clippy --workspace --all-targets     # tem que passar limpo
npm run check                               # svelte-check
```

Testes marcados `#[ignore]` batem na rede/YouTube e são o canário quando a API
muda. Rodar sob demanda:

```sh
cargo test -p ytcl-core   --test innertube_live  -- --ignored --test-threads=1
cargo test -p ytcl-player --test playback_live   -- --ignored
# alguns aceitam VID=<videoId> para testar um vídeo específico
```

Diagnóstico rápido sem abrir a UI:

```sh
cargo run -p ytcl-core --example search  -- "radiohead" [songs|albums|artists]
cargo run -p ytcl-core --example library                 # rotas da biblioteca
```

## Rodar o app

```sh
npm install
npm run tauri dev                          # debug + hot reload do frontend
npm run tauri build -- --no-bundle          # release -> target/release/ytcl
```

**Sempre use `npm run tauri build`, nunca `cargo build` direto** — o `cargo build`
sozinho não embute o frontend e o app abre com "Could not connect to localhost".

Variáveis úteis ao depurar:

```sh
RUST_LOG=info,ytcl_player=debug,ytcl_core=debug ./target/release/ytcl
YTCL_MPV_LOG=/tmp/mpv.log ./target/release/ytcl     # log interno do mpv
```

## Dependências de sistema (dev)

Fedora: `webkit2gtk4.1-devel openssl-devel librsvg2-devel mpv-libs-devel gcc gcc-c++ make`
Mais: Rust (rustup), Node 20+, e **`yt-dlp` no PATH** (usado para resolver o
stream — ver abaixo).

## Decisões que NÃO devem ser quebradas

Estas sustentam a performance e a segurança. Ver `docs/plano.md` para o porquê.

1. **Capa nunca passa pelo IPC.** Vai pelo protocolo custom `ytmart://`
   (`src-tauri/src/protocol.rs`), servida do cache em disco. O frontend só
   aponta `<img src="ytmart://<hash>">`. Nunca mande imagem por `invoke`.
2. **Lista grande é paginada no comando**, não no frontend
   (`playlist_tracks(id, continuation)`, `library_*` com teto).
3. **Posição da faixa é emitida a ~4 Hz.** A barra interpola em CSS entre os
   eventos (`PlayerBar.svelte` / `player.svelte.ts::displayPosition`). Não subir
   a frequência.
4. **O cookie de sessão vai só para o cofre do SO** (`keyring`), nunca para
   arquivo. O `ScrubbedStorage` em `innertube.rs` remove `auth_cookie`/
   `oauth_token` antes de o rustypipe gravar o cache dele.
5. **`ytcl-core`/`ytcl-player` sem dependência de Tauri.**
6. **Modelos próprios na fronteira do IPC** (`ytcl-core/src/model.rs`,
   `#[serde(rename_all = "camelCase")]`). Não reexportar tipos do rustypipe nem
   do yt-dlp para o frontend.

## Resolução de stream: rustypipe vs yt-dlp

- **Metadados** (busca, álbum, artista, playlist, biblioteca): `rustypipe`,
  atrás da trait `MetadataSource`. `crates/ytcl-core/src/innertube.rs` é o
  ÚNICO arquivo que conhece os tipos do rustypipe.
- **Stream** (videoId → URL de áudio): **`yt-dlp`** via subprocesso
  (`crates/ytcl-core/src/ytdlp.rs`, trait `StreamResolver`). A decifragem de
  assinatura do rustypipe quebrou e as URLs do cliente iOS dele vêm truncadas.
  Detalhes em `docs/plano.md > Problemas conhecidos`.
- **Playback**: o mpv não fala com o YouTube direto — o áudio passa pelo
  protocolo `ytclstream://` (`crates/ytcl-player/src/proxy.rs`), que baixa via
  `reqwest` em blocos de 256 KB (o YouTube dá 403 em `Range` aberto).

## rustypipe está com `[patch.crates-io]`

O `Cargo.toml` da raiz aponta `rustypipe` para um fork
(`codeberg.org/Leoccker/rustypipe`) com um fix de uma linha. PR upstream:
`codeberg.org/ThetaDev/rustypipe/pulls/87`. O procedimento para voltar ao
oficial está em `docs/plano.md > Problemas conhecidos`.

## Git

- Branch de trabalho: `dev`. Fluxo: `dev` → PR → `main` no GitHub
  (`github.com/Leoccker/YTCL`).
- **Não** adicionar `Co-Authored-By: Claude` nem `Claude-Session:` nos commits.
- Autor: `Leoccker <leodrive013@gmail.com>` (identidade do `~/.gitconfig`).
- Mensagens de commit em português, explicando o "porquê".

## Aviso legal

O projeto usa a API interna do YouTube (zona cinzenta de ToS). É uso pessoal.
Ao mexer no README ou em qualquer texto público: o nome do projeto é **YTCL**,
"YouTube"/"YouTube Music" só como descrição; disclaimer de não afiliação; não
liderar com "sem anúncios". Ver `docs/plano.md > Context > Condições de
publicação`.
