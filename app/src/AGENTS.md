# UI rules

These apply to every UI change, here and in the Unity Bridge's windows.

- Avoid unnecessary secondary labels and descriptions. Let the layout do the
  explaining.
- Preserve the accepted workflow, compact controls and visual language unless a
  redesign is requested. The rendered App is the source of truth.
- Inspect the repeated journey and intermediate states before handing work back.
  Compilation and automated tests do not establish visual quality or usability.
- Check relevant widths, scales, languages, themes, keyboard/focus and error/loading
  states. Fix clipped text, overflow, overlap, broken focus and unusable actions.
  When rendering or native interaction cannot be inspected, state that limitation.
- Do not add tutorial paragraphs or cards, redundant labels, decorative dashboards
  or speculative settings.
- Light and Dark backgrounds and borders stay neutral grayscale; accents belong to
  controls, highlights and statuses.
- UI text goes into every `locales/*.json`.

## Where things are

- `App.tsx` composes one hook per area from `features/<area>/use*.ts`; hooks take
  each other's result objects as parameters. There are no reducers, contexts or
  global store.
- Talk to Rust only through `backend.ts` and `ai-backend.ts`. Their types come from
  `bindings/`, which `pnpm --dir app bindings` generates; never edit it by hand.
- Filtering, search, sorting and category rules live in Rust (`catalogue/query.rs`,
  `taxonomy/`), not in the UI.
- Setup and library rules are in `../src-tauri/src/AGENTS.md`.
