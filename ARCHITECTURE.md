# Architecture

This is a map of NekoNekoAsset for people who want to change it: where things live
and which rules the code keeps. The code says how. [CONTRIBUTING.md](CONTRIBUTING.md)
covers setup and checks.

## The problem

A VRChat creator buys assets on BOOTH, Gumroad and Jinxxy. Each store has its own
sign-in and pages, and none of them answers "what do I own, and where are the
files?" NekoNekoAsset signs in to each store as the buyer, reads what the accounts
own, downloads and extracts the files into one library folder, sorts them for
VRChat, and sends selected files to a running Unity Editor.

Everything runs on the user's computer. There is no server of ours, no telemetry
and no shared catalogue. A library folder may sit on a NAS and serve several PCs,
which take turns.

What makes it harder than it sounds:

- Store pages are the only source of truth, and they change without notice. A page
  the App does not understand must never read as "you own nothing".
- Owning a product, a store listing it, a gift and a free download are different
  facts with different evidence, and the App keeps them apart.
- The user's corrections (names, categories, tags, thumbnails, notes) must survive
  every later sync.
- Since 0.2.0, users own their libraries: every release opens the libraries
  earlier releases made, without a reset.

## Bird's-eye view

```text
React UI (app/src) --typed commands--> Rust core (app/src-tauri/src) --HTTPS--> BOOTH, Gumroad, Jinxxy
                   <--nna:changed----        |
                                             +--> library folder: SQLite catalogue + managed files
                                             +--loopback TCP--> Unity Bridge (unity/Packages/...)
```

The Rust core owns all data, store sessions, HTTP, jobs and files. The UI renders
what the core sends and asks it to act. Long work (sync, download, import, Unity
transfer) runs as durable jobs: the command returns a job ID at once, the job
records its progress in the catalogue, and an `nna:changed` event tells the UI what
to refresh. Jobs survive a restart and resume from their checkpoints.

## Code map

### `app/src-tauri/src/`: the Rust core

`lib.rs` starts the App: `setup_library` opens the selected library (or the setup
state before one exists), starts the workers and registers the Tauri commands.
Most commands carry `{action, args}`; `ipc::parse` reads them into one tagged enum
per service:

| Command | Request |
| --- | --- |
| `catalogue_action` | `catalogue::api::CatalogueRequest` |
| `booth_action` (all three stores) | `stores::api::StoreRequest` |
| `storage_action` | `storage::api::StorageRequest` |
| `unity_action` | `unity::api::UnityRequest` |
| `licenses_action` | `licenses::api::LicensesRequest` |
| `native_file_action` | `native::FileRequest` |
| `ai_action`, `ai_analysis_action`, `ai_suggestion_action` | `ai::AiRequest`, `ai::jobs::AiJobRequest`, `ai::suggestions::AiSuggestionRequest` |

Answers and events are Rust structs too (`ipc::Change` for every `nna:changed`).
They derive `ts_rs::TS`, and `pnpm --dir app bindings` writes their TypeScript to
`app/src/bindings`.

- `catalogue/` is the library database. `mod.rs` holds `Catalogue` (records,
  transactions, `put_many`, `put_many_if_unchanged`, work permits), `projection.rs`
  turns source records plus user overlays into the products the UI shows,
  `query.rs` filters, searches and sorts, `acquisition.rs` owns acquisition-date
  order, `avatars.rs` the compatible-avatar options, `spending.rs` the per-currency
  totals, `sync.rs` and `worker.rs` the Automerge folder exchange between devices,
  `portable.rs` backup and restore. `api/` declares the requests, answers and record
  views.
- `booth/` reads BOOTH. `mod.rs` holds `BoothService` and the download ticket,
  `parser.rs` and `free_item.rs` read pages, `http.rs` paces and retries requests,
  `auth.rs` signs in, `metadata.rs` reads public item data, `safety.rs` decides
  which hosts and URLs are allowed. `sync/` is the scan:
  `mod.rs` runs it as named stages (`scan_source_inner`), `records.rs` builds the
  typed records, `publication.rs` publishes products, `checkpoints.rs` resumes,
  `routes.rs` picks the next page, `free_history.rs` keeps the free-download list.
- `stores/` holds what the stores share and the other two stores. `mod.rs` holds
  `StoreService`, which routes store actions and issues download tickets, and the
  `StoreError` type. `gumroad/` is Gumroad (`mod.rs` parses, `service.rs`
  collects) and `jinxxy/mod.rs` is Jinxxy. `session.rs`, `session_cache.rs` and
  `native_cookies.rs` keep each account's protected cookies, `http.rs` builds
  clients, `public_metadata.rs` reads listings without an account.
- `storage/` writes files. `transfer.rs` downloads, `archive.rs` and
  `archive_process.rs` extract with the bundled 7-Zip, `local.rs` imports the user's
  own files and folders, `operations.rs` runs disk operations (backup payloads,
  library checks), `portable.rs` copies backup and restore payloads, `shared.rs`
  moves a library, `io.rs` holds `stable_error`, `unity_snapshot.rs` prepares
  files for Unity.
- `taxonomy/` classifies products (`infer_product_category`), resolves avatar
  names and tags, and holds the curated avatar identities (`avatars.json`).
- `unity/` finds running Editors (`discovery.rs`), speaks the wire protocol
  (`wire.rs`) and keeps transfer jobs and receipts (`jobs.rs`).
- `licenses/` reads VN3, UV License and CC0 notices into their own
  `licenses.sqlite3` for the optional license filter.
- `ai/` is the optional AI: provider entries, classification jobs, suggestions and
  the device-local chat store.
- The rest are single files: `native.rs` (file dialogs and file actions),
  `preview.rs` (thumbnail cache), `updates.rs` (App updater), `library_lock.rs`,
  `profile.rs` (the device profile: library locator and protected secrets),
  `setup_sessions.rs` (sign-in before a library exists), `app_reset.rs`,
  `window_state.rs`, `support.rs` (the copyable support report) and
  `path_safety.rs`.

Tests sit beside the code: `foo_tests.rs` next to `foo.rs`, or a `tests/` folder.
They use authored fixtures, never captured store pages.

### `app/src/`: the React UI

- `App.tsx` lays out the shell and calls one hook per area. Hooks live in
  `features/<area>/use*.ts` and take each other's result objects as parameters, so a
  hook's parameters list what it depends on. There is no global store, reducer or
  context.
- `backend.ts` is the door to Rust: `invokeAction(service, action, args)` is typed
  by the generated unions, so a misspelled action or a renamed field fails `tsc`.
  `ai-backend.ts` does the same for AI.
- `bindings/` is generated. Never edit it by hand.
- `model.ts` holds display rules: status badges, prices, effective tags and
  avatars, and the result grid's geometry. Filtering, search, sorting and
  categories happen in Rust (`catalogue/query.rs`, `taxonomy/`).
- `messages.ts` and `locales/<language>.json` hold UI text. `en.json` is the
  source, and `tsc` fails when another language lacks a key.
- `styles/<area>.css` are imported in cascade order by `main.tsx`; `shared/` holds
  small shared components.

### `unity/Packages/com.yank.nekonekoasset.bridge/`: the Unity Bridge

An Editor-only package. `BridgeRuntime.cs` publishes a registry file per Editor
and `BridgeServer.cs` listens on loopback; `BridgeImport.cs` and
`ImportReviewWindow.cs` show
Unity's import review before anything is written; `SmartPackage/` reads
`.unitypackage` archives. The release attests its bytes, so never reformat it.

### Elsewhere

- `scripts/`: `release.py` (release checks and source digest), `test_release.py`,
  `update-notices.py` (third-party notices) and `bindings.mjs`.
- `release/`: release settings and the maintainer's verification marker.
- `site/`: the GitHub Pages site. `.github/workflows/`: CI, release and Pages.

## Invariants

Each rule is kept by code and tests. Changing one is a design change, not a fix.

### Data

- Rust owns all state. The UI holds only what it was sent, and every change goes
  through a command.
- Records are stored as JSON in SQLite, and their shape is the compatibility
  contract. Rust types describe what the UI reads and what BOOTH's sync writes;
  test builds check each write against its type (`ipc::assert_view`,
  `catalogue::api::assert_record`), so a field nobody declared fails a test instead
  of vanishing on the next sync.
- One App process owns a library at a time. It takes the library's OS lock
  (`library_lock.rs`) before SQLite opens, and nothing ever takes the lock over.
- A persistent format change needs a versioned, data-preserving migration. No
  release may require a reset or silently move an old library.

### Facts

- Identity comes from store IDs (BOOTH item IDs, Gumroad and Jinxxy keys) and local
  IDs. Titles, file names and signed URLs never identify anything.
- Unknown stays unknown. A creation date is not an acquisition date, a listing's
  starting price is not what the user paid, and an order total is not a product's
  price.
- A page the parser does not recognize leaves its source incomplete. It never reads
  as an empty library or a finished sync.
- Public pages never establish ownership; only an account's own pages do.
- The user's edits are overlays on store records. Sync and AI never overwrite a
  title, note, thumbnail, category, compatibility choice or rejected tag the user
  set. Only the user assigns a custom category.

### Sync and files

- A product publishes as soon as its grant is complete and bound to its files and
  metadata, while later pages still load. A source's file membership is certified
  only when its listing closes.
- A product is Ready when every attached file is usable. Source coverage is a
  separate fact.
- Managed paths are readable and stable, and editing metadata never moves files.
  Extraction keeps the original on failure, keeps `.unitypackage` files and cleans
  up only what it wrote.
- A refused page proves nothing about the session. Only an account-level page on
  the store's own host (BOOTH's lists, Gumroad's library, Jinxxy's inventory) can
  ask the user to sign in again; a refused product or file fails only that asset.

### Code

- Store download errors are classified once, where they leave the stores:
  `stores::StoreError` carries the store's code and a `stores::DownloadError` class.
  Storage's `stable_error` reads that class, then its own codes matched whole, then
  io error kinds. Nothing matches message text.
- Acquisition-date order has one home, `catalogue/acquisition.rs`. An undated free
  download sorts by its place in BOOTH's free history, after every date.
- BOOTH's internal records are types (`booth/sync/records.rs`):
  `Acquisition::observe`, `RemoteFile::observe` and `Entitlement::listed` and
  `Entitlement::certified` build each record whole and name every field kept from
  the stored one. Publication signatures hash `SignedRow`, a frozen field set, so a
  new parser field does not republish every product.
- No production function passes 250 code lines (clippy's `too_many_lines`, set in
  `app/src-tauri/clippy.toml`). Long work is a row of named stages; stages that
  share many locals borrow them as one plain struct.

## Boundaries

- **IPC.** Only the main window has IPC. Store sign-in windows have none, refuse
  downloads and keep only their own store's cookies. Errors that cross IPC are
  stable codes with bounded context, never page bodies, cookies or signed URLs.
- **Store → catalogue.** A store adapter (`booth/`, `stores/gumroad/`,
  `stores/jinxxy/`) turns pages into acquisitions, remote files, entitlements and
  public metadata, and commits them in guarded transactions;
  `put_many_if_unchanged` refuses a write whose inputs another writer changed.
- **Store → storage.** For a download, `StoreService::download_ticket` issues a
  fresh signed URL and client that are never stored. Storage checks the response
  (`stores::validate_download_response`), then writes, verifies and extracts. What
  each store has been seen to answer is in `booth/ANSWERS.md`,
  `stores/gumroad/ANSWERS.md` and `stores/jinxxy/ANSWERS.md`.
- **App → Unity.** Wire protocol v1 is frozen: loopback TCP, a per-Editor registry
  file holding a random capability, length-prefixed JSON control frames, then raw
  file bytes, and a durable receipt per request ID. Additions must be optional, as
  `nativeImport` is.
- **AI.** Off by default. Only text and images the user selected go to the chosen
  provider, never cookies, archives or other records.

## Cross-cutting concerns

- **Jobs and concurrency.** Store, download, import, AI, Unity and exchange work
  hold a `catalogue::CatalogueWorkPermit` while queued or running; a restore takes
  the exclusive one. Different stores sync at the same time, while accounts of one
  store queue. Database locks are never held across network waits or long file
  work.
- **Errors.** Every error the UI sees is a stable code, translated through
  `error-i18n.ts` and the locale files.
- **Privacy.** Cookies, tokens, order details, page bodies and downloaded files
  never reach logs, Git or the support report. Store sessions are protected per
  account with Windows DPAPI.
- **Testing.** Rust tests and `pnpm --dir app check` (the UI logic checks in
  `app/checks`) run on authored data. Real-account checks are the maintainer's and
  never run in CI. Look at the rendered App for UI changes; a passing build says
  nothing about how it looks.
- **Platform.** Windows is the verified platform. macOS and Linux builds are
  untested.
