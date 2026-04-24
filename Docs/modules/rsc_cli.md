# rsc-cli

> Stub. Наполняется по мере реализации **Phase 5**.

## Назначение

Унифицированный CLI-бинарь `rsc` с subcommands. Оркестрация всех остальных крейтов + merge/verify/diff операции над БД.

## Subcommands

| Команда | Что делает |
|---|---|
| `rsc collect [--force]` | Wrapper над `rsc-collector` |
| `rsc phnt-parse` | Wrapper над `rsc-types` |
| `rsc merge` | auto + phnt + overrides → canonical.toml |
| `rsc verify [--strict]` | Integrity + drift checks |
| `rsc diff <A> <B>` | Показать разницу между build'ами |
| `rsc stats` | Coverage / category stats |

## Отвечает за

- Merge-логика: read all layers → apply priority → emit canonical
- Verify: проверка целостности, дрейф, overrides на несуществующие функции
- Diff: alphabetical compare двух auto/*.toml
- Stats: подсчёт по категориям, % coverage phnt

## Не отвечает за

- Фактический сбор PDB (делегирует в rsc-collector)
- Парсинг phnt (делегирует в rsc-types)
- Генерацию runtime кода (это build.rs в rsc-runtime)

## Структура

```
rsc-cli/
├── Cargo.toml
└── src/
    ├── main.rs                    — CLI dispatch (clap)
    └── commands/
        ├── mod.rs
        ├── collect.rs             — delegation
        ├── phnt_parse.rs          — delegation
        ├── merge.rs               — MAIN logic
        ├── verify.rs              — MAIN logic
        ├── diff.rs                — MAIN logic
        └── stats.rs               — MAIN logic
```

## Зависимости

- `clap` (v4, derive) — CLI parsing
- `toml` + `serde` — read/write БД
- `anyhow` — errors
- rsc-collector, rsc-types (опционально, как library deps для delegation — либо spawn subprocess)

## Merge алгоритм (скелет)

```
fn merge(db_dir: &Path) -> Canonical {
    let baseline = load_latest_auto(db_dir);
    let phnt = load_phnt(db_dir);
    let overrides = load_overrides(db_dir);

    let mut out = Canonical::new();

    for auto_fn in baseline.functions {
        // Step 1: start with auto
        let mut entry = from_auto(&auto_fn);

        // Step 2: overlay phnt types
        if let Some(ph) = phnt.get(&auto_fn.name) {
            entry.apply_types_from(ph);
        } else {
            entry.mark_opaque();
        }

        // Step 3: apply overrides
        for ov in overrides.iter_for(&auto_fn.name) {
            entry.apply_override(ov);
        }

        if !entry.excluded {
            out.push(entry);
        }
    }

    // Step 4: apply `add`-kind overrides
    for ov in overrides.adds() {
        out.push(Entry::from_add_override(ov));
    }

    // Step 5: compute rsc_hash
    for entry in &mut out {
        entry.rsc_hash = rsc_hash(&entry.name);
    }

    out
}
```

## Verify checks

- [ ] Все `db/auto/*.toml` парсятся без ошибок
- [ ] Phnt submodule на коммите, указанном в phnt.toml meta
- [ ] Overrides.toml ссылается только на существующие функции (кроме `kind=add`)
- [ ] Нет дубликатов в canonical
- [ ] Все SSN в диапазоне 0x0000..0x0FFF
- [ ] Все arity в 0..15
- [ ] Нет коллизий хешей
- [ ] `--strict` mode: выходит 1 также на WARN'ах
