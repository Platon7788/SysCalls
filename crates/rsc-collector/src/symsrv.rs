//! Microsoft Symbol Server client.
//!
//! Layout: `https://msdl.microsoft.com/download/symbols/<pdb_name>/<GUID><AGE>/<pdb_name>`
//!
//! Cache (KSC-compatible): `%APPDATA%\rsc\cache\<pdb_name>\<GUID><AGE>\<pdb_name>`.
//!
//! CAB handling: if the response starts with `MSCF`, we save the CAB
//! temporarily and invoke `expand.exe -F:* <cab> <dir>` to decompress —
//! the same approach the `KernelSymbolsCollector` uses, and it keeps
//! the dep footprint minimal (no pure-Rust CAB crate).

use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::LazyLock;
use std::time::Duration;

use tracing::{debug, info, warn};
use ureq::Agent;

use crate::error::CollectError;
use crate::pe::PdbRef;

const DEFAULT_BASE_URL: &str = "https://msdl.microsoft.com/download/symbols";
/// Spoofed user-agent — matches the real Microsoft Symbol Server client,
/// which keeps some corporate proxies from 403'ing us.
const USER_AGENT: &str = "Microsoft-Symbol-Server/10.0.0.0";
/// Large, because kernel PDBs (~60 MB) can take a while on slow links.
const TIMEOUT: Duration = Duration::from_secs(300);
const CAB_MAGIC: &[u8; 4] = b"MSCF";

/// Computed once: `%APPDATA%\rsc\cache`. Override via `RSC_CACHE_DIR`.
static CACHE_ROOT: LazyLock<PathBuf> = LazyLock::new(|| {
    if let Ok(dir) = std::env::var("RSC_CACHE_DIR") {
        return PathBuf::from(dir);
    }
    let appdata =
        std::env::var("APPDATA").unwrap_or_else(|_| String::from(r"C:\Users\Default\AppData\Roaming"));
    PathBuf::from(appdata).join("rsc").join("cache")
});

pub struct SymSrv {
    agent: Agent,
    base_url: String,
}

impl SymSrv {
    pub fn new() -> Self {
        let config = Agent::config_builder()
            .user_agent(USER_AGENT)
            .timeout_global(Some(TIMEOUT))
            .build();
        Self {
            agent: Agent::new_with_config(config),
            base_url: DEFAULT_BASE_URL.to_string(),
        }
    }

    /// Fetch the PDB for the given `PdbRef`, using the disk cache when
    /// possible. Returns a path to the decompressed PDB on local disk.
    pub fn fetch(&self, pdb: &PdbRef) -> Result<PathBuf, CollectError> {
        let cache_path = cache_path_for(pdb);
        if cache_path.exists() {
            debug!(path = %cache_path.display(), "PDB cache hit");
            return Ok(cache_path);
        }

        if let Some(parent) = cache_path.parent() {
            fs::create_dir_all(parent).map_err(|e| {
                CollectError::Network(format!(
                    "create cache dir {}: {e}",
                    parent.display()
                ))
            })?;
        }

        let url = format!("{}/{}/{}/{}", self.base_url, pdb.pdb_name, pdb.sym_path(), pdb.pdb_name);
        info!(url = %url, "downloading PDB");

        let bytes = self.download(&url)?;
        self.store(&bytes, &cache_path)?;

        Ok(cache_path)
    }

    fn download(&self, url: &str) -> Result<Vec<u8>, CollectError> {
        let mut response = self
            .agent
            .get(url)
            .call()
            .map_err(|e| match e {
                ureq::Error::StatusCode(status) => CollectError::SymbolServer {
                    url: url.to_string(),
                    status,
                },
                other => CollectError::Network(format!("{other}")),
            })?;

        let mut buf = Vec::new();
        response
            .body_mut()
            .as_reader()
            .read_to_end(&mut buf)
            .map_err(|e| CollectError::Network(format!("read body: {e}")))?;
        Ok(buf)
    }

    fn store(&self, bytes: &[u8], dest: &Path) -> Result<(), CollectError> {
        if bytes.len() >= 4 && &bytes[..4] == CAB_MAGIC {
            debug!(size = bytes.len(), "response is CAB; unpacking via expand.exe");
            self.unpack_cab(bytes, dest)
        } else {
            atomic_write(dest, bytes)
        }
    }

    fn unpack_cab(&self, cab: &[u8], dest: &Path) -> Result<(), CollectError> {
        // 1. Write the CAB to a sibling tempfile.
        let cab_path = dest.with_extension("cab.tmp");
        atomic_write(&cab_path, cab)?;

        // 2. Ask Windows' bundled `expand.exe` to decompress into the cache dir.
        let dest_dir = dest.parent().unwrap_or(Path::new("."));
        let status = Command::new("expand.exe")
            .args([
                "-F:*",
                cab_path.to_string_lossy().as_ref(),
                dest_dir.to_string_lossy().as_ref(),
            ])
            .status()
            .map_err(|e| CollectError::CabExtract(format!("spawn expand.exe: {e}")))?;

        let _ = fs::remove_file(&cab_path);

        if !status.success() {
            return Err(CollectError::CabExtract(format!(
                "expand.exe returned {:?}",
                status.code()
            )));
        }

        if !dest.exists() {
            warn!(path = %dest.display(), "expand.exe succeeded but target missing");
            return Err(CollectError::CabExtract(
                "expand.exe produced no matching file".into(),
            ));
        }
        Ok(())
    }
}

fn cache_path_for(pdb: &PdbRef) -> PathBuf {
    CACHE_ROOT
        .join(&pdb.pdb_name)
        .join(pdb.sym_path())
        .join(&pdb.pdb_name)
}

fn atomic_write(dest: &Path, bytes: &[u8]) -> Result<(), CollectError> {
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent).map_err(|e| {
            CollectError::Network(format!("create dir {}: {e}", parent.display()))
        })?;
    }
    let tmp = dest.with_extension("tmp");
    fs::write(&tmp, bytes)
        .map_err(|e| CollectError::Network(format!("write tmp {}: {e}", tmp.display())))?;
    fs::rename(&tmp, dest)
        .map_err(|e| CollectError::Network(format!("rename tmp → dest: {e}")))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_root_is_under_appdata_or_override() {
        let root = CACHE_ROOT.clone();
        assert!(
            root.ends_with("rsc/cache") || root.ends_with(r"rsc\cache"),
            "unexpected cache root: {}",
            root.display()
        );
    }

    #[test]
    fn cache_path_layout() {
        let pdb = PdbRef {
            pdb_name: "ntdll.pdb".to_string(),
            guid: [
                0xAB, 0xCD, 0xEF, 0x01, 0x23, 0x45, 0x67, 0x89, 0xAB, 0xCD, 0xEF, 0x01, 0x23, 0x45,
                0x67, 0x89,
            ],
            age: 1,
        };
        let path = cache_path_for(&pdb);
        let s = path.to_string_lossy();
        assert!(s.contains("ntdll.pdb"));
        assert!(s.contains("ABCDEF01234567"));
        assert!(s.contains("1"));
    }
}
