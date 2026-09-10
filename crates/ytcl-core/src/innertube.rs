//! Adaptador InnerTube sobre o `rustypipe`.
//!
//! Este e o unico arquivo do projeto que conhece os tipos do rustypipe. Tudo
//! que sai daqui ja esta nos modelos de `crate::model`, para que uma quebra da
//! API interna do YouTube — que acontece algumas vezes por ano — tenha um
//! lugar so para ser consertada.

use std::path::{Path, PathBuf};

use async_trait::async_trait;
use rustypipe::cache::CacheStorage;
use rustypipe::client::{RustyPipe, RustyPipeQuery};
use rustypipe::model::paginator::Paginator;
use rustypipe::model::{
    AlbumItem, ArtistItem, MusicItem, MusicPlaylistItem, Thumbnail, TrackItem,
};
use serde::de::DeserializeOwned;
use serde::Serialize;

use crate::error::{CoreError, Result};
use crate::metadata::MetadataSource;
use crate::stream::{
    AudioCodec as StreamCodec, AudioTrack, ResolvedStream, StreamResolver,
};
use crate::model::{
    Album, AlbumRef, ArtRef, Artist, ArtistRef, Page, Playlist, SearchFilter, SearchItem, Track,
};

/// Tamanho de capa para linhas de lista (48px logicos em telas 2x, com folga).
const ART_ROW: u32 = 128;
/// Tamanho de capa para cards de grade e telas de detalhe.
const ART_CARD: u32 = 320;
/// Teto ao puxar as listas da biblioteca de uma vez. Sem isso, uma conta com
/// milhares de itens deixaria o primeiro carregamento pendurado.
const LIBRARY_LIMIT: usize = 1000;

/// Quantos itens de cada tipo a busca "tudo" mostra antes de o usuario
/// escolher um filtro.
const ALL_ARTISTS: usize = 3;
const ALL_TRACKS: usize = 12;
const ALL_ALBUMS: usize = 8;

pub struct InnerTube {
    rp: RustyPipe,
}

impl InnerTube {
    /// O cache do rustypipe (versoes dos clientes InnerTube, JS de decifragem)
    /// fica em `<cache_dir>/rustypipe_cache.json`, atras de um storage que
    /// remove os segredos antes de gravar — ver [`ScrubbedStorage`].
    pub fn new(cache_dir: &Path) -> Result<Self> {
        let storage = ScrubbedStorage::new(cache_dir.join("rustypipe_cache.json"));
        let rp = RustyPipe::builder()
            .storage(Box::new(storage))
            .build()
            .map_err(|e| CoreError::Other(format!("iniciando o cliente InnerTube: {e}")))?;
        Ok(Self { rp })
    }

    /// Carrega o cookie de sessao de uma conta.
    ///
    /// Isto tambem **valida**: o rustypipe busca o youtube.com com o cookie
    /// para extrair os cabecalhos de sessao, e falha se o cookie nao presta.
    /// Um erro aqui vira `SessionExpired`, que a UI trata com o banner de
    /// reconexao.
    pub async fn set_cookie(&self, cookie: &str) -> Result<()> {
        self.rp
            .user_auth_set_cookie(cookie.to_string())
            .await
            .map_err(|e| match map_err(e) {
                // Qualquer falha ao aplicar um cookie e, na pratica, "esse
                // cookie nao serve" — seja expirado, incompleto ou rejeitado.
                CoreError::SessionExpired | CoreError::Other(_) | CoreError::Parse(_) => {
                    CoreError::SessionExpired
                }
                other => other,
            })
    }

    /// Esquece o cookie em memoria. Nao mexe no cofre — quem chama decide se
    /// tambem remove a conta.
    pub async fn clear_cookie(&self) {
        let _ = self.rp.user_auth_remove_cookie().await;
    }

    fn query(&self) -> RustyPipeQuery {
        self.rp.query()
    }

    /// Busca "tudo": artistas, musicas e albuns em paralelo.
    ///
    /// Nao usamos `music_search_main`. Ele mapeia a pagina de "melhores
    /// resultados" do YouTube Music, que e rasa de proposito: devolve ~4 itens
    /// sem duracao, e o campo de artista traz o rotulo da categoria ("Song")
    /// no lugar do nome. Tres buscas filtradas em paralelo custam o mesmo
    /// tempo de parede e devolvem dados completos.
    async fn search_all(&self, query: &str) -> Result<Page<SearchItem>> {
        let q = self.query();

        let (artistas, musicas, albuns) = tokio::join!(
            q.music_search_artists(query),
            q.music_search_tracks(query),
            q.music_search_albums(query),
        );

        let mut items = Vec::new();
        let mut algum_erro = None;

        // Uma secao que falha nao pode zerar a busca inteira: aproveitamos o
        // que veio e so propagamos o erro se nada tiver vindo.
        match artistas {
            Ok(r) => items.extend(
                r.items.items.into_iter().take(ALL_ARTISTS).map(|a| SearchItem::Artist(artist_from(a))),
            ),
            Err(e) => algum_erro = Some(map_err(e)),
        }
        match musicas {
            Ok(r) => items.extend(
                r.items.items.into_iter().take(ALL_TRACKS).map(|t| SearchItem::Track(track_from(t))),
            ),
            Err(e) => algum_erro = Some(map_err(e)),
        }
        match albuns {
            Ok(r) => items.extend(
                r.items.items.into_iter().take(ALL_ALBUMS).map(|a| SearchItem::Album(album_from(a))),
            ),
            Err(e) => algum_erro = Some(map_err(e)),
        }

        if items.is_empty() {
            if let Some(e) = algum_erro {
                return Err(e);
            }
        }

        // Sem continuacao: para ver mais de um tipo, o usuario escolhe o
        // filtro correspondente — que e como o proprio YTM se comporta.
        Ok(Page::new(items))
    }
}

/// Traduz o erro do rustypipe para o nosso, preservando a distincao que a UI
/// precisa fazer: sessao expirada mostra banner de reconexao, parse quebrado
/// e bug nosso (ou mudanca do YouTube), rede e transitorio.
fn map_err(e: rustypipe::error::Error) -> CoreError {
    use rustypipe::error::Error;
    match e {
        Error::Http(inner) => CoreError::Network(inner.to_string()),
        Error::Extraction(inner) => CoreError::Parse(inner.to_string()),
        other => {
            let s = other.to_string();
            // O rustypipe nao tem variante propria para "nao logado"; a
            // mensagem e o unico sinal disponivel.
            if s.contains("not logged in") || s.contains("authentication") {
                CoreError::SessionExpired
            } else {
                CoreError::Other(s)
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Cache do rustypipe sem os segredos
// ---------------------------------------------------------------------------

/// `CacheStorage` que persiste tudo que o rustypipe cacheia **menos** o
/// cookie de sessao e o token OAuth.
///
/// Por que: o rustypipe grava `auth_cookie` e `oauth_token` no mesmo JSON das
/// versoes de cliente e do JS de decifragem. Esses dois campos sao segredos e
/// nao podem cair num arquivo em texto — eles vivem no cofre do SO (ver
/// [`crate::auth`]). Os outros campos sao caros de refazer e nao tem nada de
/// sensivel, entao continuam em disco.
///
/// Consequencia: a cada abertura o rustypipe comeca deslogado, e o app
/// re-hidrata a sessao chamando [`InnerTube::set_cookie`] com o cookie do
/// cofre.
struct ScrubbedStorage {
    path: PathBuf,
}

impl ScrubbedStorage {
    fn new(path: PathBuf) -> Self {
        Self { path }
    }
}

impl CacheStorage for ScrubbedStorage {
    fn write(&self, data: &str) {
        let scrubbed = match serde_json::from_str::<serde_json::Value>(data) {
            Ok(mut v) => {
                if let Some(obj) = v.as_object_mut() {
                    obj.remove("auth_cookie");
                    obj.remove("oauth_token");
                }
                serde_json::to_string(&v).unwrap_or_else(|_| data.to_string())
            }
            // Se nao for JSON valido, melhor gravar como veio do que perder o
            // cache inteiro.
            Err(_) => data.to_string(),
        };

        if let Some(dir) = self.path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Err(e) = std::fs::write(&self.path, &scrubbed) {
            tracing::warn!("nao consegui gravar o cache do rustypipe: {e}");
            return;
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&self.path, std::fs::Permissions::from_mode(0o600));
        }
    }

    fn read(&self) -> Option<String> {
        std::fs::read_to_string(&self.path).ok()
    }
}

// ---------------------------------------------------------------------------
// Conversao de tipos
// ---------------------------------------------------------------------------

/// Escolhe uma capa e ja a normaliza para o tamanho que vamos exibir.
///
/// As URLs de thumbnail do YouTube carregam a dimensao no proprio caminho, e
/// reescreve-la evita baixar 544x544 para desenhar num quadrado de 48px.
fn pick_art(thumbs: &[Thumbnail], target: u32) -> Option<ArtRef> {
    let base = thumbs.iter().max_by_key(|t| t.width * t.height)?;
    let url = crate::artwork::sized_url(&base.url, target);
    Some(ArtRef {
        hash: crate::artwork::hash_url(&url),
        url,
        width: target,
        height: target,
    })
}

fn artists_of(list: &[rustypipe::model::ArtistId]) -> Vec<ArtistRef> {
    list.iter()
        .map(|a| ArtistRef { id: a.id.clone(), name: a.name.clone() })
        .collect()
}

fn track_from(t: TrackItem) -> Track {
    Track {
        art: pick_art(&t.cover, ART_ROW),
        artists: artists_of(&t.artists),
        album: t.album.map(|a| AlbumRef { id: Some(a.id), title: a.name }),
        duration_secs: t.duration,
        id: t.id,
        title: t.name,
        // O YouTube Music nao marca explicito nos itens de lista; so na
        // pagina do album. Fica falso ate termos a informacao de verdade.
        is_explicit: false,
        set_video_id: None,
    }
}

fn album_from(a: AlbumItem) -> Album {
    Album {
        art: pick_art(&a.cover, ART_CARD),
        artists: artists_of(&a.artists),
        year: a.year.map(u32::from),
        track_count: None,
        id: a.id,
        title: a.name,
    }
}

fn artist_from(a: ArtistItem) -> Artist {
    Artist {
        art: pick_art(&a.avatar, ART_CARD),
        subscribers: a.subscriber_count.map(format_count),
        id: a.id,
        name: a.name,
    }
}

fn playlist_from(p: MusicPlaylistItem) -> Playlist {
    Playlist {
        art: pick_art(&p.thumbnail, ART_CARD),
        author: p.channel.map(|c| c.name),
        track_count: p.track_count.map(|n| n as u32),
        id: p.id,
        title: p.name,
    }
}

/// 1.234.567 -> "1,2 mi". O YouTube devolve o numero cru; formatar aqui evita
/// espalhar logica de apresentacao pelo frontend.
fn format_count(n: u64) -> String {
    match n {
        0..=999 => n.to_string(),
        1_000..=999_999 => format!("{:.1} mil", n as f64 / 1e3),
        1_000_000..=999_999_999 => format!("{:.1} mi", n as f64 / 1e6),
        _ => format!("{:.1} bi", n as f64 / 1e9),
    }
}

fn search_item_from(item: MusicItem) -> Option<SearchItem> {
    Some(match item {
        MusicItem::Track(t) => SearchItem::Track(track_from(t)),
        MusicItem::Album(a) => SearchItem::Album(album_from(a)),
        MusicItem::Artist(a) => SearchItem::Artist(artist_from(a)),
        MusicItem::Playlist(p) => SearchItem::Playlist(playlist_from(p)),
        // Perfis de usuario nao tem lugar num cliente de musica.
        MusicItem::User(_) => return None,
    })
}

// ---------------------------------------------------------------------------
// Cursor de paginacao
// ---------------------------------------------------------------------------

/// O `Paginator` do rustypipe carrega tudo que a proxima pagina precisa
/// (ctoken, visitor_data, endpoint, flag de autenticacao). Serializamos ele
/// sem os itens e usamos como token opaco — o frontend so devolve a string.
///
/// `Paginator` e `#[non_exhaustive]`, entao nao da para construi-lo por
/// literal; ir e voltar por serde e o caminho que a propria crate suporta.
fn cursor_of<T: Serialize>(p: &Paginator<T>) -> Option<String> {
    p.ctoken.as_ref()?;
    let mut v = serde_json::to_value(p).ok()?;
    // Os itens ja foram entregues; carregar copia deles no token seria
    // desperdicio de banda no IPC e no cache.
    if let Some(obj) = v.as_object_mut() {
        obj.insert("items".into(), serde_json::Value::Array(Vec::new()));
    }
    serde_json::to_string(&v).ok()
}

fn paginator_from_cursor<T: DeserializeOwned>(cursor: &str) -> Result<Paginator<T>> {
    serde_json::from_str(cursor)
        .map_err(|e| CoreError::Other(format!("token de continuacao invalido: {e}")))
}

/// Busca a proxima pagina a partir de um cursor e devolve so os itens novos.
async fn next_page<T>(query: &RustyPipeQuery, cursor: &str) -> Result<(Vec<T>, Option<String>)>
where
    T: Serialize + DeserializeOwned + rustypipe::model::traits::FromYtItem + Clone,
{
    let mut p: Paginator<T> = paginator_from_cursor(cursor)?;
    p.extend(query).await.map_err(map_err)?;
    let next = cursor_of(&p);
    Ok((p.items, next))
}

// ---------------------------------------------------------------------------

#[async_trait]
impl MetadataSource for InnerTube {
    async fn search(&self, query: &str, filter: SearchFilter) -> Result<Page<SearchItem>> {
        let q = self.query();

        // Cada filtro devolve um tipo concreto diferente, entao a conversao
        // para SearchItem acontece por braco.
        Ok(match filter {
            SearchFilter::All => return self.search_all(query).await,
            SearchFilter::Songs => {
                let r = q.music_search_tracks(query).await.map_err(map_err)?;
                Page {
                    continuation: cursor_of(&r.items),
                    items: r.items.items.into_iter().map(|t| SearchItem::Track(track_from(t))).collect(),
                }
            }
            SearchFilter::Albums => {
                let r = q.music_search_albums(query).await.map_err(map_err)?;
                Page {
                    continuation: cursor_of(&r.items),
                    items: r.items.items.into_iter().map(|a| SearchItem::Album(album_from(a))).collect(),
                }
            }
            SearchFilter::Artists => {
                let r = q.music_search_artists(query).await.map_err(map_err)?;
                Page {
                    continuation: cursor_of(&r.items),
                    items: r.items.items.into_iter().map(|a| SearchItem::Artist(artist_from(a))).collect(),
                }
            }
            SearchFilter::Playlists => {
                // `community = true`: playlists feitas por usuarios. As curadas pelo
                // proprio YTM ja aparecem na aba "tudo".
                let r = q.music_search_playlists(query, true).await.map_err(map_err)?;
                Page {
                    continuation: cursor_of(&r.items),
                    items: r.items.items.into_iter().map(|p| SearchItem::Playlist(playlist_from(p))).collect(),
                }
            }
        })
    }

    async fn search_more(&self, continuation: &str) -> Result<Page<SearchItem>> {
        let q = self.query();
        // O cursor guarda o tipo implicitamente: a busca "tudo" pagina
        // MusicItem, as filtradas paginam o tipo do filtro. Tentamos MusicItem
        // primeiro porque e o caso do filtro padrao.
        if let Ok((items, next)) = next_page::<MusicItem>(&q, continuation).await {
            return Ok(Page {
                items: items.into_iter().filter_map(search_item_from).collect(),
                continuation: next,
            });
        }
        let (items, next) = next_page::<TrackItem>(&q, continuation).await?;
        Ok(Page {
            items: items.into_iter().map(|t| SearchItem::Track(track_from(t))).collect(),
            continuation: next,
        })
    }

    async fn album(&self, id: &str) -> Result<(Album, Vec<Track>)> {
        let a = self.query().music_album(id).await.map_err(map_err)?;
        let album = Album {
            art: pick_art(&a.cover, ART_CARD),
            artists: artists_of(&a.artists),
            year: a.year.map(u32::from),
            track_count: Some(u32::from(a.track_count)),
            id: a.id,
            title: a.name,
        };
        Ok((album, a.tracks.into_iter().map(track_from).collect()))
    }

    async fn artist(&self, id: &str) -> Result<(Artist, Vec<Track>, Vec<Album>)> {
        let a = self.query().music_artist(id, false).await.map_err(map_err)?;
        let artist = Artist {
            art: pick_art(&a.header_image, ART_CARD),
            subscribers: a.subscriber_count.map(format_count),
            id: a.id,
            name: a.name,
        };
        Ok((
            artist,
            a.tracks.into_iter().map(track_from).collect(),
            a.albums.into_iter().map(album_from).collect(),
        ))
    }

    async fn playlist(&self, id: &str) -> Result<Playlist> {
        let p = self.query().music_playlist(id).await.map_err(map_err)?;
        Ok(Playlist {
            art: pick_art(&p.thumbnail, ART_CARD),
            author: p.channel.map(|c| c.name),
            track_count: p.track_count.map(|n| n as u32),
            id: p.id,
            title: p.name,
        })
    }

    async fn playlist_tracks(&self, id: &str, continuation: Option<&str>) -> Result<Page<Track>> {
        let q = self.query();

        // Sem cursor e a primeira pagina, que vem junto com os metadados da
        // playlist; com cursor, e continuacao pura.
        let (items, next) = match continuation {
            Some(c) => next_page::<TrackItem>(&q, c).await?,
            None => {
                let p = q.music_playlist(id).await.map_err(map_err)?;
                let next = cursor_of(&p.tracks);
                (p.tracks.items, next)
            }
        };

        Ok(Page {
            items: items.into_iter().map(track_from).collect(),
            continuation: next,
        })
    }

    async fn library_playlists(&self) -> Result<Vec<Playlist>> {
        let q = self.query();
        let mut p = q.music_saved_playlists().await.map_err(map_err)?;
        p.extend_limit(&q, LIBRARY_LIMIT).await.map_err(map_err)?;
        Ok(p.items.into_iter().map(playlist_from).collect())
    }

    async fn library_albums(&self) -> Result<Vec<Album>> {
        let q = self.query();
        let mut p = q.music_saved_albums().await.map_err(map_err)?;
        p.extend_limit(&q, LIBRARY_LIMIT).await.map_err(map_err)?;
        Ok(p.items.into_iter().map(album_from).collect())
    }

    async fn library_artists(&self) -> Result<Vec<Artist>> {
        let q = self.query();
        let mut p = q.music_saved_artists().await.map_err(map_err)?;
        p.extend_limit(&q, LIBRARY_LIMIT).await.map_err(map_err)?;
        Ok(p.items.into_iter().map(artist_from).collect())
    }

    async fn liked_songs(&self, continuation: Option<&str>) -> Result<Page<Track>> {
        let q = self.query();
        let (items, next) = match continuation {
            Some(c) => next_page::<TrackItem>(&q, c).await?,
            None => {
                let p = q.music_liked_tracks().await.map_err(map_err)?;
                let next = cursor_of(&p.tracks);
                (p.tracks.items, next)
            }
        };
        Ok(Page {
            items: items.into_iter().map(track_from).collect(),
            continuation: next,
        })
    }
}

// ---------------------------------------------------------------------------
// Resolução de stream
// ---------------------------------------------------------------------------

#[async_trait]
impl StreamResolver for InnerTube {
    async fn resolve(&self, video_id: &str) -> Result<ResolvedStream> {
        // Sem botguard, o `player()` do rustypipe usa os clientes iOS e TV,
        // que não exigem PO token. Roda sem autenticação — faixa pública
        // resolve sem cookie.
        let player = self
            .rp
            .query()
            .player(video_id)
            .await
            .map_err(map_err)?;

        let tracks: Vec<AudioTrack> = player
            .audio_streams
            .iter()
            .map(|a| AudioTrack {
                url: a.url.clone(),
                itag: a.itag,
                codec: match a.codec {
                    rustypipe::model::AudioCodec::Opus => StreamCodec::Opus,
                    _ => StreamCodec::Aac,
                },
                bitrate: a.average_bitrate.max(a.bitrate),
                loudness_db: a.loudness_db.map(f64::from),
                has_drm: !a.drm_systems.is_empty(),
            })
            .collect();

        let prefer_opus = true; // a UI passa a preferência via config na camada de cima
        let chosen = crate::stream::pick_track(&tracks, prefer_opus).ok_or_else(|| {
            CoreError::Other("nenhuma trilha de áudio tocável (só DRM?)".into())
        })?;

        Ok(ResolvedStream {
            url: chosen.url.clone(),
            codec: chosen.codec,
            bitrate: chosen.bitrate,
            expires_at: player.valid_until.unix_timestamp().max(0) as u64,
            loudness_db: chosen.loudness_db,
            duration_secs: Some(player.details.duration).filter(|d| *d > 0),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formata_contagem_em_escalas() {
        assert_eq!(format_count(0), "0");
        assert_eq!(format_count(999), "999");
        assert_eq!(format_count(1_500), "1.5 mil");
        assert_eq!(format_count(2_400_000), "2.4 mi");
        assert_eq!(format_count(3_100_000_000), "3.1 bi");
    }

    #[test]
    fn escolhe_a_maior_capa_e_redimensiona() {
        // `Thumbnail` e non_exhaustive; serde e o caminho suportado.
        let thumbs: Vec<Thumbnail> = serde_json::from_str(
            r#"[
              {"url":"https://lh3.googleusercontent.com/a=w60-h60-l90-rj","width":60,"height":60},
              {"url":"https://lh3.googleusercontent.com/a=w544-h544-l90-rj","width":544,"height":544}
            ]"#,
        )
        .expect("fixture valida");
        let art = pick_art(&thumbs, 128).expect("ha capas");
        assert_eq!(art.url, "https://lh3.googleusercontent.com/a=w128-h128-l90-rj");
        assert_eq!(art.width, 128);
        assert!(crate::artwork::is_valid_hash(&art.hash));
    }

    #[test]
    fn sem_capa_nao_inventa_uma() {
        assert!(pick_art(&[], 128).is_none());
    }
}
