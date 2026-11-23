# Rust core rules

These cover the whole core. Folders below add their own `AGENTS.md`.

## Library, profiles and setup

- A selected library contains SQLite, media and managed files. One instance holds
  an OS/SMB lifetime lock (`library_lock.rs`) before opening SQLite; devices take
  turns, never concurrent NAS writes. No timeout or forced lock takeover.
- Device profiles keep the selected-library locator, protected credentials and AI
  chats.
- Setup confirms a default library candidate immediately, so account login or
  Offline does not require a manual chooser. Keep the candidate, accounts and
  preferences in memory; create library folders, SQLite and the locator only at
  Finish, then restart once before the initial metadata sync. Existing completed
  libraries keep their normal Open behavior. Recommend a spacious drive;
  auto-download defaults OFF.

## Jobs

- Activity lists jobs as notices for the user. Clear history deletes every finished
  job, so nothing may decide from one or from its absence: keep what work remembers
  in its own records (download intents, local sources, artifacts, checkpoints,
  coverage, accounts). A running task whose job row is gone was cancelled. A local
  import that stopped before adding all its assets only leaves Activity, since its
  Retry continues from what it copied; Discard in Storage drops it with the copies of
  the assets it did not add.

## Stores

`booth/` is BOOTH; `stores/` holds Gumroad, Jinxxy and what all three share. What
each store has been seen to answer is in `booth/ANSWERS.md`,
`stores/gumroad/ANSWERS.md` and `stores/jinxxy/ANSWERS.md`; add new observations
there with their date.

- BOOTH, Gumroad and Jinxxy have isolated multi-account sessions. Preserve protected
  device sessions across restart; reauthenticate only when needed. Names are
  editable; logout and deletion are distinct. A failed first login leaves no
  unusable account. Close verified login windows.
- Publish complete observed grants while later listing pages remain pending;
  associate available metadata, owned variants, paid facts and file descriptors
  before each commit. Later grants merge into the same product. Certify new or
  changed membership only after source closure; Ready depends on every currently
  attached file being usable, even while source coverage remains incomplete. Keep
  unchanged usable snapshots during refresh.
- Different providers run concurrently; accounts of the same provider queue. Show
  actual stages, separate listing and detail counts, useful errors and terminal
  results. Unknown fields remain unknown.
- Import › BOOTH: free assets adds listings through one signed-in account: only what
  the item page offers as a free download, never a paid variation, published as that
  account's free-history row with the time it was added, then downloaded. What it
  downloaded stays usable when the seller replaces, removes or deletes the item, as
  for any free download.
- Jinxxy's approved compiled `.3b` flow uses only its freshly issued memory-only
  token at the HTTPS endpoint from that same response, without redirects or
  logging.
