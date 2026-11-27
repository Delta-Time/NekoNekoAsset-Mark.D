# Storage rules

- Product download includes all accessible files and variants, old offered files,
  source files, PSDs and documentation. Explicit single-file download is supported.
  Partial download is normal; Attention is for genuine failures and actionable
  issues.
- Downloaded requires every currently attached file to be usable (a file its store
  refused as no longer offered counts as dropped); source membership coverage stays
  a separate fact.
- Auto-extract supported archives, preserve internal names and structure, and keep
  originals on failure. Delete eligible originals only after validated durable
  extraction; keep unitypackages. Use readable stable paths; metadata edits do not
  move files. Clean only outputs owned by the operation.
- An archive password the user types serves only that file's next extraction. It
  reaches 7-Zip on its input, never the command line, and never enters a job, the
  catalogue or a log.
- Local files, folders and ZIPs are copied into managed storage without changing
  the originals. The default one-product mode accepts many inputs; batch mode has
  independent folder and ZIP product drafts. Loose files share a product directory.
- Local URL Fetch, and the current price read after an import or a link change in
  Edit, supply metadata, never account ownership. That read never delays anything,
  any failure stays unknown, and a price shows only beside the listing it came from.
- Store errors arrive classified (`stores::StoreError`); `io.rs`'s `stable_error`
  never matches message text.
- A request that removes or copies files only queues a job and answers at once. The
  storage worker runs one job at a time: removals of downloaded files, then local
  imports, go after the download in progress and before queued downloads.
