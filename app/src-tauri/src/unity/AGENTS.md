# Unity rules

These cover the App side here and the Bridge in
`unity/Packages/com.yank.nekonekoasset.bridge/`.

- Unity selects live, enabled, verified Editors and exact available selected files.
  The Bridge uses native review and import, durable same-ID receipts and partial
  results.
- Exactly one selected unitypackage with an ASCII file name opens in Unity's own
  Import Unity Package window of the picker's selected Editor instead (additive
  `nativeImport`, read in place, with no transfer, receipt or App job) while
  Features › Use native import for a single package is on (the default). Older
  Bridges, other names and the setting off use the Bridge.
- Failed-import cleanup is explicit and limited to unchanged new writes.
- No catalogue browser, offline project scan, Open App or avatar assembly.
- Preserve wire protocol v1; additions stay optional, as `nativeImport` is.
- Never reformat `unity/Packages`: the Bridge release attests its bytes. Keep `.meta`
  GUIDs stable.
