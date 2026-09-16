// Sem console no Windows em release.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod graphics;

fn main() {
    // Precisa vir antes de ytcl_lib::run(): mexe em variável de ambiente do
    // processo, e o run() já sobe threads/tasks assíncronas (GTK, tokio).
    // Ver graphics.rs para o porquê da mitigação em si.
    graphics::apply();
    ytcl_lib::run()
}
