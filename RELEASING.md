# Releasing NekoNekoAsset

This is the maintainer's process. Contributors need only
[CONTRIBUTING.md](CONTRIBUTING.md).

Each release has one numeric App version (`0.3.5`) and tag (`v0.3.5`). Keep
`app/package.json`, Cargo and Tauri config in agreement. The Bridge keeps the
version of the release that last changed it: a release that changes the Bridge
sets Bridge `package.json` to the same new version and attaches the Bridge, while
an App-only release attaches none. Release checks reject an unchanged Bridge
version whose sources differ from that version's tag. Use a patch increment for
routine releases. A minor or major increment requires the project owner's
explicit agreement before changing the version.

Starting with 0.2.1, every release needs an entry in the root `changelog.md` and
the same notes on its GitHub Release page. Keep release sections newest first,
with a heading such as `## 0.2.1 Beta - 2026-10-14`, then describe the actual
user-facing changes. The newest section must match the App version and have
nonempty notes; duplicate version sections are rejected. An App-only release
names the unchanged Bridge version and links the release that has its files.
Release tooling copies that complete heading and section into the draft and
published Release body.
The App update feed uses the published Release body, so release notes have one
source of truth. `release/config.json` contains only the Beta `prerelease` flag.

Windows releases contain the NSIS installer and its Tauri
signature; no portable App is distributed. Releases that change the Bridge also
include `.unitypackage`, UPM `.tgz`, ZIP and the verified inventory. VCC listing
is not yet configured.

Before tagging, the maintainer must perform bounded metadata-only real-session
checks in an empty isolated catalogue, with automatic downloads OFF: five pages
per available BOOTH Library/purchases/gifts/free-history, Gumroad buyer
library/bundle and Jinxxy inventory source, or a verified shorter natural end.
Drain details, compare observed facts with actual database/UI projections and
owned variants/files, and validate raw file lists/counts against the entitlement
union. Unexplained missing facts, duplicated aliases or count mismatches block a
release. Record aggregate publication timing, counts, requests and backoff; no
full-library scan or asset download is required or authorized by this check.
Also verify the current App Rust sender against an isolated actual Unity Editor:
received bytes, import result and durable same-ID receipt must pass. Unavailable
sessions/licensing block release instead of being counted as a pass.

Stage the final public build inputs, run `python scripts/release.py source-digest`,
then record the completed checks in the tracked `release/verification.json`:

```json
{
  "formatVersion": 1,
  "version": "0.2.1",
  "sourceSha256": "<source-digest output>",
  "verifiedAt": "<completed UTC timestamp, RFC 3339>",
  "checks": {
    "storeMetadata": {
      "passed": true,
      "pageLimit": 5,
      "autoDownload": false,
      "fullScan": false,
      "sources": ["booth", "gumroad", "jinxxy"],
      "failures": 0
    },
    "fileIdentity": {"passed": true, "failures": 0},
    "appUnityTransfer": {
      "passed": true,
      "receivedBytesMatch": true,
      "importSucceeded": true,
      "durableReceipt": true
    }
  }
}
```

This is a maintainer attestation after actual local checks; CI never receives
private sessions or local verification harnesses. Include only aggregate
evidence. Build-input changes invalidate the digest. Run
`python scripts/release.py check --tag v0.3.5 --require-verification`, commit the
source and marker, then push the matching tag. The Release workflow builds the
App, and the Bridge when the release changes it, uploads the complete checked set
to a draft, publishes it together, and updates GitHub Pages `/app/latest.json`
and `/bridge/latest.json` directly. The Bridge feed follows the newest release
that attaches the Bridge, so App-only releases do not offer a Bridge update.
Published assets are immutable. Rerunning a published tag refreshes Pages only;
fixes require a new version. Pages selects the highest published numeric version,
including Beta prereleases, so older-tag reruns cannot downgrade the update feed.

Configure GitHub Pages to use Actions and allow the `github-pages` environment
to deploy version tags. Store the Tauri updater private key and optional password
in repository secrets `TAURI_SIGNING_PRIVATE_KEY` and
`TAURI_SIGNING_PRIVATE_KEY_PASSWORD`; retain a secure backup outside Git. The
matching public key is in Tauri config. Release signatures bind their App version
in the signed trusted comment; manual signing must pass `--app-version`. Updater
signatures authenticate updates;
the first Beta does not include a Windows Authenticode publisher certificate.
