//! Diagnostico para o overlay de debug (F3).
//!
//! Existe porque "RSS < 200 MB" e criterio de aceite do projeto, e um numero
//! que so aparece no fim e um numero que ninguem persegue.

use serde::Serialize;

#[derive(Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct MemInfo {
    /// PSS somado da arvore de processos. PSS divide cada pagina compartilhada
    /// entre os processos que a usam, entao somar a arvore da o numero real —
    /// com RSS, as bibliotecas que os tres processos compartilham (GTK, libc,
    /// WebKit) seriam contadas tres vezes e o total sai ~45% inflado.
    pub pss_mb: Option<f64>,
    /// RSS somado, so para comparacao no overlay. Sempre maior que o PSS.
    pub rss_mb: Option<f64>,
    pub process_count: u32,
}

/// No Linux o WebKitGTK roda em processos separados (WebKitWebProcess,
/// WebKitNetworkProcess). Medir so o processo principal daria uns 30 MB e
/// esconderia justamente a parte cara — por isso somamos os descendentes.
#[cfg(target_os = "linux")]
fn read_tree_rss() -> MemInfo {
    use std::collections::HashMap;

    const PAGE_KB: f64 = 4.0; // /proc/*/statm conta em paginas de 4 KiB no x86-64

    // ppid -> filhos, montado numa passada so sobre /proc.
    let mut children: HashMap<u32, Vec<u32>> = HashMap::new();
    let mut mem: HashMap<u32, (f64, f64)> = HashMap::new(); // pid -> (rss_mb, pss_mb)

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
        children.entry(ppid).or_default().push(pid);

        let rss_mb = std::fs::read_to_string(format!("/proc/{pid}/statm"))
            .ok()
            .and_then(|s| s.split_whitespace().nth(1)?.parse::<f64>().ok())
            .map(|pages| pages * PAGE_KB / 1024.0);

        let Some(rss_mb) = rss_mb else { continue };
        mem.insert(pid, (rss_mb, read_pss_mb(pid).unwrap_or(rss_mb)));
    }

    let mut total_rss = 0.0;
    let mut total_pss = 0.0;
    let mut count = 0u32;
    let mut stack = vec![std::process::id()];

    while let Some(pid) = stack.pop() {
        if let Some((rss, pss)) = mem.get(&pid) {
            total_rss += rss;
            total_pss += pss;
            count += 1;
        }
        if let Some(kids) = children.get(&pid) {
            stack.extend(kids);
        }
    }

    MemInfo {
        pss_mb: Some(total_pss),
        rss_mb: Some(total_rss),
        process_count: count,
    }
}

/// PSS vem de smaps_rollup (Linux >= 4.14). Se nao der para ler, o chamador
/// cai para RSS — numero pior, mas melhor que nenhum.
#[cfg(target_os = "linux")]
fn read_pss_mb(pid: u32) -> Option<f64> {
    let rollup = std::fs::read_to_string(format!("/proc/{pid}/smaps_rollup")).ok()?;
    rollup
        .lines()
        .find_map(|l| l.strip_prefix("Pss:"))
        .and_then(|v| v.split_whitespace().next()?.parse::<f64>().ok())
        .map(|kb| kb / 1024.0)
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
