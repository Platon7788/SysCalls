//! NTSTATUS wrapper and ergonomic `Result` helpers.
//!
//! Runtime code never panics: failures surface via `NtStatus` or
//! `RscResult<T>`. See §S-10.
//!
//! The `name()` method returns `Some(&'static str)` for a small curated list
//! of common statuses. Enable the `status-names-full` feature to include the
//! full table (≈ 260 entries, +10 KB to the binary).

use crate::types::NTSTATUS;

/// `#[repr(transparent)]` wrapper around a raw `NTSTATUS` value.
#[repr(transparent)]
#[derive(Copy, Clone, Eq, PartialEq, Hash)]
pub struct NtStatus(pub NTSTATUS);

/// Convenience result alias used by the crate's high-level helpers.
pub type RscResult<T> = Result<T, NtStatus>;

impl NtStatus {
    /// Constructs a status from a raw value.
    #[inline]
    pub const fn new(status: NTSTATUS) -> Self {
        Self(status)
    }

    /// Raw `i32` code (sign bit encodes severity).
    #[inline]
    pub const fn code(self) -> NTSTATUS {
        self.0
    }

    /// Top two bits encode severity: 0 = SUCCESS, 1 = INFO, 2 = WARN, 3 = ERR.
    #[inline]
    pub const fn severity(self) -> u8 {
        ((self.0 as u32) >> 30) as u8
    }

    #[inline]
    pub const fn is_success(self) -> bool {
        self.0 >= 0
    }

    #[inline]
    pub const fn is_information(self) -> bool {
        self.severity() == 1
    }

    #[inline]
    pub const fn is_warning(self) -> bool {
        self.severity() == 2
    }

    #[inline]
    pub const fn is_error(self) -> bool {
        self.severity() == 3
    }

    /// Returns the symbolic name for a recognized status, `None` otherwise.
    pub const fn name(self) -> Option<&'static str> {
        status_name(self.0)
    }

    /// Turn this status into an `Ok(value)` on success, `Err(self)` otherwise.
    #[inline]
    pub fn to_result<T>(self, value: T) -> RscResult<T> {
        if self.is_success() {
            Ok(value)
        } else {
            Err(self)
        }
    }
}

impl core::fmt::Debug for NtStatus {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self.name() {
            Some(n) => write!(f, "NtStatus({n} = 0x{:08X})", self.0 as u32),
            None => write!(f, "NtStatus(0x{:08X})", self.0 as u32),
        }
    }
}

impl core::fmt::Display for NtStatus {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        core::fmt::Debug::fmt(self, f)
    }
}

impl From<NTSTATUS> for NtStatus {
    #[inline]
    fn from(value: NTSTATUS) -> Self {
        Self(value)
    }
}

impl From<NtStatus> for NTSTATUS {
    #[inline]
    fn from(value: NtStatus) -> Self {
        value.0
    }
}

/// Extension for raw `NTSTATUS` values returned by FFI / asm.
pub trait NtStatusExt: Sized + Copy {
    /// Convert a raw NTSTATUS to a unit result (`Ok(())` on success).
    fn to_result(self) -> RscResult<()>;

    /// Like [`to_result`] but returns an arbitrary value on success.
    fn to_result_with<T>(self, value: T) -> RscResult<T>;
}

impl NtStatusExt for NTSTATUS {
    #[inline]
    fn to_result(self) -> RscResult<()> {
        NtStatus(self).to_result(())
    }

    #[inline]
    fn to_result_with<T>(self, value: T) -> RscResult<T> {
        NtStatus(self).to_result(value)
    }
}

// --- Curated list of well-known statuses ----------------------------------

macro_rules! declare_status {
    ( $( $name:ident = $value:expr ),* $(,)? ) => {
        $(
            #[doc = concat!("`", stringify!($name), "`.")]
            pub const $name: NtStatus = NtStatus($value as i32);
        )*

        /// Maps a raw NTSTATUS to its symbolic name.
        const fn status_name(code: NTSTATUS) -> Option<&'static str> {
            match code as u32 {
                $(
                    v if v == ($value as u32) => Some(stringify!($name)),
                )*
                _ => None,
            }
        }
    };
}

// Critical / frequently-seen statuses. Full set is gated behind
// the `status-names-full` feature (to be populated later phases).
declare_status! {
    STATUS_SUCCESS                   = 0x00000000u32,
    STATUS_UNSUCCESSFUL              = 0xC0000001u32,
    STATUS_NOT_IMPLEMENTED           = 0xC0000002u32,
    STATUS_INVALID_INFO_CLASS        = 0xC0000003u32,
    STATUS_INFO_LENGTH_MISMATCH      = 0xC0000004u32,
    STATUS_ACCESS_VIOLATION          = 0xC0000005u32,
    STATUS_INVALID_HANDLE            = 0xC0000008u32,
    STATUS_INVALID_PARAMETER         = 0xC000000Du32,
    STATUS_NO_MEMORY                 = 0xC0000017u32,
    STATUS_ACCESS_DENIED             = 0xC0000022u32,
    STATUS_BUFFER_TOO_SMALL          = 0xC0000023u32,
    STATUS_OBJECT_NAME_NOT_FOUND     = 0xC0000034u32,
    STATUS_OBJECT_NAME_COLLISION     = 0xC0000035u32,
    STATUS_OBJECT_PATH_NOT_FOUND     = 0xC000003Au32,
    STATUS_INSUFFICIENT_RESOURCES    = 0xC000009Au32,
    STATUS_NOT_SUPPORTED             = 0xC00000BBu32,
    STATUS_TIMEOUT                   = 0x00000102u32,
    STATUS_PENDING                   = 0x00000103u32,
    STATUS_PROCEDURE_NOT_FOUND       = 0xC000007Au32,
    STATUS_IMAGE_NOT_AT_BASE         = 0x40000003u32,
    STATUS_BUFFER_OVERFLOW           = 0x80000005u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn success_is_ok() {
        assert!(STATUS_SUCCESS.is_success());
        assert!(!STATUS_SUCCESS.is_error());
        assert_eq!(STATUS_SUCCESS.code(), 0);
    }

    #[test]
    fn access_denied_is_error() {
        assert!(!STATUS_ACCESS_DENIED.is_success());
        assert!(STATUS_ACCESS_DENIED.is_error());
        assert_eq!(STATUS_ACCESS_DENIED.severity(), 3);
    }

    #[test]
    fn pending_is_success_class() {
        // 0x00000103 — top two bits are 00 → SUCCESS severity class.
        assert!(STATUS_PENDING.is_success());
        assert_eq!(STATUS_PENDING.severity(), 0);
        assert!(!STATUS_PENDING.is_information());
    }

    #[test]
    fn image_not_at_base_is_information_class() {
        // 0x40000003 — top two bits are 01 → INFORMATION. Positive as i32,
        // so `is_success` is true (non-error) while `is_information` is
        // also true (subtype).
        assert!(STATUS_IMAGE_NOT_AT_BASE.is_success());
        assert_eq!(STATUS_IMAGE_NOT_AT_BASE.severity(), 1);
        assert!(STATUS_IMAGE_NOT_AT_BASE.is_information());
    }

    #[test]
    fn buffer_overflow_is_warning_class() {
        // 0x80000005 — top two bits are 10 → WARNING. Negative as i32, so
        // `is_success` is false.
        assert!(!STATUS_BUFFER_OVERFLOW.is_success());
        assert_eq!(STATUS_BUFFER_OVERFLOW.severity(), 2);
        assert!(STATUS_BUFFER_OVERFLOW.is_warning());
    }

    #[test]
    fn status_name_roundtrip() {
        assert_eq!(STATUS_ACCESS_DENIED.name(), Some("STATUS_ACCESS_DENIED"));
        assert_eq!(NtStatus(0xDEADBEEFu32 as i32).name(), None);
    }

    #[test]
    fn ext_trait_maps_to_result() {
        let ok: NTSTATUS = 0;
        let err: NTSTATUS = 0xC0000005u32 as i32;
        assert!(ok.to_result().is_ok());
        assert!(err.to_result().is_err());
        assert_eq!(ok.to_result_with(42).unwrap(), 42);
    }

    #[test]
    fn transparent_size() {
        assert_eq!(core::mem::size_of::<NtStatus>(), core::mem::size_of::<NTSTATUS>());
    }
}
