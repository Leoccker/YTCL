//! Busca pela linha de comando, para exercitar o nucleo sem abrir a UI.
//!
//! Uso: cargo run -p ytcl-core --example search -- "radiohead" [filtro]
//! Filtros: all (padrao), songs, albums, artists, playlists

use ytcl_core::innertube::InnerTube;
use ytcl_core::metadata::MetadataSource;
use ytcl_core::model::{SearchFilter, SearchItem};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let query = args.next().unwrap_or_else(|| "radiohead".into());
    let filter = match args.next().as_deref() {
        Some("songs") => SearchFilter::Songs,
        Some("albums") => SearchFilter::Albums,
        Some("artists") => SearchFilter::Artists,
        Some("playlists") => SearchFilter::Playlists,
        _ => SearchFilter::All,
    };

    let dir = std::env::temp_dir().join("ytcl-example");
    std::fs::create_dir_all(&dir)?;
    let it = InnerTube::new(&dir)?;

    let inicio = std::time::Instant::now();
    let page = it.search(&query, filter).await?;
    let levou = inicio.elapsed();

    println!("\"{query}\" ({filter:?}) — {} itens em {levou:.0?}\n", page.items.len());

    for item in page.items.iter().take(15) {
        match item {
            SearchItem::Track(t) => {
                let artistas = t.artists.iter().map(|a| a.name.as_str()).collect::<Vec<_>>().join(", ");
                let dur = t
                    .duration_secs
                    .map(|s| format!("{}:{:02}", s / 60, s % 60))
                    .unwrap_or_else(|| "--:--".into());
                println!("  faixa    {dur:>6}  {} — {artistas}", t.title);
            }
            SearchItem::Album(a) => {
                let artistas = a.artists.iter().map(|x| x.name.as_str()).collect::<Vec<_>>().join(", ");
                let ano = a.year.map(|y| y.to_string()).unwrap_or_default();
                println!("  album    {ano:>6}  {} — {artistas}", a.title);
            }
            SearchItem::Artist(a) => {
                println!("  artista  {:>6}  {}", "", a.name);
            }
            SearchItem::Playlist(p) => {
                let n = p.track_count.map(|n| n.to_string()).unwrap_or_default();
                println!("  playlist {n:>6}  {}", p.title);
            }
        }
    }

    let com_capa = page.items.iter().filter(|i| art_of(i).is_some()).count();
    println!("\ncapas resolvidas: {com_capa}/{}", page.items.len());
    println!("proxima pagina: {}", if page.continuation.is_some() { "sim" } else { "nao" });

    if let Some(art) = page.items.iter().find_map(art_of) {
        println!("exemplo de capa: ytmart://{}", art.hash);
        println!("             -> {}", art.url);
    }

    Ok(())
}

fn art_of(i: &SearchItem) -> Option<&ytcl_core::model::ArtRef> {
    match i {
        SearchItem::Track(t) => t.art.as_ref(),
        SearchItem::Album(a) => a.art.as_ref(),
        SearchItem::Artist(a) => a.art.as_ref(),
        SearchItem::Playlist(p) => p.art.as_ref(),
    }
}
