# rsc-collector

## Назначение

CLI-инструмент сборки ground-truth snapshot'а ntdll.pdb с Microsoft Symbol Server
для текущей Windows build. Автоматически находит ntdll (x64 native + x86 WoW64),
скачивает PDB, извлекает `Zw*` функции через DbgHelp, дизассемблирует stubs для
SSN + arity, эмиттит `db/auto/<build>.toml`.

## Статус

✅ **Phase 3 завершена** (session 005, 2026-04-24). Live run на Win10 22H2
(build 19045.6466) успешно собрал **473 функции x64 + 492 функции x86 WoW64**.

## Структура (1454 строк Rust, 8 файлов)

```
crates/rsc-collector/
├── Cargo.toml
└── src/
    ├── main.rs          — CLI (clap), orchestration, logging (~279 строк)
    ├── error.rs         — CollectError через core::error::Error (~61 строк)
    ├── version.rs       — WindowsBuild detect через registry (~81 строк)
    ├── pe.rs            — PE parse + CodeView RSDS + SHA-256 (~278 строк)
    ├── symsrv.rs        — HTTP + cache + CAB unpack (~204 строк)
    ├── pdb_reader.rs    — DbgHelp FFI + RAII session + TLS sink (~215 строк)
    ├── stub_disasm.rs   — SSN + arity из байтов (~173 строк, 4 теста)
    └── emit.rs          — TOML serialize per DATABASE.md (~163 строк, 2 теста)
```

## Пайплайн

```
1. version::WindowsBuild::detect()
   HKLM\SOFTWARE\Microsoft\Windows NT\CurrentVersion
     → CurrentMajorVersionNumber + CurrentMinorVersionNumber
     → CurrentBuildNumber (REG_SZ) + UBR (DWORD)
   → build_id = "{Major}_{Build}_{UBR}"

2. pe::read_ntdll(path)
   C:\Windows\System32\ntdll.dll  (x64 native)
   C:\Windows\SysWOW64\ntdll.dll  (x86 WoW64)
     ↓ parse MZ/PE → DataDirectory[DEBUG] → IMAGE_DEBUG_DIRECTORY[CODEVIEW]
     ↓ decode RSDS: GUID (16B) + Age (4B) + PDB name (null-terminated ASCII)
     ↓ SHA-256 of the whole image
   → ImageMeta { bytes, path, file_size, sha256, pdb: PdbRef }

3. symsrv::SymSrv::fetch(&pdb_ref)
   URL: https://msdl.microsoft.com/download/symbols/{name}/{GUID}{AGE}/{name}
     ↓ User-Agent: Microsoft-Symbol-Server/10.0.0.0
     ↓ Cache: %APPDATA%\rsc\cache\{name}\{GUID}{AGE}\{name}
     ↓ CAB detection (MSCF magic) → expand.exe -F:*
     ↓ Atomic write
   → PathBuf (locally cached PDB)

4. pdb_reader::PdbSession::open(pdb_path)
   → DbgHelp SymInitializeW + SymLoadModuleExW
   session.enumerate("Zw*")
   → SymEnumSymbolsW + TLS sink pattern
   → Vec<ZwSymbol { name, rva }>

5. For each Zw* symbol:
   nt_name = "Nt" + sym.name[2..]  (rsc-runtime hashes "Nt*")
   stub_bytes = image.bytes[rva_to_file_offset(rva) .. +32]
   stub_disasm::disasm_x64/x86(stub_bytes)
     → StubInfo { ssn, arity, bytes_hex }

6. emit::AutoSnapshot.upsert(nt_name, |e| e.ssn_xXX = ...)
   snapshot.sort()
   emit::write_atomic("db/auto/{build_id}.toml")
```

## CLI

```
rsc-collector [OPTIONS]

OPTIONS:
  --force                    Re-download PDB and regenerate even if file exists
  --arch <ARCH>              x64 | x86 | both (default: both)
  --build-id <ID>            Override auto-detected (for testing)
  --db-dir <PATH>            Output directory (default: db/auto)
  -v, --verbose              Increase log verbosity (repeat: -v/-vv/-vvv)
  -h, --help                 Show help
```

Логи через `tracing` + `tracing-subscriber`. Env override: `RSC_LOG=debug`.

## Deps (per maintainer dependencies doc)

| Crate | Назначение |
|---|---|
| `windows` 0.62 | DbgHelp FFI (SymInitializeW, SymLoadModuleExW, SymEnumSymbolsW) |
| `ureq` 3 | sync HTTP client с rustls |
| `winreg` 0.56 | Registry для UBR/Major/Minor/Build |
| `iced-x86` 1.21 | (reserved for x64 arity heuristic — Phase 4+) |
| `sha2` 0.10 | SHA-256 ntdll.dll для meta поля |
| `time` 0.3 | RFC3339 timestamps |
| `toml` + `serde` | TOML (de)serialization |
| `clap` 4 | CLI |
| `tracing` + `-subscriber` | logging |
| `anyhow` | top-level error propagation |

## Ключевые технические детали

### GUID формат для Symbol Server URL

PE хранит CodeView GUID: Data1/2/3 в little-endian, Data4 в network order.
URL требует **network byte order** (big-endian hex без дефисов):
- Data1: bytes reversed (4 → 1)
- Data2: bytes reversed (2 → 1)
- Data3: bytes reversed (2 → 1)
- Data4: as-is

Пример: file bytes `18 0B F1 B9 0A A7 97 56 D0 EF EA 5E 56 30 AC 7E`
→ URL: `B9F10B18A70A5697D0EFEA5E5630AC7E` + `{AGE uppercase hex}`

См. `pe::PdbRef::guid_hex()`.

### x86 WoW64 имеет `wntdll.pdb`, не `ntdll.pdb`

PDB name берётся из CodeView entry самой DLL — `SysWOW64\ntdll.dll` содержит
ссылку на `wntdll.pdb` (W = WoW64). Symbol Server URL автоматически правильный.

### SSN на WoW64 — raw encoding

Stub вида `B8 0F 00 03 00` = `mov eax, 0x0003000F`. Высокие 16 бит = service
table (3 = WoW64 gate dispatch), низкие 16 = индекс. Мы сохраняем **raw** значение
(196623 для NtClose) — именно это EAX должно содержать при вызове WoW64 gate.

### DbgHelp RAII

`PdbSession` обёртывает `SymInitialize` в конструкторе и `SymCleanup` в `Drop`.
Гарантирует корректный cleanup даже при panic.

TLS sink (`thread_local! static SINK`) передаёт символы из DbgHelp callback'а
в Rust-код без `&mut` через `RefCell`. Работает потому что DbgHelp
enumerates synchronously на вызывающем потоке.

### Первый Zw* был `jmp`

На Win10 22H2 первый `Zw*` имеет prologue `E9 ...` (jmp) вместо стандартного
`4C 8B D1 B8`. Вероятно CFG / hot-patch forwarder. Fallback `search_ssn_by_mov_eax`
находит `B8` в первых 16 байтах и извлекает SSN.

### Modern Rust (1.95) используется

- `std::sync::LazyLock<PathBuf>` для `CACHE_ROOT` (стабилизировано 1.80)
- `let ... else` для ранних return'ов (стабилизировано 1.65)
- `core::ptr::without_provenance_mut` для synthetic handle (стабилизировано 1.84)
- `nn.is_multiple_of(4)` вместо `nn % 4 == 0` (стабилизировано 1.87)
- `core::error::Error` trait (в core с 1.81)

## Live результат (Win10 22H2, build 19045.6466)

```
[*] build_id=10_19045_6466 label="10 (build 19045.6466)"
[*] x64: C:\Windows\System32\ntdll.dll → 473 Zw* functions
    PDB guid=180BF1B9-0AA7-5697-D0EF-EA5E5630AC7E age=1
[*] x86: C:\Windows\SysWOW64\ntdll.dll → 492 Zw* functions
    PDB guid=A9F8BCE9-412D-59F7-716E-6C8DB6CB6F3F age=1 (wntdll.pdb)
[*] snapshot: db/auto/10_19045_6466.toml (110 KB, 492 [[syscall]] entries)
```

Примеры entries:
- `NtAllocateVirtualMemory`: ssn_x64=24, ssn_x86=24, arity_x86=6
- `NtClose`: ssn_x64=15, ssn_x86=196623 (WoW64 raw = 0x0003000F), arity_x86=1

Эти SSN совпадают с legacy Win10/Win11 значениями — pipeline работает корректно.

## Тесты

- `cargo test -p rsc-collector` — **9 passed**:
  - stub_disasm (4): canonical x64/x86 stubs + 6-arg case + hex upper
  - emit (2): upsert merging + TOML skips Option<None>
  - symsrv (2): cache root location + cache path layout
  - pdb_reader (1): wide string terminator

## Что дальше (Phase 3b / backlog)

- x64 arity heuristic через iced-x86 (пока arity_x64 = None, phnt overlay заполнит в Phase 4-5)
- Обработка нестандартных prologue (CFG / hot-patch)
- Несколько CodeView entries в DataDirectory (берём первый)
- Retry при network failure
- CLI flag `--dry-run` (показать что собирается без записи)
