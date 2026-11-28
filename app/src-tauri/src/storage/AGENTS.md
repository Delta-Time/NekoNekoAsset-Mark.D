# Storage rules

- Product download includes all accessible files and variants, old offered files,
  source files, PSDs and documentation. Explicit single-file download is supported.
  Partial download is normal; Attention is for genuine failures and actionable
  issues.
- Downloaded requires every currently attached file to be usable (a file its store
  refused as no longer offered counts as dropped); source membership coverage stays
  a separate fact.
- Auto-extract ZIP archives, preserve internal names and structure, and keep
  originals on failure. A name Windows cannot store (`a:b`, `Folder.`, `aux.png`)
  lands under the name 7-Zip writes for it, in archives and local imports alike, and
  the rest extract as usual. Delete eligible originals only after validated durable
  extraction; keep unitypackages. Use readable stable paths; metadata edits do not
  move files. A new folder name keeps 32 characters of its title; recorded paths
  never change. Clean only outputs owned by the operation.
- Verify archive passwords against the contents before optional plaintext catalogue storage.
  Bind them to the file and content hash. RAR and 7z stay available for manual extraction.
  Pass passwords on 7-Zip's standard input and keep them out of jobs and logs.
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
  imports, go after the download in progress and before queued downloads. A local
  import holds storage shared, as a download does, so requests made while it copies
  queue too; moves, backups and deleting local assets wait for it or answer busy.
