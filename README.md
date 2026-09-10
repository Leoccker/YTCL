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
| 2 — Autenticação | pendente |
| 3 — Reprodução | pendente |
| 4 — UI completa | pendente |
| 5 — Empacotamento | pendente |

Hoje o app **busca** músicas, álbuns, artistas e playlists direto do YouTube
Music, com cache em SQLite e lista virtualizada. Ainda **não reproduz** áudio
(Fase 3) nem conecta a uma conta (Fase 2).

Para exercitar o núcleo sem abrir a UI:

```sh
cargo run -p ytcl-core --example search -- "radiohead"
cargo run -p ytcl-core --example search -- "caetano veloso" albums
```

O plano completo — arquitetura, fases, alvos de performance, riscos — está em
**[`docs/plano.md`](docs/plano.md)**, com o estado de cada fase no topo.

## Dependências do sistema

### Fedora
```sh
sudo dnf install -y webkit2gtk4.1-devel openssl-devel \
  libappindicator-gtk3-devel librsvg2-devel mpv-libs-devel \
  gcc gcc-c++ make file dpkg
```

### Debian/Ubuntu
```sh
sudo apt install -y libwebkit2gtk-4.1-dev libssl-dev \
  libayatana-appindicator3-dev librsvg2-dev libmpv-dev \
  build-essential file
```

### Windows
Visual Studio Build Tools (C++), WebView2 Runtime (já vem no Windows 11), e a
`libmpv-2.dll` — que o bundler empacota como recurso.

Além disso: [Rust](https://rustup.rs) e Node 20+.

## Desenvolvimento

```sh
npm install
npm run tauri dev
```

`F3` abre o overlay de performance: fps, pior frame do último segundo e memória
da árvore de processos. No Linux o WebKitGTK roda em processos separados
(`ytcl`, `WebKitWebProcess`, `WebKitNetworkProcess`), então medir só o processo
principal esconderia a maior parte.

O overlay mostra dois números de memória. **RSS** é o critério de aceite
(alvo: < 500 MB) porque é o que o monitor do sistema mostra. **PSS** fica ao
lado para diagnóstico: divide cada página compartilhada entre os processos que
a usam. Na Fase 0, com a tela vazia, o release media 390 MB de RSS contra
211 MB de PSS — a diferença é o GTK, a libc e o WebKit que os três processos
compartilham. Desse total, o binário do app respondia por 3,6 MB; o resto é
piso do toolkit.

## Verificação

```sh
cargo test --workspace
cargo clippy --all-targets -- -D warnings
npm run check
```

## Arquitetura

| Camada | Onde | Papel |
|---|---|---|
| UI | `src/` | Svelte 5 (runes) + Vite, sem framework de componentes |
| Ponte | `src-tauri/` | comandos, eventos, protocolo `ytmart://`, janela de login |
| Núcleo | `crates/ytcl-core/` | InnerTube, resolução de stream, cache SQLite, auth |
| Áudio | `crates/ytcl-player/` | libmpv, fila, gapless |

Três regras que sustentam a responsividade e não devem ser quebradas:

1. **Capa nunca passa pelo IPC.** Vai pelo protocolo `ytmart://`, servida do
   cache em disco, para o webview decodificar fora da thread principal.
2. **Lista grande é paginada no comando**, não no frontend.
3. **Posição da faixa é emitida a ~4 Hz**, com a barra interpolando em CSS.

## Licença

GPL-3.0-only.
