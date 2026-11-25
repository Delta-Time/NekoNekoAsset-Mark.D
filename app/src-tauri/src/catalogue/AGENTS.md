# Catalogue rules

- Default All excludes unclassified and effective NonVRChat products through
  Features › Hide Unclassified in All assets (default on); turning it off lists
  them. Acquisition and accessible files still publish to the database. Preserve
  manual category overrides and explicit category access.
- Features › Hide free assets that are only terms of use (default on) leaves out a
  BOOTH product held only through free downloads whose every file is the terms of
  use of a listing that also sells a paid variation (`terms_only.rs`). Hiding
  a real free asset is worse than showing a terms PDF, so anything less certain
  stays listed. Derived per list snapshot; nothing is rewritten.
- Custom categories are created and assigned only by the user (Edit, Batch Edit,
  category drops). Source inference, AI and local import never assign, change or
  remove them; deleting one returns its products to automatic classification.
- Prefer current prices bound to owned variants. When no owned current quote is
  available, show the public BOOTH starting price with an explicit "From" label; a
  local product read from its own link shows that one price without the label;
  never present it as an owned quote or paid amount. Spending totals stay per
  currency and use only proven paid facts.
- User names, thumbnails, notes, category and compatibility overrides and rejected
  tags survive source sync and AI.
- Compatible-avatar automatic defaults update only after all store sync and update
  jobs settle, never during scanning. Actual owned-base and matching-count rules
  apply then; explicit user on/off overrides stay separate and always win.
- Acquisition-date order lives in `acquisition.rs` and nowhere else.
