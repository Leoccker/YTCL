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
pública documentada. Usá-la contraria os Termos de Serviço do YouTube, e a API muda sem aviso — a
decifragem de assinatura e os identificadores de cliente quebram algumas vezes por ano. O projeto
exige manutenção contínua. Todo o plano isola essa fragilidade numa única camada trocável.

**Condições de publicação.** O repositório é público (`github.com/Leoccker/YTCL`). O que separa o
perfil de risco de "um binário na minha máquina" do de "um serviço público" está aqui:

- **Nome sem marca.** O projeto se chama YTCL, não "YouTube Music Client". "YouTube" e "YouTube
  Music" aparecem só como descrição da finalidade, nunca como nome do produto ou no logo. Isso é a
  diferença entre uso nominativo justo e violação de trademark — o gancho mais concreto que o
  Google tem.
- **Disclaimer de não afiliação** no topo do README e visível no app.
- **A comunicação não lidera com "sem anúncios".** Contornar anúncio é o que o Google defende
  comercialmente; é o que atrai carta de advogado. O pitch é "cliente nativo e leve".
- **Sem monetização.** Doação é tolerada pela prática da comunidade; venda não. O projeto não faz
  nem uma coisa nem outra.
- **Sem instância pública, sem serviço de download.** Distribui código-fonte e, na Fase 5,
  binários — como ferramenta, não como serviço.
- **Se vier DMCA:** o precedente é o `youtube-dl` (contestado, revertido em ~2 semanas). Plano B é
  mover para Codeberg ou self-host. Perde-se o histórico de estrelas, não o código.

---

## Estado

| Fase | Estado |
|---|---|
| 0 — Fundação | **concluída** (2026-09-09) |
| 1 — Metadados e busca | **concluída** (2026-09-09) |
| 2 — Autenticação | **concluída** (2026-09-10) |
| 3 — Reprodução | próxima |
| 4 — UI completa | — |
| 5 — Empacotamento | — |

**Fase 0** entregou o workspace Cargo com `ytcl-core` e `ytcl-player`, o crate
Tauri com o protocolo `ytmart://`, o frontend Svelte 5 + Vite, a config em TOML
e o overlay de diagnóstico (F3).

**Fase 1** entregou o adaptador InnerTube, o cache SQLite com
stale-while-revalidate, o download de capas sob demanda e a tela de busca com
lista virtualizada.

**Fase 2** entregou o cofre de contas (`ytcl-core/src/auth.rs`, cookie no
keyring do SO), a janela de login do Google (`src-tauri/src/login_window.rs`), a
re-hidratação da sessão no boot, os comandos de biblioteca e a tela de
biblioteca com abas (playlists, álbuns, artistas, curtidas) mais o banner de
reconexão.

### Correções de rota

Registradas aqui porque contradizem o que as seções abaixo diziam antes:

- **O medidor de memória somava RSS da árvore de processos**, contando as
  bibliotecas compartilhadas uma vez por processo. Passou a medir PSS, com RSS
  ao lado. *(Fase 0)*
- **O alvo de memória de 200 MB era estimativa por analogia, não medição.** Com
  o piso do WebKitGTK medido nesta máquina, passou para 500 MB de RSS. *(Fase 0)*
- **`ytmapi-rs` foi eliminado; o `rustypipe` faz tudo.** O plano previa dividir
  metadados e stream entre as duas crates, partindo da premissa de que o
  ytmapi-rs tinha endpoints de YTM mais ricos. A premissa estava errada: o
  rustypipe cobre busca, álbum, artista, playlist, biblioteca, rádio, letras,
  autenticação e resolução de stream. Além de simplificar, isso resolveu um
  conflito real — as duas crates pediam versões incompatíveis do `reqwest`
  (0.13 contra 0.12), o que faria o cargo compilar duas pilhas HTTP inteiras.
  *(Fase 1)*
- **O handler do `ytmart://` virou assíncrono e baixa a capa sob demanda.** O
  plano previa uma fila de download em `artwork.rs` que o frontend acionaria.
  Como o handler já recebe o hash, deixá-lo resolver a URL pelo cache e baixar
  na hora tira o frontend da equação: ele só aponta um `<img>` e o webview
  espera como esperaria qualquer imagem da rede. *(Fase 1)*
- **O `rookie` (importar cookies do navegador) saiu.** O plano listava como
  caminho alternativo de login. Ele depende de `rusqlite 0.31`, que fixa
  `libsqlite3-sys 0.28`, e o nosso cache usa `rusqlite 0.40` / `libsqlite3-sys
  0.38` — dois crates linkando `sqlite3`, o que o cargo recusa. O fallback sem
  webview passou a ser **colar o cookie manualmente** (zero dependências, cobre
  o mesmo caso: quem não quer a janela embutida). *(Fase 2)*
- **`ScrubbedStorage` no lugar de um `CacheStorage` custom no keyring.** O plano
  falava em gravar os cookies no keyring; a parte nova é que o rustypipe insiste
  em persistir `auth_cookie`/`oauth_token` no próprio cache JSON. A solução é um
  `CacheStorage` que remove esses dois campos antes de gravar — o resto do cache
  (versões de cliente, JS de decifragem) continua em disco, e a sessão é
  re-injetada do keyring a cada abertura. *(Fase 2)*
- **Stale-while-revalidate sem eventos.** Em vez de emitir um evento quando a
  revalidação termina, os comandos devolvem `{ data, stale }` e o frontend
  repete a chamada com `refresh: true`. Menos peças móveis, mesmo efeito.
  *(Fase 1)*

---

## Problemas conhecidos

- **rustypipe patchado por um fork.** O `rustypipe 0.11.4` falha ao ler o
  `gridContinuation` vazio que o YouTube manda no fim do feed de playlists da
  biblioteca (`FEmusic_liked_playlists`): o campo `items` vem ausente e o struct
  `GridRenderer` o exige, gerando `missing field items`. O `[patch.crates-io]`
  na raiz aponta para `codeberg.org/Leoccker/rustypipe`, que adiciona
  `#[serde(default)]` a esse campo — uma linha, a mesma coisa que o campo
  `continuations` logo abaixo já tem. Verificado: as quatro rotas de biblioteca
  passam (`cargo run -p ytcl-core --example library`), teste de regressão em
  `innertube_live.rs::library_playlists_pagina_ate_o_fim_sem_quebrar`.
  **Remover o patch quando o upstream publicar a correção.**

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
 │  ytcl-core (tokio)    │   │  ytcl-player                  │
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

**Dados — `rustypipe`, sozinho.** Um cliente InnerTube em Rust que cobre todo o
escopo do projeto: `music_search` e variantes por filtro, `music_album`,
`music_artist`, `music_playlist`, a biblioteca do usuário
(`music_saved_*`, `music_liked_tracks`, `music_history`), `music_radio` e
`music_lyrics` do roadmap, além da parte difícil — decifragem de assinatura e
do parâmetro `n`, versionamento dos clientes InnerTube, cache do JS do player e
`get_po_token`.

O plano original dividia isso entre `ytmapi-rs` (metadados) e `rustypipe`
(stream). A divisão foi descartada na Fase 1: o rustypipe faz os dois, e manter
as duas crates traria uma duplicação real da pilha HTTP, porque pedem versões
incompatíveis do `reqwest`.

Ele fica atrás das traits `MetadataSource` e `StreamResolver` em `ytcl-core`
(`crates/ytcl-core/src/innertube.rs` é o único arquivo que conhece seus tipos):
uma quebra da API interna do YouTube tem um lugar só para ser consertada.

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
> experimental no Linux. Para áudio, o crate `libmpv2` direto em `ytcl-player`, sem saída de vídeo,
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
e resultados de busca, com TTL por tipo. Capas ficam em `~/.cache/ytcl/art/` nomeadas pelo hash da
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
  ytcl-core/
    src/{lib,model,auth,metadata,stream,cache,artwork,config}.rs
  ytcl-player/
    src/{lib,mpv,queue,state}.rs
packaging/                # ícones, .desktop, wrapper de lançamento no Linux
```

---

## Plano de execução

### Fase 0 — Fundação
1. `cargo create-tauri-app` com template Svelte + TS; converter em workspace com `crates/`.
2. Runtime tokio gerenciado como estado do Tauri (`app.manage(...)`); todo comando é `async`.
3. `ytcl-core/src/config.rs`: diretórios via `directories-next` (`~/.config/ytcl`,
   `~/.cache/ytcl`), config em TOML.
4. `src-tauri/src/protocol.rs`: registrar `ytmart://` já nesta fase — servindo um placeholder — pra
   que o padrão esteja estabelecido antes de qualquer tela existir.
5. Tema em `src/lib/theme.css` com variáveis CSS; sem biblioteca de componentes.
6. **Sanity check de performance desde já:** um overlay de debug (F3) com FPS do frontend
   (`requestAnimationFrame`) e RSS do processo. Se a rolagem cair de 60 fps em qualquer fase
   seguinte, resolve-se antes de continuar.

### Fase 1 — Metadados e busca
1. Trait `MetadataSource` em `ytcl-core/src/metadata.rs` (`search`, `get_album`, `get_artist`,
   `get_playlist`, `get_library`, `get_liked`) sobre modelos próprios em `model.rs` — não expor os
   tipos do `ytmapi-rs`, para desacoplar.
2. Implementar `YtmApiSource` com `ytmapi-rs` sem autenticação primeiro (busca pública já funciona
   sem login, o que permite ver a UI viva cedo).
3. `ytcl-core/src/cache.rs`: schema SQLite + migrações, `get_or_fetch` com TTL e revalidação em
   background (stale-while-revalidate).
4. `ytcl-core/src/artwork.rs`: fila de download com `tokio::Semaphore`, gravação em disco, e o
   `ytmart://` servindo com `Cache-Control` longo. O frontend só usa `<img src="ytmart://hash">`.
5. `views/Search.svelte` + `components/VirtualList.svelte`: virtualização por janela de rolagem
   (renderizar só as linhas visíveis + margem). Comandos paginados.

### Fase 2 — Autenticação
1. `src-tauri/src/login_window.rs`: `WebviewWindowBuilder` com URL de login do Google e UA de
   desktop; listener de navegação; ao chegar em `music.youtube.com`, `cookies_for_url` e fechar.
2. `ytcl-core/src/auth.rs`: validar os cookies com uma chamada autenticada de teste antes de aceitar;
   gravar no keyring com `keyring-rs`, indexado por `account_id`.
3. Caminho alternativo `import_from_browser()` com `rookie`, filtrando `.youtube.com`.
4. Renovação: detectar 401/403 nas chamadas, marcar a sessão como expirada e mostrar um banner não
   modal de reconexão — sem interromper o que está tocando.
5. Múltiplas contas com seletor no menu desde já: retrofitar isso depois é caro.

### Fase 3 — Reprodução
1. `ytcl-player/src/mpv.rs`: libmpv com `vid=no`, `audio-display=no`, `cache=yes`, `cache-secs=120`,
   `demuxer-max-bytes=64MiB`, `replaygain=track`, `gapless-audio=yes`. Loop de eventos do mpv numa
   thread dedicada, traduzido para `PlayerEvent` e emitido ao frontend (posição a ~4 Hz).
2. `ytcl-core/src/stream.rs`: trait `StreamResolver`; implementação com `rustypipe` escolhendo a
   melhor trilha só de áudio (Opus 251 → AAC 140), devolvendo URL + expiração + `loudnessDb`.
3. **Pré-resolução:** faltando ~20 s para o fim, resolver a próxima faixa e passá-la ao mpv com
   `loadfile ... append`. É o que dá gapless real.
4. `ytcl-player/src/queue.rs`: fila com faixa atual, histórico, shuffle (Fisher-Yates com semente
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
3. **Mitigação de gráficos no Linux** (`packaging/ytcl.sh`, usado no `.desktop`): detectar driver
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
`ytcl`, `WebKitWebProcess`, `WebKitNetworkProcess` — compartilham de GTK, libc
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
| Violação de ToS / trademark do YouTube | Nome sem marca (YTCL), disclaimer de não afiliação, sem monetização, sem instância pública; ver "Condições de publicação" no Context |

---

## Verificação

**Testes automatizados**
- `ytcl-core`: parsing contra fixtures JSON gravadas (respostas reais do InnerTube em
  `tests/fixtures/`), para a suíte rodar offline e determinística.
- `ytcl-player`: fila (shuffle, repeat, histórico) sem tocar no libmpv, via trait mockada.
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
