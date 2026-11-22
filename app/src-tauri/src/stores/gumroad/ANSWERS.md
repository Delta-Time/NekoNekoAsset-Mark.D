# What Gumroad answers

Only answers someone has seen are listed. "Unknown" means nobody has observed it, so
the code's handling there is a guess (last section). When you see a new answer, add it
with the date and how you saw it.

| Request | Success | Refused | Removed | Signed out |
| --- | --- | --- | --- | --- |
| Library (`gumroad.com/library`) | 200 HTML whose first `data-page` payload is the Inertia component `Library/Index` | Unknown | Unknown | The login page, component `Logins/New` |
| Purchase content (`/d/<id>`) | 200 HTML, component `UrlRedirects/DownloadPage` | Unknown | Unknown | Unknown |
| File link on that page | A redirect whose `Location` is the file on a public HTTPS host | Unknown | Unknown | Unknown |
| That file location | 200 with the bytes | Unknown | Unknown | Unknown |

## Evidence

- Release gates read the library and content pages above and publish from them.
- A protected request from a signed-out session ends on the login page
  (`Logins/New`), as the parser notes. The 0.3.11 gate's first attempt found a stale
  saved session signed out.
- 2026-11-23, download check: three products downloaded (several files, a single file,
  and a cancelled then retried download), every file through its file link's
  redirect, with matching hashes. Instrumented runs recorded every final GET (14 per
  run for 11 files) answering 200 and passing `validate_download_response`.

## How the code reads answers

- Only the library can say the account is signed out (`AUTH_REQUIRED`). A content page
  that is not a success or not `UrlRedirects/DownloadPage` fails only that purchase
  (`PERMISSION_DENIED`).
- File link (`service.rs`): any redirect with a `Location` resolves the file; 401 is
  `DOWNLOAD_RESOLVER_FAILED`, 403 `SIGNED_LINK_EXPIRED`, 404 or 410
  `FILE_UNAVAILABLE`, anything else `DOWNLOAD_LINK_UNAVAILABLE`.
- File location (`validate_download_response`): 401 or 403 is `SIGNED_LINK_EXPIRED`,
  404 or 410 `FILE_UNAVAILABLE`, 429 `RATE_LIMITED`, any other failure
  `DOWNLOAD_RESOLVER_FAILED`.
