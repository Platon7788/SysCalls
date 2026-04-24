# vendor/

Vendored dependencies pinned for reproducibility.

## Planned contents

- **`phnt/`** — git submodule of [`winsiderss/phnt`](https://github.com/winsiderss/phnt)
  (MIT, typed NT API headers). Pinned commit. Parsed by `rsc-types` (Phase 4)
  into `db/phnt/phnt.toml`.

- **`pdb-snapshots/`** — committed, compressed PDB snapshots for selected
  Windows builds. Used in CI to verify reproducibility of `db/auto/*.toml`
  without requiring a live Microsoft Symbol Server.
  Storage format TBD (OQ-5: zip / lz4 / git-lfs).

## How to add phnt (pending Phase 0 / parallel task)

```
cd <repo-root>
git submodule add https://github.com/winsiderss/phnt SysCalls/vendor/phnt
cd SysCalls/vendor/phnt
git checkout <pin-commit>
cd ../../..
git add SysCalls/.gitmodules SysCalls/vendor/phnt
git commit -m "chore(vendor): pin phnt submodule at <short-sha>"
```

See `Docs/DECISIONS.md#D-15`.
