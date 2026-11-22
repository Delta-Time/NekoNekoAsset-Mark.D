# BOOTH native adapter

The production adapter uses original Rust parsing and native HTTP. It does not
launch a separate collector or generate catalogue fixtures at startup.

## Integration

Construct `BoothService::new(catalogue)` and call `start(app_handle)` once to
resume queued/interrupted jobs. `command(app, action, args)` handles account
login/logout/removal, source sync, cancellation/retry, selected-product updates and
adding free items by their listing (`import_free`). Login/sync/update/import return a
durable `jobId`; `nna:changed` announces committed changes. Durable records, rather
than events, remain authoritative.

The four source scopes are `library`, `purchase`, `gift` and `free`.
`sync` supports `incremental`, `full` and `forced`; `source:"all"` collects all
four. Selected updates change only selected products and do not claim a new
complete global account baseline.

## Authentication and HTTP

Each UUID account uses its own device-local `auth/booth/<UUID>` native profile.
A new window starts at `about:blank`; the shared session helper restores the
Windows DPAPI-protected cookie snapshot before navigating to the provider.
Only verified login captures first-party cookies. Cookie expiry is retained;
credentials never enter catalogue, checkpoints, portable metadata or logs.
Successful login keeps its native window hidden; failure/cancellation closes it.
Logout/removal clears the saved snapshot and native browsing data separately.

Only the trusted main window has IPC. Auth navigation permits the required
HTTPS BOOTH/Pixiv origins, rejects purchasing/download routes and disables
browser downloads. The parser rejects explicit signed-out provider metadata,
password/login forms and known authentication challenges.

One listing request and at most two detail requests share global request-start
pacing of 350–450 ms jitter (400 ms average, about 2.5 starts per second).
Attempts and Retry-After handling are bounded; final rate limiting stops the
pipeline. A product whose public page still fails with a network/server error
gets one later attempt after the public queue drains; if that fails, the product
stays pending and the sync ends `PUBLIC_METADATA_FAILED` instead of failing as a
whole. Incremental sync does not reread public pages already proven removed
(404/410); Full does. Source responses must retain the
requested account-source path/page before parsing. Failures return fixed codes,
never private bodies or signed URLs.

## Collection and publication

Source traversal and public enrichment are interleaved. One current public
observation is reused per product. Parsed rows and prepared product drafts are
journaled in the current job's checkpoints, outside visible source projection.
Verified free-item HTML contributes only allowlisted public support observations
to a same-job memory cache. Free-source metadata waits for that item's endpoint,
then still reads fresh JSON while reusing available support facts instead of
fetching the same HTML again. Other sources continue independently; cross-source
arrival and restart may require a fresh support page. Raw HTML is never retained.

An incremental Library scan with a recent complete baseline stops at a two-page overlap in
the default order, then reads `sort=file_updated_at_desc` for changed files until the rows
that led it last time appear, and reads every page whenever that does not fit. Only the page's own
`data-search-params` states an order. Incremental purchase scans read every order-list page; an order the last complete
purchase baseline already read reuses its rows (checkpoint `reused`, baseline
`details`) instead of a new receipt request. Full rereads every receipt. Records
whose only change is `observedAt` are not rewritten, and list events name only
products whose catalogue facts changed.

`sync/records.rs` builds acquisitions, remote files and entitlements whole
(`Acquisition::observe`, `RemoteFile::observe`, `Entitlement::listed`/`certified`) and
names every field a refresh keeps from the stored record; test builds check each
record the catalogue writes against its type. Publication signatures hash
`SignedRow` (`sync/publication.rs`), a frozen field set, so a new parser field does
not republish products; `metadata::PUBLIC_FIELDS` lists what publication copies.

Complete observed grants publish atomically after their bound file/variant facts
and available metadata are associated, while unrelated later pages remain pending.
Later grants merge into the same product; final file membership is certified only
after required source closure.
Acquisitions, file descriptors, entitlements and enriched product facts commit
in the same guarded transaction. An unrelated product's unresolved evidence
does not turn known fields into guesses; unidentified membership remains
incomplete. Refresh failures/cancellation retain the prior usable snapshot.

Public offering metadata does not establish ownership. Owned current prices
require exact owned-variant binding. Paid JPY values require a proven matching
product line, quantity/BOOST and safe arithmetic; true zero is retained.
Receipt totals and listing minima are never substituted for actual paid values.
Unknown unrelated receipt regions do not gate these needed-field proofs.

BOOTH's gift list shows no dates, and the item page's received notice is built in
the browser from the JSON sent to the signed-in account. An undated gift row reads
`items/<id>.json` with its own account's session before its listing page is
journaled, so `gift.accepted_at` publishes with the grant. Only that date is kept,
never the `gift.url` claim link. A date never changes, so a stored one costs no
request. A removed listing has no date to read: it stays unknown, is retried only
by Full like its public page, and never affects files or coverage.

Free items can be added by their listing (`sync/free_import.rs`). The import runs as
an Update of one account (`args.freeImport`), so it queues behind that account's
sync and resumes after a restart like one. For each listing in the order given it
reads the public JSON, then the item page with the account's session, as a free
item's Update does, and takes only what that page offers as a free download: a paid
variation has no download control there. A listing that offers nothing free, cannot
be read, is gone, or (while the terms-only list switch is on) offers only its terms
of use is left out and named in the job's `failures`. An added item publishes as the
account's free-history row, under the record ID BOOTH's free history gives it
(`free_history_record_id`), so the history a later sync reads merges with it. Its
`acquiredDateText` is the minute it was added and `acquiredAt` the millisecond, one
millisecond after the item before it at least, so items added together keep their
order; a later observation keeps both. A `download_intent` then downloads it like
Download does. An item the account holds already is only downloaded.

Account pages follow the BOOTH account's language and have no locale in their URLs;
public item pages and JSON follow the URL, which is always `/ja/`, so category names,
prices and gift dates do not move with the account. Nothing that decides a sync may
read translated account-page text. A list with nothing in it is a lone text panel
(`.bg-white.text-center`, or `.l-orders-index > .text-center` for orders, which then
has no link to itself either); reached past the first page it ends the list but counts
one unresolved, because a row may have moved past what was read. A list of one page has
a `.pager` element with nothing in it. A delisted row has a faded thumbnail
(`img.l-library-item-thumbnail` with an `opacity-` class) and no item link. Another
page shape stays `NO_RECOGNIZED_ROWS`, `PRODUCT_ID_UNRESOLVED` or terminal-unproven, so a
redesign leaves the source uncertified rather than empty or finished. Receipts keep
their summary labels: `注文番号`/`注文日時` in Japanese and `Order Number`/`Created At` in
the four other languages, and an unrecognized label only leaves that date unknown.

Only digital source fields are parsed. Unknown/invalid digital controls remain
unresolved. An audio track's format menu (`track-download-button`) is one file
only when every format resolves to it; the row's alternate-app menu binds its
variant. Album zip menus add no file when every entry is a card track, and album
alternate-app menus cannot add Library files. Tracks are named and downloaded as
MP3 (`type=mp3`); without exactly one MP3 choice the name stays unknown. Authenticated free-history rows prove historical account/product
ownership; complete offered-file membership requires separately bound item-page
evidence. Precisely bound closed paid-only offers or item 404/410 settle as
availability warnings. Their history acquisition and available metadata publish
without an empty entitlement/certification marker or changes to existing grants,
files and artifacts; free membership and coverage stay incomplete. Unknown
controls or unbound evidence still require action. An empty page or one control
alone does not certify all files.
Source/file identities never come from names. User titles, notes, tags,
classification/compatibility corrections and local artifacts survive sync.

Collection, enrichment, publication and finalization progress are durable.
Terminal job/account/source results are recorded; `lastSync` advances only on
success. Current-job restart rereads source boundaries/head evidence and checks
cancellation. There are no collector imports, schema upgrades, automatic data
moves or standalone backfills. Manual Move library location remains a storage
operation.

## Download boundary and verification

Tickets resolve the unsigned numeric file route afresh with its bound variant.
Signed final URLs and HTTP clients are transient and non-serializable. Storage
validates the actual response, bytes and extraction before declaring Ready.
Metadata completeness alone does not establish usable local files. An error leaving
the ticket or the response check becomes a `stores::StoreError`, classified once by
its code. What BOOTH has been seen to answer, including a removed file refused like
a signed-out request, is in [ANSWERS.md](ANSWERS.md).

Focused native tests use independently authored parser/catalogue fixtures for
binding, unknown evidence, zero/quantity/BOOST, ownership, publication, replay,
cancellation and overlay retention. They do not certify a real account scan,
login/MFA or asset download. Real-account checks are the maintainer's; personal
records and credentials never enter fixtures, tests or logs.
