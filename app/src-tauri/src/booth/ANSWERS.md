# What BOOTH answers

Only answers someone has seen are listed. "Unknown" means nobody has observed it, so
the code's handling there is a guess (last section). When you see a new answer, add it
with the date and how you saw it.

| Request | Success | Refused | Removed | Signed out |
| --- | --- | --- | --- | --- |
| Account list: Library, gifts, free history, orders (`accounts.booth.pm`) | 200 HTML. An empty list is one lone text panel; a one-page list has an empty `.pager` | Unknown | A delisted Library row has a faded thumbnail and no item link | Ends on BOOTH's sign-in page |
| Public item page or JSON (`booth.pm/ja/items/<id>`, `.json`) | 200. Signed in, a gift's `.json` holds `gift: {accepted_at, url}` | Unknown | 404 | 200, with `gift: null` in the `.json` |
| File (`/downloadables/<id>` route) | Redirects to a file host, then 200 with the bytes | Unknown | Refused like a signed-out request, not 404 | Unknown |

The removed-file answer is one of the three the code reads as a refusal (401, BOOTH's
sign-in page, or 403 from a BOOTH host); which one was not recorded.

## Evidence

- 2026-10-30 (`95e2fb7`): requests without the session cookie ended on the sign-in page.
- 2026-11-03, gift dates: a signed-in `items/<id>.json` has `gift: {accepted_at, url}`
  and a signed-out one `gift: null`. `/ja/` writes the date as `YYYY年M月D日`; the other
  locales write `YYYY-MM-DD`. The three removed listings answered 404.
- 2026-11-05, real account pages in all five languages: account pages follow the
  account's language, public pages the URL's locale; the empty-list and one-page shapes
  above; `?page=9999` shows the past-the-end shape; `booth.pm/items/<id>` without a
  locale is 404 for a client without BOOTH's cookies.
- 2026-11-22 (`db730d7`), from a user's report: two old files of paid items, replaced
  by newer versions, were refused the way a signed-out request is, instead of 404.
- 2026-11-23, download check: a zip downloaded through the ticket and matched its
  hash. BOOTH lists no file sizes.

## How the code reads answers

- Only an account list can say the account is signed out (`AUTH_REQUIRED`). A refused
  item, receipt or file fails only that asset.
- `validate_download_response` in `mod.rs`: 401, BOOTH's sign-in page or a 403 from a
  BOOTH host is `PERMISSION_DENIED`; a 403 from another host is `SIGNED_LINK_EXPIRED`;
  404 or 410 is `FILE_UNAVAILABLE`; 429 is `RATE_LIMITED`; 5xx is
  `DOWNLOAD_RESOLVER_FAILED`; HTML that is not an attachment is
  `DOWNLOAD_LINK_UNAVAILABLE`.
- Because a removed file is refused rather than 404, a refusal makes the download
  reread the page that last listed the file, then at most three receipts (or the free
  item's page). Only a complete listing of the same product and variation without the
  file makes it `FILE_UNAVAILABLE`, which counts the file as dropped.
