# rsc-types

> Stub. Наполняется по мере реализации **Phase 4**.

## Назначение

Парсер phnt-headers → `db/phnt/phnt.toml`. Производит typed signatures overlay для canonical DB.

## Отвечает за

- Чтение C-хедеров из `vendor/phnt/` (git submodule)
- Regex/tokenizer парсинг `NTSYSCALLAPI NTSTATUS NTAPI NtXxx(...)`
- Извлечение SAL annotations (`_In_`, `_Out_`, `_Inout_`)
- Нормализация phnt-типов в canonical-типы (через mapping table)
- Обработка версионных макросов (`PHNT_VERSION`)
- TOML emission (phnt-layer schema)

## Не отвечает за

- PDB, SSN, RVA — это `rsc-collector`
- Merge — это `rsc-cli`

## Структура

```
rsc-types/
├── Cargo.toml
└── src/
    ├── main.rs             — CLI: parse → emit
    ├── parser.rs           — C-header regex/tokenizer
    └── normalizer.rs       — type mapping table
```

## Зависимости

- `regex` — primary parsing tool (headers достаточно регулярны)
- `toml` + `serde` (output)
- `anyhow` (errors)

Альтернативно может использоваться `tree-sitter-c` — но regex достаточен для phnt format.

## Type mapping

Hardcoded таблица `phnt_type_name → rsc_canonical_type`:

```rust
const TYPE_MAP: &[(&str, &str)] = &[
    ("PHANDLE",       "*mut HANDLE"),
    ("PVOID",         "*mut c_void"),
    ("PULONG",        "*mut u32"),
    ("PSIZE_T",       "*mut usize"),
    ("PUNICODE_STRING", "*mut UNICODE_STRING"),
    // ... hundreds
];
```

Отсутствующий тип → WARN + opaque fallback (`*mut c_void`).

## Версионные gates

phnt использует `#if (PHNT_VERSION >= PHNT_WIN7)` и т.п. Парсер должен:
1. Сохранять min_phnt_version в output
2. Не падать на nested conditionals
3. Игнорировать `#ifdef _KERNEL_MODE` блоки (мы user-mode)

## Обновление phnt

1. `cd vendor/phnt && git pull && git checkout <new-commit>`
2. `cargo run -p rsc-types`
3. `cargo run -p rsc-cli -- verify` — проверить что не сломалось
