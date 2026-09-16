//! Instala e mantem atualizada a copia gerenciada do `yt-dlp`.
//!
//! O YouTube muda toda semana e um `yt-dlp` velho para de resolver stream
//! (ver `docs/plano.md > Problemas conhecidos`). Este modulo cuida de tres
//! coisas, nessa ordem, sempre no boot:
//!
//! 1. **Instalacao**: se a copia gravavel em `<data_dir>/bin` ainda nao
//!    existe e o bundle trouxe um binario embutido, copia um pro outro.
//! 2. **Escolha**: decide qual caminho o `YtDlp` do player vai usar
//!    (`choose_binary`).
//! 3. **Atualizacao**: em segundo plano, no maximo 1x/dia, tenta `-U` e cai
//!    para baixar a release mais recente do GitHub se falhar.
//!
//! So mexe na copia gerenciada — nunca no `ytdlp_path` do config (o usuario
//! cuida dele) nem no `yt-dlp` do PATH (o gerenciador de pacotes do sistema
//! cuida dele). Nada aqui pode impedir o app de abrir ou tocar com a copia
//! atual: toda falha vira `tracing::warn!` para quem chama, nunca panic.
//!
//! Sem dependencia do Tauri: quem resolve `resource_dir` (o caminho do
//! binario embutido no bundle) e o `src-tauri`; aqui so recebemos
//! `Option<&Path>`.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use sha2::{Digest, Sha256};

use crate::error::{CoreError, Result};

/// Nome do executavel do yt-dlp nesta plataforma — o mesmo nome que o
/// empacotamento usa para o recurso embutido (`resource_dir/yt-dlp/<NOME>`).
pub fn bin_name() -> &'static str {
    if cfg!(windows) {
        "yt-dlp.exe"
    } else {
        "yt-dlp"
    }
}

/// Caminho da copia gerenciada (gravavel) dentro de `bin_dir`.
pub fn managed_path(bin_dir: &Path) -> PathBuf {
    bin_dir.join(bin_name())
}

/// Arquivo-carimbo com o epoch da ultima atualizacao BEM-SUCEDIDA. Fica no
/// mesmo diretorio do binario para viver e morrer junto com ele.
fn stamp_path(bin_dir: &Path) -> PathBuf {
    bin_dir.join(".yt-dlp-last-update")
}

const UPDATE_INTERVAL_SECS: u64 = 24 * 60 * 60;
const UPDATE_TIMEOUT: Duration = Duration::from_secs(120);
const RELEASE_BASE: &str = "https://github.com/yt-dlp/yt-dlp/releases/latest/download";

/// De onde veio o binario escolhido por `choose_binary` — so para log.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinarySource {
    /// `ytdlp_path` no config do usuario.
    Config,
    /// Copia gerenciada em `<data_dir>/bin`.
    Managed,
    /// `yt-dlp` do PATH do sistema.
    Path,
}

impl BinarySource {
    pub fn label(self) -> &'static str {
        match self {
            Self::Config => "config (ytdlp_path)",
            Self::Managed => "copia gerenciada",
            Self::Path => "PATH do sistema",
        }
    }
}

/// Escolhe o binario do yt-dlp na ordem do contrato: `ytdlp_path` do config
/// (se definido) -> copia gerenciada (se o arquivo existir) -> nome no PATH.
/// So o config pode apontar para um caminho invalido — os outros dois so
/// precisam existir/estar no PATH, o que `YtDlp::check` confere depois.
pub fn choose_binary(config_ytdlp_path: Option<&str>, bin_dir: &Path) -> (String, BinarySource) {
    if let Some(path) = config_ytdlp_path.filter(|p| !p.is_empty()) {
        return (path.to_string(), BinarySource::Config);
    }
    let managed = managed_path(bin_dir);
    if managed.is_file() {
        return (managed.display().to_string(), BinarySource::Managed);
    }
    (bin_name().to_string(), BinarySource::Path)
}

/// Copia o binario embutido do bundle para a copia gerenciada, se esta
/// ainda nao existir. `embedded` pode ser `None` ou apontar para um arquivo
/// que nao existe (build de desenvolvimento sem o download do
/// empacotamento) — isso nunca e erro, so significa "nada para instalar".
///
/// Precisa rodar antes de `choose_binary` para a primeira execucao ja tocar
/// com a copia gerenciada em vez de cair no PATH do sistema.
pub fn install_from_embedded(bin_dir: &Path, embedded: Option<&Path>) -> Result<()> {
    let managed = managed_path(bin_dir);
    if managed.is_file() {
        return Ok(());
    }
    let Some(embedded) = embedded.filter(|p| p.is_file()) else {
        return Ok(());
    };

    std::fs::create_dir_all(bin_dir)
        .map_err(|e| CoreError::Other(format!("criando {}: {e}", bin_dir.display())))?;

    atomic_copy(embedded, &managed)?;
    set_executable(&managed)?;
    tracing::info!("yt-dlp: instalado a copia gerenciada a partir do binario embutido");
    Ok(())
}

/// Copia para um arquivo temporario no MESMO diretorio do destino e troca
/// por `rename` — um leitor concorrente (a proxima resolucao de stream, ou
/// a propria task de atualizacao) nunca ve um arquivo pela metade.
fn atomic_copy(src: &Path, dst: &Path) -> Result<()> {
    let tmp = tmp_path(dst);
    std::fs::copy(src, &tmp).map_err(|e| {
        CoreError::Other(format!(
            "copiando {} para {}: {e}",
            src.display(),
            tmp.display()
        ))
    })?;
    std::fs::rename(&tmp, dst)
        .map_err(|e| CoreError::Other(format!("substituindo {}: {e}", dst.display())))?;
    Ok(())
}

fn tmp_path(dst: &Path) -> PathBuf {
    let mut name = dst.file_name().unwrap_or_default().to_os_string();
    name.push(".tmp");
    dst.with_file_name(name)
}

#[cfg(unix)]
fn set_executable(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755))
        .map_err(|e| CoreError::Other(format!("permissao de execucao em {}: {e}", path.display())))
}

#[cfg(not(unix))]
fn set_executable(_path: &Path) -> Result<()> {
    Ok(())
}

fn read_stamp(bin_dir: &Path) -> Option<u64> {
    std::fs::read_to_string(stamp_path(bin_dir))
        .ok()?
        .trim()
        .parse()
        .ok()
}

fn write_stamp(bin_dir: &Path, now: u64) -> Result<()> {
    std::fs::write(stamp_path(bin_dir), now.to_string())
        .map_err(|e| CoreError::Other(format!("gravando carimbo de atualizacao: {e}")))
}

/// Se ja passaram 24h desde a ultima atualizacao BEM-SUCEDIDA. Sem carimbo
/// (primeira vez, ou a ultima tentativa falhou antes de gravar), atualiza.
pub fn should_update(bin_dir: &Path, now: u64) -> bool {
    match read_stamp(bin_dir) {
        Some(last) => now.saturating_sub(last) >= UPDATE_INTERVAL_SECS,
        None => true,
    }
}

/// Tenta `<managed> -U` (auto-atualizacao nativa do yt-dlp); se falhar, cai
/// para baixar a release mais recente do GitHub. So mexe em `bin_dir`.
///
/// Devolve `false` quando nao ha copia gerenciada (build de desenvolvimento
/// usando o yt-dlp do PATH): sem isso quem chama gravaria o carimbo e logaria
/// uma atualizacao que nunca aconteceu.
pub async fn update_managed(bin_dir: &Path) -> Result<bool> {
    let managed = managed_path(bin_dir);
    if !managed.is_file() {
        return Ok(false);
    }

    if try_self_update(&managed).await.is_ok() {
        return Ok(true);
    }

    download_latest(bin_dir, &managed).await?;
    Ok(true)
}

async fn try_self_update(managed: &Path) -> Result<()> {
    let mut cmd = tokio::process::Command::new(managed);
    cmd.arg("-U");
    crate::ytdlp::no_window(&mut cmd);

    let output = tokio::time::timeout(UPDATE_TIMEOUT, cmd.output())
        .await
        .map_err(|_| CoreError::Other("yt-dlp -U expirou".into()))?
        .map_err(|e| CoreError::Other(format!("rodando yt-dlp -U: {e}")))?;

    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        return Err(CoreError::Other(format!(
            "yt-dlp -U saiu com {}: {}",
            output.status,
            err.lines().last().unwrap_or("").trim()
        )));
    }
    Ok(())
}

/// Nome do asset na release do GitHub para esta plataforma. So Linux
/// x86_64 e Windows: as duas plataformas que o app empacota (Fase 5).
fn asset_name() -> &'static str {
    if cfg!(windows) {
        "yt-dlp.exe"
    } else {
        "yt-dlp_linux"
    }
}

async fn download_latest(bin_dir: &Path, managed: &Path) -> Result<()> {
    let client = reqwest::Client::new();

    let sums_text = client
        .get(format!("{RELEASE_BASE}/SHA2-256SUMS"))
        .send()
        .await
        .map_err(|e| CoreError::Network(format!("baixando SHA2-256SUMS: {e}")))?
        .error_for_status()
        .map_err(|e| CoreError::Network(format!("SHA2-256SUMS: {e}")))?
        .text()
        .await
        .map_err(|e| CoreError::Network(format!("lendo SHA2-256SUMS: {e}")))?;

    let sums = parse_sha256sums(&sums_text);
    let asset = asset_name();
    let expected = sums
        .get(asset)
        .ok_or_else(|| CoreError::Parse(format!("SHA2-256SUMS nao lista {asset}")))?;

    let bytes = client
        .get(format!("{RELEASE_BASE}/{asset}"))
        .send()
        .await
        .map_err(|e| CoreError::Network(format!("baixando {asset}: {e}")))?
        .error_for_status()
        .map_err(|e| CoreError::Network(format!("{asset}: {e}")))?
        .bytes()
        .await
        .map_err(|e| CoreError::Network(format!("lendo {asset}: {e}")))?;

    let got = sha256_hex(&bytes);
    if &got != expected {
        return Err(CoreError::Parse(format!(
            "checksum do {asset} nao bate (esperado {expected}, obtido {got}) — descartando"
        )));
    }

    std::fs::create_dir_all(bin_dir)
        .map_err(|e| CoreError::Other(format!("criando {}: {e}", bin_dir.display())))?;
    let tmp = tmp_path(managed);
    std::fs::write(&tmp, &bytes)
        .map_err(|e| CoreError::Other(format!("gravando {}: {e}", tmp.display())))?;
    set_executable(&tmp)?;

    // No Windows, sobrescrever um .exe em uso (uma resolucao de stream em
    // andamento com o binario antigo) pode falhar com "Access is denied".
    // Nao e fatal: fica o arquivo .tmp para a proxima tentativa, e o
    // carimbo nao avanca, entao o proximo boot tenta de novo.
    std::fs::rename(&tmp, managed)
        .map_err(|e| CoreError::Other(format!("substituindo {}: {e}", managed.display())))?;

    Ok(())
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// Parseia o `SHA2-256SUMS` da release: linhas `<hash>  <nome>` (formato
/// `sha256sum`, duas colunas separadas por espaco/asterisco). Devolve
/// `nome -> hash` em minusculas.
fn parse_sha256sums(text: &str) -> HashMap<String, String> {
    text.lines()
        .filter_map(|line| {
            let mut parts = line.split_whitespace();
            let hash = parts.next()?;
            let name = parts.next()?.trim_start_matches('*');
            Some((name.to_string(), hash.to_lowercase()))
        })
        .collect()
}

/// Task de atualizacao para rodar em segundo plano no boot (`tokio::spawn`).
/// Cuida do carimbo e do log; nunca propaga erro e nunca bloqueia quem
/// chama — o boot do app nao pode esperar nem falhar por causa dela.
pub async fn run_update_task(bin_dir: PathBuf) {
    let now = crate::epoch_secs();
    if !should_update(&bin_dir, now) {
        return;
    }
    match update_managed(&bin_dir).await {
        Ok(false) => tracing::debug!("yt-dlp: sem copia gerenciada, nada para atualizar"),
        Ok(true) => {
            if let Err(e) = write_stamp(&bin_dir, now) {
                tracing::warn!("yt-dlp: atualizou mas nao gravou o carimbo: {e}");
            } else {
                tracing::info!("yt-dlp: copia gerenciada atualizada");
            }
        }
        Err(e) => {
            tracing::warn!("yt-dlp: atualizacao falhou, tentando de novo no proximo boot: {e}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "ytcl-ytdlp-manager-test-{name}-{}-{}",
            std::process::id(),
            crate::epoch_secs()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn escolhe_config_quando_definido() {
        let bin_dir = tmp_dir("config");
        let (path, source) = choose_binary(Some("/opt/meu-yt-dlp"), &bin_dir);
        assert_eq!(path, "/opt/meu-yt-dlp");
        assert_eq!(source, BinarySource::Config);
        let _ = std::fs::remove_dir_all(&bin_dir);
    }

    #[test]
    fn config_vazio_nao_conta() {
        let bin_dir = tmp_dir("config-vazio");
        let (_, source) = choose_binary(Some(""), &bin_dir);
        assert_eq!(source, BinarySource::Path);
        let _ = std::fs::remove_dir_all(&bin_dir);
    }

    #[test]
    fn escolhe_gerenciado_quando_existe_e_sem_config() {
        let bin_dir = tmp_dir("managed");
        std::fs::write(managed_path(&bin_dir), b"fake").unwrap();
        let (path, source) = choose_binary(None, &bin_dir);
        assert_eq!(path, managed_path(&bin_dir).display().to_string());
        assert_eq!(source, BinarySource::Managed);
        let _ = std::fs::remove_dir_all(&bin_dir);
    }

    #[test]
    fn cai_para_path_sem_config_e_sem_gerenciado() {
        let bin_dir = tmp_dir("path-fallback");
        let (path, source) = choose_binary(None, &bin_dir);
        assert_eq!(path, bin_name());
        assert_eq!(source, BinarySource::Path);
        let _ = std::fs::remove_dir_all(&bin_dir);
    }

    #[test]
    fn instala_a_partir_do_embutido_com_permissao_de_execucao() {
        let base = tmp_dir("install");
        let bin_dir = base.join("bin");
        let embedded = base.join("embutido");
        std::fs::write(&embedded, b"conteudo fake do yt-dlp").unwrap();

        install_from_embedded(&bin_dir, Some(&embedded)).unwrap();

        let managed = managed_path(&bin_dir);
        assert!(managed.is_file());
        assert_eq!(
            std::fs::read(&managed).unwrap(),
            std::fs::read(&embedded).unwrap()
        );

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&managed).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o755);
        }

        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn nao_reinstala_se_a_copia_gerenciada_ja_existe() {
        let base = tmp_dir("no-reinstall");
        let bin_dir = base.join("bin");
        std::fs::create_dir_all(&bin_dir).unwrap();
        std::fs::write(managed_path(&bin_dir), b"copia atual").unwrap();

        let embedded = base.join("embutido");
        std::fs::write(&embedded, b"outra versao").unwrap();

        install_from_embedded(&bin_dir, Some(&embedded)).unwrap();

        assert_eq!(
            std::fs::read(managed_path(&bin_dir)).unwrap(),
            b"copia atual"
        );
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn sem_binario_embutido_nao_e_erro() {
        let base = tmp_dir("sem-embutido");
        let bin_dir = base.join("bin");

        assert!(install_from_embedded(&bin_dir, None).is_ok());
        assert!(install_from_embedded(&bin_dir, Some(&base.join("nao-existe"))).is_ok());
        assert!(!managed_path(&bin_dir).exists());

        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn regra_das_24h_do_carimbo() {
        let bin_dir = tmp_dir("stamp");
        let now = 1_000_000u64;

        // Sem carimbo: atualiza.
        assert!(should_update(&bin_dir, now));

        write_stamp(&bin_dir, now).unwrap();
        assert!(!should_update(&bin_dir, now));
        assert!(!should_update(&bin_dir, now + UPDATE_INTERVAL_SECS - 1));
        assert!(should_update(&bin_dir, now + UPDATE_INTERVAL_SECS));

        let _ = std::fs::remove_dir_all(&bin_dir);
    }

    #[test]
    fn parse_sha256sums_le_formato_com_duas_colunas() {
        let texto = "\
1fa6733c37ea6fb51c99ad8fe785e7b7e5f3246c9b980230329d4fb72ed8d4d6  yt-dlp
66674953fe251b89f4d08c5f0e35e0728679bd67ab3d7d05c0562af101dd3e7a  yt-dlp.exe
58162f9bfdc27458ea47bfcb311cf47028f17d8154a8bf7d689861d46399230a  yt-dlp_linux
";
        let sums = parse_sha256sums(texto);
        assert_eq!(
            sums.get("yt-dlp_linux").map(String::as_str),
            Some("58162f9bfdc27458ea47bfcb311cf47028f17d8154a8bf7d689861d46399230a")
        );
        assert_eq!(
            sums.get("yt-dlp.exe").map(String::as_str),
            Some("66674953fe251b89f4d08c5f0e35e0728679bd67ab3d7d05c0562af101dd3e7a")
        );
        assert_eq!(sums.len(), 3);
    }

    #[test]
    fn parse_sha256sums_ignora_linhas_vazias() {
        let sums = parse_sha256sums("\n\n  \n");
        assert!(sums.is_empty());
    }

    #[test]
    fn asset_name_bate_com_a_plataforma() {
        // So confere que o nome escolhido existe no SHA2-256SUMS real
        // testado em `parse_sha256sums_le_formato_com_duas_colunas` — aqui
        // so garantimos que a funcao devolve algo nao vazio e estavel.
        assert!(!asset_name().is_empty());
    }

    #[test]
    fn sha256_hex_bate_com_vetor_conhecido() {
        // SHA-256 de "" (string vazia) — vetor de teste padrao.
        assert_eq!(
            sha256_hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    /// Baixa o `SHA2-256SUMS` real e confere que o asset desta plataforma
    /// esta listado com um hash de 64 hex chars — sem baixar o binario
    /// (~35 MB). Canario para o formato do arquivo mudar.
    #[tokio::test]
    #[ignore = "precisa de rede"]
    async fn sha256sums_real_lista_o_asset_desta_plataforma() {
        let sums_text = reqwest::Client::new()
            .get(format!("{RELEASE_BASE}/SHA2-256SUMS"))
            .send()
            .await
            .unwrap()
            .error_for_status()
            .unwrap()
            .text()
            .await
            .unwrap();

        let sums = parse_sha256sums(&sums_text);
        let hash = sums.get(asset_name()).expect("asset nao listado");
        assert_eq!(hash.len(), 64);
        assert!(hash.chars().all(|c| c.is_ascii_hexdigit()));
    }
}
