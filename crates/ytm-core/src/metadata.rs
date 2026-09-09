//! A fronteira entre o app e a API interna do YouTube.
//!
//! Tudo que o resto do app sabe sobre "de onde vem o metadado" esta nesta
//! trait. Quando o InnerTube mudar — e ele vai — a correcao acontece numa
//! implementacao, nao espalhada pelo codigo.

use async_trait::async_trait;

use crate::error::Result;
use crate::model::{Album, Artist, Page, Playlist, SearchFilter, SearchItem, Track};

#[async_trait]
pub trait MetadataSource: Send + Sync + 'static {
    async fn search(&self, query: &str, filter: SearchFilter) -> Result<Page<SearchItem>>;

    /// Continua uma busca ou listagem a partir do token opaco devolvido antes.
    async fn search_more(&self, continuation: &str) -> Result<Page<SearchItem>>;

    async fn album(&self, id: &str) -> Result<(Album, Vec<Track>)>;

    async fn artist(&self, id: &str) -> Result<(Artist, Vec<Track>, Vec<Album>)>;

    async fn playlist(&self, id: &str) -> Result<Playlist>;

    /// Paginado de proposito: uma playlist pode ter milhares de faixas, e
    /// serializar isso de uma vez trava os dois lados do IPC.
    async fn playlist_tracks(
        &self,
        id: &str,
        continuation: Option<&str>,
    ) -> Result<Page<Track>>;

    // --- daqui para baixo, exige sessao autenticada ---

    async fn library_playlists(&self) -> Result<Vec<Playlist>>;

    async fn library_albums(&self) -> Result<Vec<Album>>;

    async fn library_artists(&self) -> Result<Vec<Artist>>;

    async fn liked_songs(&self, continuation: Option<&str>) -> Result<Page<Track>>;
}
