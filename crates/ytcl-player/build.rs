//! Link do libmpv no Windows.
//!
//! O `libmpv2-sys` só emite `cargo:rustc-link-lib=mpv` (o *nome* da lib); no
//! Linux isso basta porque o `mpv-libs-devel`/`mpv-libs` do sistema deixa
//! `libmpv.so` num diretório que o linker já procura por padrão. No Windows
//! não existe esse padrão — quem baixa o SDK de desenvolvimento
//! (`packaging/fetch-mpv-windows.ps1`) gera o import library `mpv.lib` num
//! diretório de cache e exporta `MPV_LIB_DIR` apontando pra lá. Este
//! `build.rs` só repassa esse diretório pro linker.
fn main() {
    println!("cargo:rerun-if-env-changed=MPV_LIB_DIR");

    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if target_os != "windows" {
        // Linux (e qualquer outro alvo): nada a fazer, o libmpv2-sys já
        // resolve sozinho via link padrão do sistema.
        return;
    }

    if let Ok(dir) = std::env::var("MPV_LIB_DIR") {
        println!("cargo:rustc-link-search=native={dir}");
    }
}
