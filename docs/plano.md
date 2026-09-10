# Cliente nativo do YouTube Music para Linux e Windows

## Context

Não existe um cliente desktop de YouTube Music que seja realmente leve. As opções atuais
(`th-ch/youtube-music`, `ytmdesktop`) são Electron embrulhando `music.youtube.com`: 150–300 MB de
RAM, binário de 100+ MB, e a responsividade é a do site — pesado, cheio de animações e carregando
anúncios.

O objetivo é um cliente que fala direto com a API interna do YouTube (InnerTube), com fila e
decodificação de áudio locais, e uma UI própria e enxuta. Sem Chromium embutido, sem o site no
caminho crítico.

Decisões tomadas com o usuário:

| Decisão | Escolha |
|---|---|
| Arquitetura | Nativo + InnerTube (o site nunca é carregado) |
| UI | **Tauri 2 + Svelte 5** (webview do SO, ~12 MB de binário) |
| Login | Janela de webview nativa do Tauri com o login do Google |
| Escopo v1 | Busca + biblioteca + playlists (+ reprodução, que é pré-requisito) |

O núcleo pesado — cliente InnerTube, resolução de stream, libmpv, cache SQLite — é Rust puro e
independente da camada de UI.

**Aviso honesto sobre o risco principal:** este app usa a API interna do YouTube, não uma API
pública documentada. É zona cinzenta em relação aos Termos de Serviço do Google, e a API muda sem
aviso — a decifragem de assinatura e os identificadores de cliente quebram algumas vezes por ano.
O projeto exige manutenção contínua. Todo o plano isola essa fragilidade numa única camada
trocável.

---

## Arquitetura

```
 ┌──────────────────────────────────────────────────────────┐
 │  Frontend — Svelte 5 (runes) + Vite + TypeScript         │
 │  listas virtualizadas · CSS puro · sem framework de UI   │
 └───────┬─────────────────────────────────┬────────────────┘
         │ invoke() comandos               │ ytmart:// (capas)
         │ listen() eventos                │ protocolo custom, sem IPC
 ┌───────▼─────────────────────────────────▼────────────────┐
 │  src-tauri — comandos, eventos, janela de login          │
 └───────┬──────────────────────────────────────────────────┘
         │
 ┌───────▼──────────────┐   ┌──────────────────────────────┐
 │  ytm-core (tokio)    │   │  ytm-player                  │
 │   ├ InnerTube        │   │   ├ libmpv (vid=no)          │
 │   ├ resolver stream  │◀──┤   ├ fila + histórico         │
 │   ├ auth + keyring   │   │   └ pré-resolução da próxima │
 │   └ cache SQLite     │   └──────────────────────────────┘
 └──────────────────────┘
```

A regra que garante a responsividade: **nenhum comando Tauri bloqueia**. Todo comando é `async`,
roda no runtime tokio, e o frontend renderiza estado otimista enquanto espera. Progresso de
reprodução e mudanças de estado chegam por `emit`, não por polling.

### Três decisões de performance específicas do Tauri

Estas são o que separa um app Tauri fluido de um lento — valem mais que qualquer micro-otimização:

1. **Capas nunca passam pelo IPC.** Registrar um protocolo custom `ytmart://` em Rust que serve os
   arquivos do cache em disco. O webview passa a tratá-las como imagens normais: decodifica fora da
   thread principal, cacheia sozinho, e o IPC fica livre. Mandar base64 por `invoke` seria o erro
   clássico que engasga a rolagem de uma grade de álbuns.
2. **Listas grandes são paginadas no comando, não no frontend.** `get_playlist_tracks(id, offset,
   limit)` em vez de devolver 5.000 faixas de uma vez. Serializar JSON gigante trava os dois lados.
3. **Posição da faixa é emitida a ~4 Hz, não a cada frame.** A barra de progresso interpola em CSS
   entre os eventos. Emitir a 60 Hz satura o canal de eventos sem ganho visual algum.

### Escolhas de biblioteca, e por quê

**Frontend — Svelte 5 com runes, Vite, TypeScript, sem SvelteKit.** SvelteKit traz roteamento com
conceitos de SSR que não existem num app desktop; um SPA Vite puro com um enum de rota e pilha de
navegação é mais leve e mais direto. Svelte compila para JS sem virtual DOM — é a escolha certa
quando o alvo é o WebKitGTK, que é mais lento que o Chromium e agradece menos trabalho em runtime.
Sem biblioteca de componentes: CSS próprio, variáveis CSS para o tema.

**Dados — `ytmapi-rs` + `rustypipe`, cada um no que é melhor.**
- `ytmapi-rs` é uma porta em Rust do `ytmusicapi`, com endpoints tipados do YouTube Music (busca
  com filtros, biblioteca, playlists, artistas, álbuns) e auth por cookie. É a camada de
  **metadados e navegação**.
- `rustypipe` é mais forte na parte difícil: decifragem de assinatura e do parâmetro `n`,
  versionamento dos clientes InnerTube, cache do JS do player, fallback entre identidades de
  cliente. É a camada de **resolução de stream**.

Os dois ficam atrás de traits (`MetadataSource`, `StreamResolver`) em `ytm-core`: uma quebra de API
significa trocar uma implementação, não reescrever o app.

**PO token / BotGuard:** as notas do rustypipe indicam que o cliente **YouTube Music envia mas não
exige** o token `pot`, e o cliente **TV não o usa**. Com as identidades certas, o v1 provavelmente
não precisa de BotGuard. Mesmo assim a trait `StreamResolver` prevê um `PoTokenProvider` opcional,
com `rustypipe-botguard` como plano B. Não entra no v1.

**Áudio — `libmpv2` chamando libmpv em modo somente-áudio (`vid=no`).** Verificado no planejamento:
o **Symphonia ainda não tem decodificador Opus funcional** (só um PR em andamento para o modo
SILK). Como o melhor áudio do YouTube Music é o itag 251 (Opus/WebM), um player 100% Rust ficaria
preso ao AAC do itag 140 — e ainda teria que implementar streaming HTTP com Range, buffer, seek e
gapless do zero. O libmpv entrega tudo isso testado: gapless, `--cache` em disco, `--replaygain`
alimentado pelo `loudnessDb` que o próprio YouTube devolve.

> Nota: **não** usar `tauri-plugin-libmpv` — ele existe para embutir vídeo numa janela e é
> experimental no Linux. Para áudio, o crate `libmpv2` direto em `ytm-player`, sem saída de vídeo,
> é mais simples e totalmente multiplataforma.

**Login — janela de webview nativa do Tauri.** `WebviewWindowBuilder` abre o login normal do
Google numa janela separada, com user-agent de navegador desktop real (sem isso o Google devolve
"este navegador não é seguro"). Ao detectar navegação para `music.youtube.com`, ler os cookies com
a API `cookies_for_url` (disponível no Tauri 2 via wry ≥ 0.47) e fechar a janela.
- **Alternativa sem login interativo:** botão "Importar do navegador" com o crate `rookie`, que lê
  e descriptografa cookies de Chrome/Firefox/Edge/Chromium em Linux e Windows.
- Cookies vão para o keyring do SO via `keyring-rs` (Secret Service no Linux, Credential Manager no
  Windows), nunca em arquivo texto.

**Cache — `rusqlite` com feature `bundled`.** Um SQLite guarda faixas, álbuns, artistas, playlists
e resultados de busca, com TTL por tipo. Capas ficam em `~/.cache/ytmc/art/` nomeadas pelo hash da
URL e servidas pelo `ytmart://`. A UI **sempre** lê do cache primeiro e revalida em segundo plano —
é isso que faz a navegação parecer instantânea.

---

## Estrutura do repositório

```
Cargo.toml                # [workspace]
package.json              # frontend
vite.config.ts
index.html
src/                      # Svelte 5 + TS
  main.ts
  lib/
    api.ts                # wrappers tipados de invoke() + listen()
    stores/               # estado com runes: player, queue, library, auth
    components/           # TrackRow, PlayerBar, ArtGrid, VirtualList, ContextMenu
    views/                # Search, Library, Playlist, Album, Artist
    theme.css
src-tauri/                # crate do app Tauri
  src/
    main.rs
    commands/             # search.rs, library.rs, player.rs, auth.rs
    protocol.rs           # handler do ytmart://
    login_window.rs
  tauri.conf.json
crates/
  ytm-core/
    src/{lib,model,auth,metadata,stream,cache,artwork,config}.rs
  ytm-player/
    src/{lib,mpv,queue,state}.rs
packaging/                # ícones, .desktop, wrapper de lançamento no Linux
```

---

## Plano de execução

### Fase 0 — Fundação
1. `cargo create-tauri-app` com template Svelte + TS; converter em workspace com `crates/`.
2. Runtime tokio gerenciado como estado do Tauri (`app.manage(...)`); todo comando é `async`.
3. `ytm-core/src/config.rs`: diretórios via `directories-next` (`~/.config/ytmc`,
   `~/.cache/ytmc`), config em TOML.
4. `src-tauri/src/protocol.rs`: registrar `ytmart://` já nesta fase — servindo um placeholder — pra
   que o padrão esteja estabelecido antes de qualquer tela existir.
5. Tema em `src/lib/theme.css` com variáveis CSS; sem biblioteca de componentes.
6. **Sanity check de performance desde já:** um overlay de debug (F3) com FPS do frontend
   (`requestAnimationFrame`) e RSS do processo. Se a rolagem cair de 60 fps em qualquer fase
   seguinte, resolve-se antes de continuar.

### Fase 1 — Metadados e busca
1. Trait `MetadataSource` em `ytm-core/src/metadata.rs` (`search`, `get_album`, `get_artist`,
   `get_playlist`, `get_library`, `get_liked`) sobre modelos próprios em `model.rs` — não expor os
   tipos do `ytmapi-rs`, para desacoplar.
2. Implementar `YtmApiSource` com `ytmapi-rs` sem autenticação primeiro (busca pública já funciona
   sem login, o que permite ver a UI viva cedo).
3. `ytm-core/src/cache.rs`: schema SQLite + migrações, `get_or_fetch` com TTL e revalidação em
   background (stale-while-revalidate).
4. `ytm-core/src/artwork.rs`: fila de download com `tokio::Semaphore`, gravação em disco, e o
   `ytmart://` servindo com `Cache-Control` longo. O frontend só usa `<img src="ytmart://hash">`.
5. `views/Search.svelte` + `components/VirtualList.svelte`: virtualização por janela de rolagem
   (renderizar só as linhas visíveis + margem). Comandos paginados.

### Fase 2 — Autenticação
1. `src-tauri/src/login_window.rs`: `WebviewWindowBuilder` com URL de login do Google e UA de
   desktop; listener de navegação; ao chegar em `music.youtube.com`, `cookies_for_url` e fechar.
2. `ytm-core/src/auth.rs`: validar os cookies com uma chamada autenticada de teste antes de aceitar;
   gravar no keyring com `keyring-rs`, indexado por `account_id`.
3. Caminho alternativo `import_from_browser()` com `rookie`, filtrando `.youtube.com`.
4. Renovação: detectar 401/403 nas chamadas, marcar a sessão como expirada e mostrar um banner não
   modal de reconexão — sem interromper o que está tocando.
5. Múltiplas contas com seletor no menu desde já: retrofitar isso depois é caro.

### Fase 3 — Reprodução
1. `ytm-player/src/mpv.rs`: libmpv com `vid=no`, `audio-display=no`, `cache=yes`, `cache-secs=120`,
   `demuxer-max-bytes=64MiB`, `replaygain=track`, `gapless-audio=yes`. Loop de eventos do mpv numa
   thread dedicada, traduzido para `PlayerEvent` e emitido ao frontend (posição a ~4 Hz).
2. `ytm-core/src/stream.rs`: trait `StreamResolver`; implementação com `rustypipe` escolhendo a
   melhor trilha só de áudio (Opus 251 → AAC 140), devolvendo URL + expiração + `loudnessDb`.
3. **Pré-resolução:** faltando ~20 s para o fim, resolver a próxima faixa e passá-la ao mpv com
   `loadfile ... append`. É o que dá gapless real.
4. `ytm-player/src/queue.rs`: fila com faixa atual, histórico, shuffle (Fisher-Yates com semente
   estável) e repeat (off/all/one).
5. URL expirada: ao detectar erro de rede do mpv, re-resolver e retomar da mesma posição, sem que o
   usuário perceba.

### Fase 4 — UI completa
1. Layout: barra lateral (Início, Buscar, Biblioteca, Playlists), área central com pilha de
   navegação (voltar/avançar), `PlayerBar` fixa no rodapé.
2. `views/Library`, `Playlist`, `Album`, `Artist` — todas virtualizadas e lendo do cache primeiro.
3. `PlayerBar`: capa, título/artista clicáveis, seek com scrubbing, volume, shuffle/repeat, painel
   de fila com reordenação por drag & drop (`draggable` nativo do HTML).
4. `TrackRow` com menu de contexto: tocar em seguida, adicionar à fila, ir para álbum/artista,
   curtir.
5. Estados vazios, skeletons e erros inline — nunca um spinner cobrindo a tela.
6. Atalhos: espaço, setas, `Ctrl+F`, `Ctrl+L`.

### Fase 5 — Empacotamento
1. Bundler do próprio Tauri: `.deb`, `.rpm` e AppImage no Linux; MSI e NSIS no Windows.
2. **libmpv no Windows:** empacotar `libmpv-2.dll` (build do shinchiro) como recurso. **No Linux:**
   declarar dependência do `libmpv` do sistema em `.deb`/`.rpm`, e embutir a `.so` no AppImage.
3. **Mitigação de gráficos no Linux** (`packaging/ytmc.sh`, usado no `.desktop`): detectar driver
   NVIDIA e, só nesse caso, exportar `__NV_DISABLE_EXPLICIT_SYNC=1` e, como último recurso,
   `WEBKIT_DISABLE_DMABUF_RENDERER=1`. Sem essa detecção, uma parte dos usuários NVIDIA abre o app
   e vê uma janela em branco. Em drivers NVIDIA ≥ 560 o workaround deve ser pulado, porque degrada
   a performance.
4. `.desktop`, ícones e metainfo AppStream.
5. CI no GitHub Actions: matriz `ubuntu-latest` + `windows-latest`, com `cargo clippy -- -D warnings`,
   `cargo test`, `svelte-check` e build dos pacotes nas tags.

---

## Alvos de performance (critério de aceite, não aspiração)

| Métrica | Alvo | Medido na Fase 0 |
|---|---|---|
| Janela visível a partir do clique | < 1 s | a instrumentar |
| Rolagem de listas e grades | 60 fps sustentados | a medir sob carga |
| Clique em "tocar" → primeiro som (cache quente) | < 800 ms | — |
| Memória (RSS da árvore de processos) | **< 500 MB** | 390 MB (tela vazia) |
| Tamanho do instalador | < 30 MB sem contar a libmpv | binário: 5,3 MB |

O alvo de memória é aferido em **RSS**, que é o número que aparece no monitor
do sistema. O overlay mostra o **PSS** ao lado para diagnóstico: PSS divide
cada página compartilhada entre os processos que a usam, e a diferença entre os
dois (390 contra 211 MB na Fase 0) é exatamente o que os três processos —
`ytmc`, `WebKitWebProcess`, `WebKitNetworkProcess` — compartilham de GTK, libc
e do próprio WebKit.

Vale saber de onde vem o piso, porque ele não é código nosso: numa medição do
build de release com a tela vazia, o binário do app respondia por 3,6 MB do
total. O resto é WebKitGTK, GTK e a stack gráfica do Mesa. O que cada fase
acrescenta sobre esse piso é a parte que de fato controlamos.
---

## Fora do v1 (roadmap)

1. **Rádio/mix e continuação automática** — endpoint `next` do InnerTube alimentando a fila.
2. **Integração com o SO** — crate `souvlaki` cobre MPRIS (Linux) e SMTC (Windows) numa API só;
   mais bandeja e teclas de mídia globais.
3. **Letras sincronizadas** — LRCLIB como fonte primária, timings do YTM como complemento.
4. **Last.fm scrobbling e Discord Rich Presence.**
5. **Download offline** e arquivos locais (o libmpv já toca ambos de graça).
6. **Equalizador** via filtros de áudio do mpv.

---

## Riscos e mitigações

| Risco | Mitigação |
|---|---|
| API InnerTube muda e o app para de tocar | `MetadataSource`/`StreamResolver` atrás de traits; teste de fumaça diário no CI que detecta a quebra antes do usuário |
| Google passa a exigir PO token no cliente YTM | `PoTokenProvider` já previsto; plano B é o sidecar `rustypipe-botguard` |
| Cookies rotacionados/expirados | Detecção de 401/403 + banner de reconexão não modal; sessão por conta no keyring |
| Janela em branco no Linux com NVIDIA (DMABUF do WebKitGTK) | Wrapper de lançamento com detecção de driver (Fase 5.3); documentar as variáveis no README |
| WebKitGTK mais lento que o Chromium | Svelte sem VDOM, capas fora do IPC, listas virtualizadas, comandos paginados — as três regras da seção de performance |
| libmpv como dependência nativa | DLL empacotada no Windows, `.so` do sistema com fallback no AppImage; a trait `Player` permite um backend `symphonia`+`cpal` no futuro (limitado a AAC até o Opus ficar pronto) |
| Zona cinzenta de ToS | Explícito no README; sem distribuição em lojas oficiais; sem monetização |

---

## Verificação

**Testes automatizados**
- `ytm-core`: parsing contra fixtures JSON gravadas (respostas reais do InnerTube em
  `tests/fixtures/`), para a suíte rodar offline e determinística.
- `ytm-player`: fila (shuffle, repeat, histórico) sem tocar no libmpv, via trait mockada.
- `cargo clippy --all-targets -- -D warnings`, `cargo fmt --check`, `svelte-check` no CI.

**Testes de integração (`#[ignore]`, sob demanda e num cron diário)**
- `resolve_and_play`: resolve um vídeo conhecido, toca 5 segundos pelo libmpv, verifica que a
  posição avançou. É o canário que avisa quando o YouTube muda alguma coisa.

**Verificação manual, por fase**
- Fase 1: buscar "Radiohead" e rolar 500 resultados com o overlay F3 — tem que segurar 60 fps.
  Confirmar no DevTools (Network) que as capas vêm por `ytmart://` e não por IPC.
- Fase 2: entrar com a conta, reiniciar o app, confirmar que a sessão persiste no keyring.
- Fase 3: tocar um álbum inteiro e confirmar que não há silêncio entre as faixas; derrubar a rede
  (`nmcli`) e confirmar a recuperação.
- Fase 4: percorrer biblioteca → playlist → álbum → artista com o overlay ligado; voltar e
  confirmar que veio do cache (instantâneo).
- Fase 5: instalar o `.rpm` numa VM Fedora limpa e o MSI numa VM Windows limpa; cronometrar o
  startup a frio. Se houver acesso a uma máquina NVIDIA, validar o wrapper de lançamento nela — é o
  cenário de falha mais provável no Linux.

## Referências consultadas

- [rustypipe](https://codeberg.org/ThetaDev/rustypipe) e [notas sobre PO token](https://code.thetadev.de/ThetaDev/rustypipe/src/branch/main/notes/po_token.md)
- [ytmapi-rs](https://crates.io/crates/ytmapi-rs)
- [libmpv2](https://crates.io/crates/libmpv2)
- [Symphonia](https://github.com/pdeljanov/Symphonia) — status do decodificador Opus
- [Tauri — Linux Graphics Issues](https://v2.tauri.app/develop/debug/linux-graphics/)
- [wry `WebView::cookies_for_url`](https://docs.rs/wry/latest/wry/struct.WebView.html)
- [rookie](https://github.com/thewh1teagle/rookie)
- [limusic](https://github.com/SimoHypers/limusic) — cliente YTM em Tauri, referência de features
