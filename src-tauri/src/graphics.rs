//! Mitigação de driver gráfico no Linux.
//!
//! Alguns drivers NVIDIA (proprietários, versão principal < 560) corrompem a
//! composição DMABUF que o WebKitGTK usa por padrão, e a janela abre em
//! branco. O plano original (`docs/plano.md`) previa um script wrapper
//! (`packaging/ytcl.sh`) chamado pelo `.desktop`; isso foi substituído por
//! esta checagem dentro do próprio processo, porque assim vale para o `.deb`,
//! o `.rpm` e o AppImage sem precisar reescrever o `Exec=` de cada um.
//!
//! Precisa rodar bem no início do `main()`, **antes de qualquer thread
//! nascer** — troca variável de ambiente do processo, que não é thread-safe
//! (por isso `env::set_var` é `unsafe` desde a Rust 2024).

#[cfg(target_os = "linux")]
mod linux {
    use std::env;
    use std::fs;

    /// Onde o driver NVIDIA proprietário publica a própria versão.
    const NVIDIA_VERSION_FILE: &str = "/proc/driver/nvidia/version";

    /// A partir desta versão principal (inclusive) o workaround piora a
    /// performance sem necessidade — só vale a pena abaixo dela. Ver
    /// `docs/plano.md > Riscos e mitigações`.
    const NVIDIA_FIX_MAX_MAJOR: u32 = 560;

    /// Aplica as variáveis de ambiente de mitigação conforme
    /// `YTCL_GPU_WORKAROUND` (`auto` por padrão, `off`, ou `force`).
    pub fn apply() {
        match env::var("YTCL_GPU_WORKAROUND").as_deref() {
            Ok("off") => {}
            Ok("force") => {
                // Último recurso citado no plano: além da flag do NVIDIA,
                // desliga o caminho DMABUF do WebKitGTK inteiro.
                set_if_unset("__NV_DISABLE_EXPLICIT_SYNC", "1");
                set_if_unset("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
            }
            // "auto" (padrão) e qualquer valor desconhecido caem aqui — um
            // valor de env var incomum não deve desligar a proteção.
            _ => auto(),
        }
    }

    fn auto() {
        let Ok(text) = fs::read_to_string(NVIDIA_VERSION_FILE) else {
            // Sem o arquivo: não é NVIDIA proprietário (ou é nouveau, que não
            // sofre desse bug), nada a fazer.
            return;
        };
        if let Some(major) = nvidia_major_version(&text) {
            if major < NVIDIA_FIX_MAX_MAJOR {
                set_if_unset("__NV_DISABLE_EXPLICIT_SYNC", "1");
            }
        }
    }

    /// Nunca sobrescreve o que o usuário já definiu explicitamente no
    /// ambiente — mesmo que o valor dele seja diferente do que a gente teria
    /// escolhido.
    fn set_if_unset(key: &str, value: &str) {
        if env::var_os(key).is_none() {
            // SAFETY: chamada logo no início do `main()`, antes de o Tauri
            // (ou qualquer outra coisa) criar threads — não há concorrência
            // possível de leitura/escrita nas variáveis de ambiente aqui.
            unsafe { env::set_var(key, value) };
        }
    }

    /// Extrai a versão principal do driver a partir do texto de
    /// `/proc/driver/nvidia/version`. Formatos reais observados:
    ///
    /// ```text
    /// NVRM version: NVIDIA UNIX x86_64 Kernel Module  550.120  Fri Sep 13 10:10:01 UTC 2024
    /// NVRM version: NVIDIA UNIX Open Kernel Module for x86_64  560.35.03  Release Build  (dvs-builder@U16-I3-B03-4-3)  Fri Aug 16 21:42:42 UTC 2024
    /// ```
    ///
    /// A versão é o único token da primeira linha com um ponto decimal e que
    /// começa com dígitos — isso descarta "x86_64" (sem ponto) e datas/horas
    /// (têm `:` ou não têm ponto).
    pub(super) fn nvidia_major_version(text: &str) -> Option<u32> {
        let first_line = text.lines().next()?;
        first_line.split_whitespace().find_map(|tok| {
            let major = tok.split('.').next()?;
            let looks_like_version =
                tok.contains('.') && !major.is_empty() && major.bytes().all(|b| b.is_ascii_digit());
            looks_like_version.then(|| major.parse().ok()).flatten()
        })
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn parses_classic_kernel_module() {
            let text = "NVRM version: NVIDIA UNIX x86_64 Kernel Module  550.120  Fri Sep 13 10:10:01 UTC 2024\n";
            assert_eq!(nvidia_major_version(text), Some(550));
        }

        #[test]
        fn parses_open_kernel_module() {
            let text = "NVRM version: NVIDIA UNIX Open Kernel Module for x86_64  560.35.03  Release Build  (dvs-builder@U16-I3-B03-4-3)  Fri Aug 16 21:42:42 UTC 2024\n";
            assert_eq!(nvidia_major_version(text), Some(560));
        }

        #[test]
        fn boundary_version_is_not_below_max() {
            // 560 é o limite: não é "< 560", então apply() não deveria setar
            // a variável para essa versão (testado indiretamente aqui via
            // parsing; o efeito colateral em env fica coberto pela leitura
            // manual, já que testes de env global não são paralelizáveis).
            let text = "NVRM version: NVIDIA UNIX x86_64 Kernel Module  560.0  Fri Jan 1 00:00:00 UTC 2024\n";
            let major = nvidia_major_version(text).unwrap();
            assert!(!(major < NVIDIA_FIX_MAX_MAJOR));
        }

        #[test]
        fn returns_none_without_a_version_token() {
            assert_eq!(nvidia_major_version("nada aqui parece versão\n"), None);
        }

        #[test]
        fn returns_none_for_empty_input() {
            assert_eq!(nvidia_major_version(""), None);
        }
    }
}

#[cfg(target_os = "linux")]
pub fn apply() {
    linux::apply();
}

#[cfg(not(target_os = "linux"))]
pub fn apply() {
    // No-op fora do Linux: o bug é específico do WebKitGTK + DMABUF, que só
    // existe nessa plataforma.
}
