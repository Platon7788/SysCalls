//! Phnt header parser.
//!
//! Handles the canonical phnt-style declaration shape:
//!
//! ```text
//! NTSYSCALLAPI
//! NTSTATUS
//! NTAPI
//! NtName(
//!     _In_     HANDLE      ProcessHandle,
//!     _Out_    PVOID       *BaseAddress,
//!     _Inout_  PSIZE_T     RegionSize,
//!     ...
//!     );
//! ```
//!
//! `NTSYSCALLAPI` can also appear as `NTSYSAPI` in some modules (same shape).
//! `NTAPI` is the standard Microsoft stdcall macro.
//!
//! Parsing is done in two passes:
//! 1. Strip C comments and track `#if (PHNT_VERSION >= PHNT_XXX)` /
//!    `#ifdef _KERNEL_MODE` blocks.
//! 2. Regex-locate declaration starts, then walk parameters with a
//!    paren-aware reader so nested SAL macros (`_At_(*x, _Readable_...)`)
//!    don't break the list.

use std::fs;
use std::path::Path;

use anyhow::{bail, Context, Result};
use regex::Regex;
use serde::{Deserialize, Serialize};
use tracing::{debug, warn};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PhntSignature {
    pub name: String,
    pub return_type: String,
    pub params: Vec<PhntParam>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_phnt_version: Option<String>,
    /// Source header the declaration came from (for diagnostics).
    pub source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PhntParam {
    pub name: String,
    /// Phnt type spelling, pre-normalization (e.g. `PHANDLE` → `*mut HANDLE`
    /// happens in `normalizer.rs`).
    pub r#type: String,
    pub direction: Direction,
    #[serde(default, skip_serializing_if = "core::ops::Not::not")]
    pub optional: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Direction {
    In,
    Out,
    Inout,
    /// Unknown — no recognized SAL annotation; treat as `inout` for safety.
    Unknown,
}

/// Scans all `*.h` files in the given directory and returns every
/// recognized NT syscall signature. Order is deterministic (header
/// filename asc, then declaration order within each file).
pub fn parse_directory(dir: &Path) -> Result<Vec<PhntSignature>> {
    let mut paths: Vec<_> = fs::read_dir(dir)
        .with_context(|| format!("reading phnt directory {}", dir.display()))?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().and_then(|s| s.to_str()) == Some("h"))
        .map(|e| e.path())
        .collect();
    paths.sort();

    let mut out = Vec::new();
    for path in paths {
        match parse_file(&path) {
            Ok(mut sigs) => {
                debug!(path = %path.display(), count = sigs.len(), "parsed");
                out.append(&mut sigs);
            }
            Err(e) => warn!(path = %path.display(), error = %e, "skip"),
        }
    }
    Ok(out)
}

/// Parses a single header file.
pub fn parse_file(path: &Path) -> Result<Vec<PhntSignature>> {
    let raw = fs::read_to_string(path)
        .with_context(|| format!("reading {}", path.display()))?;
    let source = path.file_name().unwrap_or_default().to_string_lossy().into_owned();
    Ok(parse_text(&raw, &source))
}

/// Parses already-loaded header text (used by tests).
pub fn parse_text(raw: &str, source: &str) -> Vec<PhntSignature> {
    let cleaned = strip_comments(raw);

    // Regex matches the three-line opener. We deliberately anchor on line
    // starts — phnt puts each keyword on its own line. Regex class escapes
    // are spelled out in ASCII form so we don't need the `unicode-perl`
    // feature of the regex crate (keeps the binary smaller).
    static STARTER: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    let starter = STARTER.get_or_init(|| {
        Regex::new(
            r"(?m)^[ \t]*(?:NTSYSCALLAPI|NTSYSAPI)[ \t]*\r?\n[ \t]*([A-Za-z_][A-Za-z0-9_]*)[ \t]*\r?\n[ \t]*NTAPI[ \t]*\r?\n[ \t]*([A-Za-z_][A-Za-z0-9_]*)[ \t]*\(",
        )
        .expect("compile starter regex")
    });

    let gates = scan_version_gates(&cleaned);
    let kernel_ranges = scan_kernel_mode_blocks(&cleaned);

    let mut out = Vec::new();
    for cap in starter.captures_iter(&cleaned) {
        // Every branch of the `let .. else` below gracefully skips the
        // match if (hypothetically) the regex structure changes and a
        // group disappears, instead of panicking in the middle of a
        // build-script-adjacent walk over hundreds of headers.
        let (Some(m0), Some(m1), Some(m2)) = (cap.get(0), cap.get(1), cap.get(2)) else {
            continue;
        };
        let match_start = m0.start();
        let ret_type = m1.as_str().to_string();
        let fn_name = m2.as_str().to_string();

        if !(fn_name.starts_with("Nt") || fn_name.starts_with("Zw")) {
            continue;
        }

        if kernel_ranges.iter().any(|r| r.contains(&match_start)) {
            continue;
        }

        let params_start = m0.end();
        let Some(params_str) = extract_param_list(&cleaned[params_start..]) else {
            warn!(fn_name, "could not extract parameter list");
            continue;
        };

        let params = parse_param_list(&params_str);

        let min_version = gates
            .iter()
            .find(|g| g.range.contains(&match_start))
            .map(|g| g.version.clone());

        out.push(PhntSignature {
            name: fn_name,
            return_type: ret_type,
            params,
            min_phnt_version: min_version,
            source: source.to_string(),
        });
    }
    out
}

// --- Parameter list parsing -----------------------------------------------

/// Read until the matching `)` for the opening one we've already consumed,
/// honoring nested parens. Returns the raw parameter-list text (without the
/// closing paren).
fn extract_param_list(s: &str) -> Option<String> {
    let mut depth: i32 = 1;
    let bytes = s.as_bytes();
    for (i, &b) in bytes.iter().enumerate() {
        match b {
            b'(' => depth += 1,
            b')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(s[..i].to_string());
                }
            }
            _ => {}
        }
    }
    None
}

/// Splits a parameter list by top-level commas (ignoring commas inside
/// nested SAL annotations).
fn split_top_level_commas(s: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut depth: i32 = 0;
    let mut start = 0usize;
    for (i, b) in s.bytes().enumerate() {
        match b {
            b'(' => depth += 1,
            b')' => depth -= 1,
            b',' if depth == 0 => {
                out.push(&s[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    out.push(&s[start..]);
    out.into_iter()
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .collect()
}

fn parse_param_list(s: &str) -> Vec<PhntParam> {
    let items = split_top_level_commas(s);
    let mut out = Vec::with_capacity(items.len());
    for item in items {
        if item == "VOID" || item == "void" {
            // `NtFoo(VOID)` — zero params, not a real arg.
            continue;
        }
        if let Some(p) = parse_one_param(item) {
            out.push(p);
        }
    }
    out
}

fn parse_one_param(raw: &str) -> Option<PhntParam> {
    // Peel off SAL annotations. They always start with an underscore and
    // come before the type; they may be simple (`_In_`) or wrapped
    // (`_At_(expr, ...)`), and there can be several in a row.
    let mut rest = raw.trim();
    let mut direction = Direction::Unknown;
    let mut optional = false;

    loop {
        rest = rest.trim_start();
        if rest.starts_with('_') && rest.len() > 1 {
            // Is this an annotation or the start of an identifier like
            // `__drv_freesMem(...)`?
            let tok_end = rest
                .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
                .unwrap_or(rest.len());
            let tok = &rest[..tok_end];
            // Recognize standard directional variants.
            let (dir, opt) = classify_sal(tok);
            if let Some(d) = dir {
                direction = d;
            }
            if opt {
                optional = true;
            }
            // If a `(` follows, skip the balanced group.
            let after = &rest[tok_end..];
            if let Some(stripped) = after.strip_prefix('(') {
                let mut depth = 1;
                let mut end = 0usize;
                for (i, b) in stripped.bytes().enumerate() {
                    match b {
                        b'(' => depth += 1,
                        b')' => {
                            depth -= 1;
                            if depth == 0 {
                                end = i + 1;
                                break;
                            }
                        }
                        _ => {}
                    }
                }
                rest = &stripped[end..];
            } else {
                rest = after;
            }
            continue;
        }
        break;
    }

    // Now `rest` should be `<type> <name>` (possibly with `*` stars).
    // In C, `PVOID *Name` is equivalent to `PVOID* Name` — the stars belong
    // to the type, the identifier is the parameter name. We always treat
    // the LAST whitespace-delimited token as the name (stripping any
    // leading stars that stuck to it), and every earlier token + those
    // stripped stars forms the type spelling.
    let tokens: Vec<&str> = rest.split_whitespace().collect();
    if tokens.is_empty() {
        return None;
    }
    let last_tok = *tokens.last()?;
    let star_count = last_tok.chars().take_while(|&c| c == '*').count();
    let raw_name = last_tok.trim_start_matches('*');
    let name = raw_name.trim_matches(|c: char| !c.is_ascii_alphanumeric() && c != '_');
    if name.is_empty() || !name.chars().next().is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
    {
        return None;
    }

    let type_head = tokens[..tokens.len() - 1].join(" ");
    let type_head = type_head.trim();
    if type_head.is_empty() {
        return None;
    }
    let stars_suffix = "*".repeat(star_count);
    let r#type = format!("{type_head}{stars_suffix}")
        .replace("* *", "**")
        .replace(" *", "*");

    // If no SAL prefix told us the direction, guess: pointers → inout,
    // values → in.
    if matches!(direction, Direction::Unknown) {
        direction = if r#type.ends_with('*') {
            Direction::Inout
        } else {
            Direction::In
        };
    }

    Some(PhntParam {
        name: name.to_string(),
        r#type,
        direction,
        optional,
    })
}

fn classify_sal(tok: &str) -> (Option<Direction>, bool) {
    // Normalize to the core SAL vocab.
    let t = tok;
    if t.starts_with("_In_opt") {
        (Some(Direction::In), true)
    } else if t.starts_with("_Inout_opt") || t.starts_with("_InOut_opt") {
        (Some(Direction::Inout), true)
    } else if t.starts_with("_Out_opt") {
        (Some(Direction::Out), true)
    } else if t.starts_with("_Inout") || t.starts_with("_InOut") {
        (Some(Direction::Inout), false)
    } else if t.starts_with("_Out") {
        (Some(Direction::Out), false)
    } else if t.starts_with("_In") {
        (Some(Direction::In), false)
    } else {
        // Unrecognized annotation (e.g. `_At_`, `__drv_freesMem`) — skip
        // without asserting a direction.
        (None, false)
    }
}

// --- Preprocessor scanning ------------------------------------------------

struct VersionGate {
    version: String,
    range: core::ops::Range<usize>,
}

/// Very lightweight `#if (PHNT_VERSION >= PHNT_XXX) ... #endif` matcher.
/// Handles only a single level of nesting (which is what phnt actually uses).
fn scan_version_gates(s: &str) -> Vec<VersionGate> {
    static RE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    let re = RE.get_or_init(|| {
        Regex::new(r"(?m)^[ \t]*#if[ \t]*\([ \t]*PHNT_VERSION[ \t]*>=[ \t]*(PHNT_[A-Za-z0-9_]+)[ \t]*\)")
            .expect("compile version-gate regex")
    });

    let mut out = Vec::new();
    for m in re.captures_iter(s) {
        let (Some(m0), Some(m1)) = (m.get(0), m.get(1)) else {
            continue;
        };
        let start = m0.end();
        let version = m1.as_str().to_string();
        if let Some(end) = find_matching_endif(s, start) {
            out.push(VersionGate {
                version,
                range: start..end,
            });
        }
    }
    out
}

fn scan_kernel_mode_blocks(s: &str) -> Vec<core::ops::Range<usize>> {
    static RE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    let re = RE.get_or_init(|| {
        Regex::new(r"(?m)^[ \t]*#ifdef[ \t]+_KERNEL_MODE([^A-Za-z0-9_]|$)")
            .expect("compile kernel-mode regex")
    });
    let mut out = Vec::new();
    for m in re.find_iter(s) {
        let start = m.end();
        if let Some(end) = find_matching_endif(s, start) {
            out.push(start..end);
        }
    }
    out
}

fn find_matching_endif(s: &str, from: usize) -> Option<usize> {
    let mut depth = 1_i32;
    let bytes = s.as_bytes();
    let mut i = from;
    while i < bytes.len() {
        // Match at start-of-line.
        if i == 0 || bytes[i - 1] == b'\n' {
            let tail = &s[i..];
            if tail.starts_with("#if") {
                depth += 1;
            } else if tail.starts_with("#endif") {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
        }
        i += 1;
    }
    None
}

// --- Comment stripping ----------------------------------------------------

/// Removes C block and line comments; leaves string literals and
/// quoted characters alone.
fn strip_comments(src: &str) -> String {
    let mut out = String::with_capacity(src.len());
    let bytes = src.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        let next = bytes.get(i + 1).copied();
        if b == b'/' && next == Some(b'/') {
            // line comment
            while i < bytes.len() && bytes[i] != b'\n' {
                i += 1;
            }
            // keep newline
        } else if b == b'/' && next == Some(b'*') {
            i += 2;
            while i + 1 < bytes.len() && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
                // preserve newlines so line numbers survive for diagnostics
                if bytes[i] == b'\n' {
                    out.push('\n');
                }
                i += 1;
            }
            i += 2.min(bytes.len().saturating_sub(i));
        } else if b == b'"' {
            out.push(b as char);
            i += 1;
            while i < bytes.len() {
                let c = bytes[i];
                out.push(c as char);
                i += 1;
                if c == b'\\' && i < bytes.len() {
                    out.push(bytes[i] as char);
                    i += 1;
                } else if c == b'"' {
                    break;
                }
            }
        } else {
            out.push(b as char);
            i += 1;
        }
    }
    out
}

// --- Helpers --------------------------------------------------------------

#[allow(dead_code)]
pub(crate) fn assert_nonempty(sigs: &[PhntSignature]) -> Result<()> {
    if sigs.is_empty() {
        bail!("phnt parser returned zero signatures — parser may be out of sync with headers");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_handles_nested_stars() {
        let text = "/*a*b*/ x // y\n z";
        let s = strip_comments(text);
        assert!(s.contains(" x"));
        assert!(s.contains("\n z"));
        assert!(!s.contains("*b*"));
    }

    #[test]
    fn parse_simple_nt_close() {
        let text = r#"
NTSYSCALLAPI
NTSTATUS
NTAPI
NtClose(
    _In_ HANDLE Handle
    );
"#;
        let sigs = parse_text(text, "test.h");
        assert_eq!(sigs.len(), 1);
        let s = &sigs[0];
        assert_eq!(s.name, "NtClose");
        assert_eq!(s.return_type, "NTSTATUS");
        assert_eq!(s.params.len(), 1);
        assert_eq!(s.params[0].name, "Handle");
        assert_eq!(s.params[0].r#type, "HANDLE");
        assert_eq!(s.params[0].direction, Direction::In);
        assert!(!s.params[0].optional);
    }

    #[test]
    fn parse_alloc_six_params() {
        let text = r#"
NTSYSCALLAPI
NTSTATUS
NTAPI
NtAllocateVirtualMemory(
    _In_ HANDLE ProcessHandle,
    _Inout_ _At_(*BaseAddress, _Readable_bytes_(*RegionSize)) PVOID *BaseAddress,
    _In_ ULONG_PTR ZeroBits,
    _Inout_ PSIZE_T RegionSize,
    _In_ ULONG AllocationType,
    _In_ ULONG PageProtection
    );
"#;
        let sigs = parse_text(text, "test.h");
        assert_eq!(sigs.len(), 1);
        let s = &sigs[0];
        assert_eq!(s.name, "NtAllocateVirtualMemory");
        assert_eq!(s.params.len(), 6);
        assert_eq!(s.params[1].name, "BaseAddress");
        assert!(s.params[1].r#type.ends_with('*'), "type={}", s.params[1].r#type);
        assert_eq!(s.params[1].direction, Direction::Inout);
    }

    #[test]
    fn honors_phnt_version_gate() {
        let text = r#"
#if (PHNT_VERSION >= PHNT_WINDOWS_10_RS5)
NTSYSCALLAPI
NTSTATUS
NTAPI
NtNewRs5Function(
    _In_ HANDLE H
    );
#endif
"#;
        let sigs = parse_text(text, "test.h");
        assert_eq!(sigs.len(), 1);
        assert_eq!(sigs[0].min_phnt_version, Some("PHNT_WINDOWS_10_RS5".into()));
    }

    #[test]
    fn skips_kernel_mode_blocks() {
        let text = r#"
#ifdef _KERNEL_MODE
NTSYSCALLAPI
NTSTATUS
NTAPI
NtDontShowUp(
    _In_ HANDLE H
    );
#endif
"#;
        let sigs = parse_text(text, "test.h");
        assert!(sigs.is_empty());
    }

    #[test]
    fn void_arg_list_is_empty() {
        let text = r#"
NTSYSCALLAPI
NTSTATUS
NTAPI
NtZero(
    VOID
    );
"#;
        let sigs = parse_text(text, "test.h");
        assert_eq!(sigs.len(), 1);
        assert_eq!(sigs[0].params.len(), 0);
    }

    #[test]
    fn opt_variants_mark_optional() {
        let text = r#"
NTSYSCALLAPI
NTSTATUS
NTAPI
NtOptFn(
    _In_opt_ HANDLE H,
    _Inout_opt_ PVOID *P
    );
"#;
        let sigs = parse_text(text, "test.h");
        assert_eq!(sigs.len(), 1);
        assert!(sigs[0].params[0].optional);
        assert!(sigs[0].params[1].optional);
        assert_eq!(sigs[0].params[0].direction, Direction::In);
        assert_eq!(sigs[0].params[1].direction, Direction::Inout);
    }
}
