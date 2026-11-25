# What Jinxxy answers

Only answers someone has seen are listed. "Unknown" means nobody has observed it, so
the code's handling there is a guess (last section). When you see a new answer, add it
with the date and how you saw it.

| Request | Success | Refused | Removed | Signed out, or session cookies missing |
| --- | --- | --- | --- | --- |
| Inventory and its detail pages (`jinxxy.com/my/inventory`, `/my/inventory/<id>`) | 200 HTML carrying the page's Apollo SSR data | Unknown | Unknown | Unknown |
| File endpoint (`api.jinxxy.com/v1/downloads/inventory/<id>/files/<file>`) | 302 to a signed location on a public HTTPS host outside Jinxxy | Unknown | Unknown | 302 to Jinxxy's sign-in page (`/login` with a query, on another Jinxxy host) |
| That signed location | A success status with the whole file, its size the inventory's | Unknown | Unknown | Unknown |
| Compiled `.3b` download | Unknown | Unknown | Unknown | Unknown |

## Evidence

- Release gates read the inventory and detail pages above and publish from them.
- 2026-11-23, download check and probes of the file endpoint (no redirect followed, no
  body read): sent the cookies the sign-in browser holds for `jinxxy.com`, HEAD and GET
  both answered 302 to a public non-Jinxxy location; sent only the cookies scoped to
  `api.jinxxy.com`, both answered 302 to the sign-in page, with or without
  `Referer`/`Origin`. The App sent only the latter until then.
- 2026-11-23, download check once the App sent the `jinxxy.com` cookies: two owned
  products, one zip each, downloaded at the inventory's sizes and extracted, with a
  `.unitypackage` in each.
- No compiled `.3b` download has completed yet; that flow was built from Jinxxy's
  public JavaScript.

## How the code reads answers

- Only the inventory can say the account is signed out (`AUTH_REQUIRED`). A detail page,
  the file endpoint or a `.3b` request answering sign-in fails only that asset
  (`DOWNLOAD_RESOLVER_FAILED`), and the next ticket checks the account again.
- File endpoint (`resolved_location` in `mod.rs`): the request carries the cookies of the
  buyer's inventory page (`endpoint_jar`). The sign-in browser keeps them to the pages'
  host, so the shared jar never sends them to `api.jinxxy.com`.
  A redirect whose `Location` is an allowed download host gives the ticket; the sign-in
  page is a sign-in answer; another status is `JINXXY_DOWNLOAD_UNAVAILABLE`, a missing
  `Location` `JINXXY_DOWNLOAD_UNPROVEN`, and another disallowed host
  `UNSAFE_DOWNLOAD_URL`.
- Signed location (`validate_download_response`): 401 or 403 is
  `SIGNED_LINK_EXPIRED`, 404 or 410 `FILE_UNAVAILABLE`, 429 `RATE_LIMITED`, 5xx
  `DOWNLOAD_RESOLVER_FAILED`, any other failure `JINXXY_DOWNLOAD_UNAVAILABLE`.
