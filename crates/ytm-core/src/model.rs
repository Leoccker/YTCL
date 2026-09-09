//! Modelos proprios do app.
//!
//! Deliberadamente NAO reexportamos os tipos do `ytmapi-rs`/`rustypipe`: se
//! um deles quebrar ou for trocado, a mudanca fica confinada ao adaptador em
//! `metadata.rs` e o frontend nao percebe nada.

use serde::{Deserialize, Serialize};

/// Referencia a uma capa. `hash` e a chave no cache em disco; o frontend
/// monta a URL `ytmart://<hash>` e deixa o webview cuidar do resto.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtRef {
    pub hash: String,
    /// URL original, guardada para o download preguicoso na primeira vez.
    pub url: String,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtistRef {
    pub id: Option<String>,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AlbumRef {
    pub id: Option<String>,
    pub title: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Track {
    /// videoId do YouTube. E a chave de tudo: cache, fila, resolucao de stream.
    pub id: String,
    pub title: String,
    pub artists: Vec<ArtistRef>,
    pub album: Option<AlbumRef>,
    pub duration_secs: Option<u32>,
    pub art: Option<ArtRef>,
    pub is_explicit: bool,
    /// `setVideoId`: necessario para remover a faixa de uma playlist.
    pub set_video_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Album {
    pub id: String,
    pub title: String,
    pub artists: Vec<ArtistRef>,
    pub year: Option<u32>,
    pub art: Option<ArtRef>,
    pub track_count: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Artist {
    pub id: String,
    pub name: String,
    pub art: Option<ArtRef>,
    pub subscribers: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Playlist {
    pub id: String,
    pub title: String,
    pub author: Option<String>,
    pub art: Option<ArtRef>,
    pub track_count: Option<u32>,
}

/// Um item de resultado de busca. Tagged enum para o frontend fazer
/// `switch (item.type)` sem adivinhacao.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SearchItem {
    Track(Track),
    Album(Album),
    Artist(Artist),
    Playlist(Playlist),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SearchFilter {
    All,
    Songs,
    Albums,
    Artists,
    Playlists,
}

/// Pagina de resultados. `continuation` e opaco: vem do InnerTube e volta
/// para ele sem o frontend interpretar.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Page<T> {
    pub items: Vec<T>,
    pub continuation: Option<String>,
}

impl<T> Page<T> {
    pub fn new(items: Vec<T>) -> Self {
        Self { items, continuation: None }
    }

    pub fn empty() -> Self {
        Self { items: Vec::new(), continuation: None }
    }
}
