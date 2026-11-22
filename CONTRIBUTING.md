# Contributing to NekoNekoAsset

Original project code is MIT licensed. Open a GitHub issue before a substantial
product or dependency change. Keep the existing compact interface and workflow, and
preserve third-party licenses, notices and stable Unity `.meta` GUIDs.

## Set up

Windows x64 is the App's platform. Install Visual Studio 2022 Build Tools with
Desktop development with C++, the WebView2 Runtime, Node.js **24.19.0**, pnpm
**11.25.0**, Python **3.14.8**, and Rust through rustup; `rust-toolchain.toml`
selects Rust **1.99.0** with rustfmt and clippy. Use the committed lockfiles.

```powershell
pnpm --dir app install --frozen-lockfile
pnpm --dir app tauri dev
```

A debug build shares the installed App's identifier, so quit an installed
NekoNekoAsset first (only one instance runs), and by default it also shares its
device profile: the library choice and store sign-ins. Set `NNA_DATA_DIR` to an
empty folder to give a debug build its own profile:

```powershell
$env:NNA_DATA_DIR = "$env:TEMP\nna-dev-profile"; pnpm --dir app tauri dev
```

Develop with a test library and authored assets. Never commit or attach personal
catalogue records, account or order details, cookies, tokens, keys, browser
profiles or downloaded asset files; keep diagnostics aggregate and redact personal
values from screenshots and logs. `pnpm --dir app tauri build --no-bundle` makes an
unsigned local build.

## Where the code lives

[ARCHITECTURE.md](ARCHITECTURE.md) maps the code and the rules it keeps. In short:

| To change | Look in |
| --- | --- |
| BOOTH sync and downloads | `app/src-tauri/src/booth/` |
| Gumroad, Jinxxy | `app/src-tauri/src/stores/gumroad/`, `app/src-tauri/src/stores/jinxxy/` |
| What a store answers | `ANSWERS.md` beside each store's code |
| Downloads, extraction, local import, library moves | `app/src-tauri/src/storage/` |
| Search, filters, sorting, what a product shows | `app/src-tauri/src/catalogue/` (`query.rs`, `projection.rs`) |
| Categories, avatar names and tags | `app/src-tauri/src/taxonomy/` |
| Unity transfer | `app/src-tauri/src/unity/` and `unity/Packages/com.yank.nekonekoasset.bridge/` |
| License filter | `app/src-tauri/src/licenses/` |
| AI | `app/src-tauri/src/ai/` and `app/src/features/ai/` |
| A screen or panel | `app/src/features/<area>/` and `app/src/styles/<area>.css` |
| UI text | `app/src/locales/<language>.json` |
| A request or answer between the UI and Rust | the service's `api` module in Rust, then `pnpm --dir app bindings` |

## Making a change

- **UI text.** `app/src/locales/en.json` is the source; a new key goes into every
  language file, and `pnpm build` fails when one lacks it. Keys are
  `<table>.<key>`; `app/src/messages.ts` lists which lookup function reads each
  table. A translation fix touches only its language's file.
- **Data between Rust and the UI** is declared once, in Rust: a service's actions are
  one tagged enum (such as `catalogue::api::CatalogueRequest`) and the records the UI
  reads are structs (such as `catalogue::api::Product`). Run
  `pnpm --dir app bindings` to regenerate their TypeScript in `app/src/bindings` and
  commit it with the Rust change, so `pnpm build` fails wherever the UI still uses an
  old name.
- **Saved data.** Since **0.2.0 Beta**, installed users' libraries are a
  compatibility contract. Avoid database, schema and storage-format changes unless
  necessary. A necessary incompatible change needs an explicit, versioned migration
  from supported earlier public versions, safe failure handling and focused checks
  that the data survives and reopens. Never silently recreate, reset or discard a
  user's database. An App update keeps the catalogue, library paths, user edits,
  settings and protected sessions.
- **Store behavior.** Test parsers on pages you write yourself, never on captured
  ones. When you observe a store answer nobody recorded, add it to that store's
  `ANSWERS.md` with the date.
- **Tests.** Keep meaningful existing tests and never weaken a check to make it
  pass. Add focused tests for deterministic logic with real regression risk; Rust
  tests sit beside the code (`foo_tests.rs` or a `tests/` folder). No new test
  framework is needed.
- **UI.** Look at the rendered App: relevant widths, scales, languages, light and
  dark themes, keyboard focus, loading and error states. A passing build is no proof
  of how it looks.
- **Formatting.** `pnpm --dir app format` and
  `cargo fmt --all --manifest-path app/src-tauri/Cargo.toml` before committing.
  Never reformat `unity/Packages`, whose bytes the Bridge release attests.
  Formatting-only commits are listed in `.git-blame-ignore-revs`; run
  `git config blame.ignoreRevsFile .git-blame-ignore-revs` to skip them in blame.
- **Dependencies.** A new Rust or npm dependency changes a lockfile that the
  third-party notices attest: run `python scripts/update-notices.py` and commit its
  output.

## Checks

From the repository root:

```powershell
pnpm --dir app exec biome ci
pnpm --dir app build
cargo fmt --all --check --manifest-path app/src-tauri/Cargo.toml
cargo clippy --locked --all-targets --manifest-path app/src-tauri/Cargo.toml -- -D warnings
cargo test --locked --lib --manifest-path app/src-tauri/Cargo.toml
pnpm --dir app bindings
python scripts/release.py check
python scripts/update-notices.py --check
python scripts/test_release.py
python unity/scripts/package_bridge.py unity/Packages/com.yank.nekonekoasset.bridge build/bridge
```

After `pnpm --dir app bindings`, `git status app/src/bindings` must show nothing.

## What CI enforces

Every push to `main` and every pull request runs `.github/workflows/ci.yml`:

- release inputs and logic: `scripts/release.py check` (which also refuses tracked
  internal or private files), the third-party notices check and
  `scripts/test_release.py`;
- `biome ci` (formatting and lint of `app/`) and `pnpm build` (`tsc` and Vite);
- `cargo fmt --check`, and clippy with `-D warnings`, which fails any production
  function over 250 code lines (`app/src-tauri/clippy.toml`);
- the Rust library tests;
- generated bindings: CI reruns `scripts/bindings.mjs` and fails when
  `app/src/bindings` differs from what is committed;
- the Unity Bridge packager, which checks archive bytes, inventory and GUIDs without
  a Unity license. It does not prove a real Unity import.

CI never sees store accounts. Real-account and real-Unity checks are the
maintainer's, before each release: see [RELEASING.md](RELEASING.md).
