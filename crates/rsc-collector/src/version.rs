//! Windows build identification — reads straight from the registry.
//!
//! Uses `HKLM\SOFTWARE\Microsoft\Windows NT\CurrentVersion`:
//! * `CurrentMajorVersionNumber` (DWORD, Win10 1507+)
//! * `CurrentMinorVersionNumber` (DWORD, Win10 1507+)
//! * `CurrentBuildNumber` (REG_SZ)
//! * `UBR` (DWORD — revision bumped per Windows Update)
//!
//! Build id format: `{Major}_{Build}_{UBR}` (see `DATABASE.md` §D-20 and
//! `KernelSymbolsCollector` for the convention).

use std::fmt;

use tracing::debug;
use winreg::enums::HKEY_LOCAL_MACHINE;
use winreg::RegKey;

use crate::error::CollectError;

const WINDOWS_NT_CURRENT_VERSION: &str = r"SOFTWARE\Microsoft\Windows NT\CurrentVersion";

#[derive(Debug, Clone, Copy)]
pub struct WindowsBuild {
    pub major: u32,
    /// Always 0 on modern Windows; kept for completeness and future use.
    #[allow(dead_code)]
    pub minor: u32,
    pub build: u32,
    pub ubr: u32,
}

impl WindowsBuild {
    /// Reads the current Windows build identity from the registry.
    pub fn detect() -> Result<Self, CollectError> {
        let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
        let cv = hklm
            .open_subkey(WINDOWS_NT_CURRENT_VERSION)
            .map_err(|e| CollectError::VersionDetection(format!("open CurrentVersion key: {e}")))?;

        let major: u32 = cv.get_value("CurrentMajorVersionNumber").map_err(|e| {
            CollectError::VersionDetection(format!("CurrentMajorVersionNumber: {e}"))
        })?;
        let minor: u32 = cv.get_value("CurrentMinorVersionNumber").map_err(|e| {
            CollectError::VersionDetection(format!("CurrentMinorVersionNumber: {e}"))
        })?;

        let build_s: String = cv
            .get_value("CurrentBuildNumber")
            .map_err(|e| CollectError::VersionDetection(format!("CurrentBuildNumber: {e}")))?;
        let build: u32 = build_s
            .parse()
            .map_err(|_| CollectError::VersionDetection(format!("unparseable build {build_s:?}")))?;

        let ubr: u32 = cv
            .get_value("UBR")
            .map_err(|e| CollectError::VersionDetection(format!("UBR: {e}")))?;

        debug!(major, minor, build, ubr, "detected Windows build");
        Ok(Self { major, minor, build, ubr })
    }

    /// Windows family number normalized against the major/build quirk.
    ///
    /// Microsoft kept `CurrentMajorVersionNumber = 10` in the registry on
    /// Windows 11 for backwards compatibility, so the actual family must
    /// be derived from the build number: builds ≥ 22000 are Win11.
    pub fn family(&self) -> u32 {
        match (self.major, self.build) {
            (11, _) => 11,
            (10, b) if b >= 22000 => 11,
            (10, _) => 10,
            (m, _) => m,
        }
    }

    /// Build id used in DB file names: `{Family}_{Build}_{UBR}`.
    pub fn id(&self) -> String {
        format!("{}_{}_{}", self.family(), self.build, self.ubr)
    }

    /// Human-readable version label for the TOML `meta.windows_version` field.
    pub fn label(&self) -> String {
        format!("{} (build {}.{})", self.family(), self.build, self.ubr)
    }
}

impl fmt::Display for WindowsBuild {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.id())
    }
}
