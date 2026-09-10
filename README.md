# YTCL

Cliente de desktop, para Linux e Windows, que fala com o YouTube Music pela API
interna do YouTube (InnerTube). Sem Chromium embutido: a UI é Svelte sobre o
webview do sistema, o núcleo é Rust e o áudio passa pelo libmpv.

## Aviso

YTCL **não é afiliado, associado, autorizado nem endossado** pelo Google ou pelo
YouTube. "YouTube" e "YouTube Music" são marcas do Google LLC, usadas aqui só
para descrever a finalidade do programa.

O projeto acessa a API **interna** do YouTube, que não é pública nem documentada.
Usá-la contraria os Termos de Serviço do YouTube. É uma ferramenta de uso
pessoal, fornecida como está, sem garantia. Você responde pelo modo como a usa.

## Estado

| Fase | Estado |
|---|---|
| 0 — Fundação | concluída |
| 1 — Metadados e busca | concluída |
| 2 — Autenticação | concluída |
| 3 — Reprodução | concluída |
| 4 — UI completa | próxima |
| 5 — Empacotamento | pendente |

Hoje o app **busca** músicas, álbuns, artistas e playlists direto do YouTube
Music, **conecta a uma conta** (biblioteca, playlists, curtidas) e **toca**
áudio — clicar numa música da busca ou das curtidas inicia a reprodução, com
fila, gapless, shuffle e repeat. Falta a navegação por telas de detalhe
(abrir um álbum/artista/playlist) — é a Fase 4.

O plano completo — arquitetura, fases, alvos de performance, problemas
conhecidos — está em **[`docs/plano.md`](docs/plano.md)**, com o estado de cada
fase e o "porquê" das decisões.

## Como funciona

| O quê | Como |
|---|---|
| Busca, biblioteca, álbum, artista, playlist | `rustypipe` (cliente InnerTube em Rust) |
| videoId → URL de áudio tocável | **`yt-dlp`** (subprocesso, ~1–2 s por faixa) |
| Baixar e decodificar o áudio | libmpv, servido pelo protocolo interno `ytclstream://` |
| Capas | protocolo interno `ytmart://`, cache em disco, nunca pelo IPC |
| Cookie de sessão | cofre de senhas do SO (Secret Service / Keychain / Credential Manager) |

O `yt-dlp` é usado só para descobrir a URL do stream, não para tocar. A
decifragem de assinatura do `rustypipe` quebrou contra o YouTube atual e as
alternativas dele vêm truncadas; o `yt-dlp` é a implementação de referência e é
atualizada toda semana. Ver `docs/plano.md > Problemas conhecidos`.

## Dependências do sistema

### Fedora
```sh
sudo dnf install -y webkit2gtk4.1-devel openssl-devel \
  libappindicator-gtk3-devel librsvg2-devel mpv-libs-devel \
  gcc gcc-c++ make file dpkg yt-dlp
```

### Debian/Ubuntu
```sh
sudo apt install -y libwebkit2gtk-4.1-dev libssl-dev \
  libayatana-appindicator3-dev librsvg2-dev libmpv-dev \
  build-essential file yt-dlp
```

### Windows
Visual Studio Build Tools (C++), WebView2 Runtime (já vem no Windows 11), a
`libmpv-2.dll` e o `yt-dlp.exe` — o bundler (Fase 5) empacota os dois como
recurso.

Além disso: [Rust](https://rustup.rs) e Node 20+.

**`yt-dlp`** precisa estar no `PATH` (ou apontado por config). A busca e a
biblioteca funcionam sem ele; tocar não. Nas distribuições ele costuma estar
no repositório oficial; senão, `pipx install yt-dlp` ou o
[binário standalone](https://github.com/yt-dlp/yt-dlp/releases).

## Desenvolvimento

```sh
npm install
npm run tauri dev        # debug + hot reload do frontend
```

Release local (o binário fica em `target/release/ytcl`):

```sh
npm run tauri build -- --no-bundle
```

Use sempre `npm run tauri build`, nunca `cargo build` direto — o `cargo build`
sozinho não embute o frontend e o app abre com "Could not connect to localhost".

### Depuração

```sh
RUST_LOG=info,ytcl_player=debug,ytcl_core=debug ./target/release/ytcl
YTCL_MPV_LOG=/tmp/mpv.log ./target/release/ytcl     # log interno do mpv
```

`F3` no app abre o overlay de performance: fps, pior frame do último segundo e
memória. No Linux o WebKitGTK roda em processos separados (`ytcl`,
`WebKitWebProcess`, `WebKitNetworkProcess`), então o overlay soma a árvore.
Mostra **RSS** (critério de aceite, alvo < 500 MB, é o que o monitor do sistema
exibe) e **PSS** ao lado (divide as páginas compartilhadas — na tela vazia o
release ficou em ~390 MB de RSS contra ~211 MB de PSS; a diferença é o piso do
GTK/WebKit, o binário do app é ~3,6 MB disso).

## Verificação

```sh
cargo test --workspace
cargo clippy --workspace --all-targets      # tem que passar limpo
npm run check                                # svelte-check
```

Testes marcados `#[ignore]` batem na rede/YouTube (o canário quando a API muda):

```sh
cargo test -p ytcl-core   --test innertube_live -- --ignored --test-threads=1
cargo test -p ytcl-player --test playback_live  -- --ignored
```

Diagnóstico sem abrir a UI:

```sh
cargo run -p ytcl-core --example search  -- "radiohead" [songs|albums|artists]
cargo run -p ytcl-core --example library                  # rotas da biblioteca
```

## Arquitetura

| Camada | Onde | Papel |
|---|---|---|
| UI | `src/` | Svelte 5 (runes) + Vite, sem framework de componentes |
| Ponte | `src-tauri/` | comandos, eventos, protocolos `ytmart://` e `ytclstream://`, janela de login |
| Núcleo | `crates/ytcl-core/` | InnerTube (rustypipe), stream (yt-dlp), cache SQLite, cofre de contas |
| Áudio | `crates/ytcl-player/` | libmpv, proxy de stream, fila, gapless |

`ytcl-core` e `ytcl-player` não dependem do Tauri — dá para exercitar tudo por
teste e por `cargo run --example`.

Regras que sustentam a responsividade e não devem ser quebradas:

1. **Capa nunca passa pelo IPC.** Vai pelo protocolo `ytmart://`, servida do
   cache em disco, para o webview decodificar fora da thread principal.
2. **Lista grande é paginada no comando**, não no frontend.
3. **Posição da faixa é emitida a ~4 Hz**, com a barra interpolando em CSS.

## Licença

GPL-3.0-only.
