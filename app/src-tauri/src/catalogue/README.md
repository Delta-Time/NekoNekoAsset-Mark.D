# Catalogue implementation notes

The catalogue owns local SQLite entities/projections, indexed search, overlays,
collections, spending and Automerge exchange. [ARCHITECTURE.md](../../../../ARCHITECTURE.md)
has the overall map. The requests, answers and record views the App reads are Rust
types in `api/`, generated into `app/src/bindings` by `pnpm --dir app bindings`;
test builds check the records written through `set_raw` against their views.

## Persistence and queries

SQLite stores source and overlay records separately, with FULL durability; selected
libraries use the DELETE rollback journal so they also work on shares.
`get/list/related` return raw entities; bootstrap/query/details return effective
presentation records. `put_many` validates a batch and projects affected products
once. `put_many_if_unchanged(records,expected)` atomically compares captured raw
inputs, including absent records, before writing; stale plans return false without
durable/revision changes. The monotonic local revision changes only for mutations.
Fresh files initialize the current schema; existing files must already match it.
There is no preference-key or schema migration.

`bootstrap {includeProducts:false}` and lightweight `state` avoid loading the
whole catalogue. `query {includeFiles:false,presentation:'list'}` returns summaries
with authoritative status/counts, `filesLoaded:false` and full ordered matched IDs.
`product_details {id,accountIds?}` loads full scoped data on demand. `account_counts`
uses distinct database product IDs; `allDownloaded` evaluates every matched ID.
Preferences use `preferences/app`.

Search normalizes NFKC/lowercase text, aliases, notes, original titles/authors and
filenames. Character grams bound CJK/substring candidates; exact checks reject
false positives. Indexed per-product lookups avoid scanning all file entities.
Title/acquired/price/status sorting keeps unknowns last. Acquisition-date order has
one home, `acquisition.rs`: an undated free download sorts by its `~`-prefixed place
in BOOTH's free history, after every date. Ready requires every
currently attached file to be usable at its current revision, independent of source closure;
partial downloads alone are not
Attention. Original archive availability is independent of Ready.

## Exchange and restore

Foreground edits atomically commit entities/projections plus durable `sync_outbox`
intents. One worker materializes ordered Automerge changes. It loads history only
when an intent is queued, and a load of more than 256 deltas rewrites the same
document as one checksummed checkpoint. Exchange, conflicts,
manifests and restore flush it first. Immutable checksum-named folder files are
published after complete writes; missing dependencies/partial files remain
retriable. Decode/serialization runs outside brief database locks and prepared
documents install only against matching generations. Concurrent writes merge
before commit. Same-field alternatives, tombstones and causal history survive
restart, repeated exchange and explicit conflict resolution.

Credentials, device paths/preferences, jobs and artifacts do not enter CRDT
exchange. Version-1 portable manifests retain validated metadata/media and a
complete checksummed Automerge snapshot with conflict alternatives; absolute root
mappings and authentication are excluded. CSV is supplementary and escapes
formula-leading cells. Restore validates everything first, creates a recovery
copy and atomically replaces records/projections/history. Ordinary metadata
restore marks artifacts unknown; only storage's independently verified payload
restore installs mappings and verified artifacts.

## Verification and provenance

Focused native checks cover deterministic persistence, atomic publication,
search/index bounds, overlays, membership/readiness, spending, cancellation,
restore and offline/restart/replay/conflict behavior. Synthetic isolated profiles
exercise multi-device logic without proving two physical PCs/NAS. Existing
ignored scale checks measure authored 10,000-product/300,000-file fixtures; they
are explicit measurement tools, not default gates or UI acceptance.

Original implementation; Automerge API reference: https://docs.rs/crate/automerge/0.7.4/source/.
No Automerge source was copied. Dependency notices are in
`app/src-tauri/resources/licenses/THIRD-PARTY-NOTICE.txt`; taxonomy sources are in
[taxonomy/README.md](../taxonomy/README.md).
