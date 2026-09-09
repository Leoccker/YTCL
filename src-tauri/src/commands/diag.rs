//! Diagnostico para o overlay de debug (F3).
//!
//! Existe porque "RSS < 200 MB" e criterio de aceite do projeto, e um numero
//! que so aparece no fim e um numero que ninguem persegue.

use serde::Serialize;

#[derive(Serialize, Default)]
pub struct MemInfo {
    /// Soma da arvore de processos, nao so a do processo principal.
    pub rss_mb: Option<f64>,
    pub process_count: u32,
}

/// No Linux o WebKitGTK roda em processos separados (WebKitWebProcess,
/// WebKitNetworkProcess). Medir so o processo principal daria uns 30 MB e
/// esconderia justamente a parte cara — por isso somamos os descendentes.
#[cfg(target_os = "linux")]
fn read_tree_rss() -> MemInfo {
    use std::collections::HashMap;

    let page_kb = 4.0; // /proc/*/statm conta em paginas de 4 KiB no x86-64

    // ppid -> filhos, montado numa passada so sobre /proc.
    let mut children: HashMap<u32, Vec<u32>> = HashMap::new();
    let mut rss_pages: HashMap<u32, u64> = HashMap::new();

    let Ok(entries) = std::fs::read_dir("/proc") else {
        return MemInfo::default();
    };

    for entry in entries.flatten() {
        let Ok(pid) = entry.file_name().to_string_lossy().parse::<u32>() else {
            continue;
        };

        // O campo 4 de /proc/<pid>/stat e o ppid, mas o campo 2 (comm) pode
        // conter espacos e parenteses — por isso cortamos depois do ultimo ')'.
        let Ok(stat) = std::fs::read_to_string(format!("/proc/{pid}/stat")) else {
            continue;
        };
        let Some(after_comm) = stat.rsplit_once(')').map(|(_, r)| r) else {
            continue;
        };
        let Some(ppid) = after_comm.split_whitespace().nth(1).and_then(|s| s.parse().ok()) else {
            continue;
        };

        if let Ok(statm) = std::fs::read_to_string(format!("/proc/{pid}/statm")) {
            if let Some(rss) = statm.split_whitespace().nth(1).and_then(|s| s.parse().ok()) {
                rss_pages.insert(pid, rss);
            }
        }
        children.entry(ppid).or_default().push(pid);
    }

    let me = std::process::id();
    let mut total_pages = 0u64;
    let mut count = 0u32;
    let mut stack = vec![me];

    while let Some(pid) = stack.pop() {
        if let Some(pages) = rss_pages.get(&pid) {
            total_pages += pages;
            count += 1;
        }
        if let Some(kids) = children.get(&pid) {
            stack.extend(kids);
        }
    }

    MemInfo {
        rss_mb: Some(total_pages as f64 * page_kb / 1024.0),
        process_count: count,
    }
}

// TODO(fase 5): equivalente no Windows via GetProcessMemoryInfo sobre o job
// object do WebView2, que tambem roda em processos separados.
#[cfg(not(target_os = "linux"))]
fn read_tree_rss() -> MemInfo {
    MemInfo::default()
}

#[tauri::command]
pub fn mem_info() -> MemInfo {
    read_tree_rss()
}
