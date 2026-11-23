import { useEffect, useRef } from 'react';
import type { Updates } from '../updates/useUpdates';
import type { Appearance } from '../appearance/useAppearance';
import type { Library } from '../library/useLibrary';
import type { UiPreferences } from '../preferences/useUiPreferences';
import type { ProductEdits } from '../products/useProductEdits';
import type { Navigation } from './useNavigation';
import type { Selection } from '../selection/useSelection';
import type { Results } from '../results/useResults';
import type { Notices } from './useNotice';

/** App-wide keys: zoom, select all, find, undo and Escape. */
export function useShortcuts({
  updates,
  appearance,
  library,
  uiPrefs,
  edits,
  nav,
  selection,
  results,
  notices,
}: {
  updates: Updates;
  appearance: Appearance;
  library: Library;
  uiPrefs: UiPreferences;
  edits: ProductEdits;
  nav: Navigation;
  selection: Selection;
  results: Results;
  notices: Notices;
}) {
  const { updateOpen } = updates;
  const { themeDeleteTarget, changeScale } = appearance;
  const { libraryStateRef, libraryOpening } = library;
  const { zoomTarget, uiScale } = uiPrefs;
  const { deleteLocalSelection, undoAction, undo } = edits;
  const { flow, dialog, context, view, setContext } = nav;
  const { addToSelection, clearSelection } = selection;
  const { matchedIds } = results;
  const { run } = notices;
  const searchRef = useRef<HTMLInputElement>(null);
  useEffect(() => {
    const handler = (e: globalThis.KeyboardEvent) => {
      if (updateOpen || themeDeleteTarget) return;
      if (!['ready', 'setup'].includes(libraryStateRef.current?.state ?? '') || libraryOpening)
        return;
      if (e.isComposing || e.keyCode === 229) return;
      const editing =
        e.target instanceof Element &&
        !!e.target.closest('input,textarea,select,[contenteditable="true"]');
      if ((e.ctrlKey || e.metaKey) && ['+', '=', '-', 'Add', 'Subtract'].includes(e.key)) {
        e.preventDefault();
        changeScale(zoomTarget.current + (['-', 'Subtract'].includes(e.key) ? -10 : 10));
        return;
      }
      if (libraryStateRef.current?.state !== 'ready') return;
      if (deleteLocalSelection) return;
      if (
        (e.ctrlKey || e.metaKey) &&
        e.key.toLowerCase() === 'a' &&
        !editing &&
        !flow &&
        !dialog &&
        !context &&
        ['library', 'settings'].includes(view)
      ) {
        e.preventDefault();
        addToSelection(matchedIds);
        return;
      }
      if ((e.ctrlKey || e.metaKey) && e.key === 'f' && !flow && !dialog) {
        e.preventDefault();
        searchRef.current?.focus();
      }
      if ((e.ctrlKey || e.metaKey) && e.key === 'z' && !flow && !dialog && !editing) {
        e.preventDefault();
        run(undoAction());
      }
      if (e.key === 'Escape' && !flow && !dialog) {
        // Escape in a field, such as the search beside a selection, only leaves the field.
        if (context) setContext(null);
        else if (editing) (document.activeElement as HTMLElement | null)?.blur();
        else clearSelection();
      }
    };
    window.addEventListener('keydown', handler);
    return () => window.removeEventListener('keydown', handler);
  }, [
    flow,
    dialog,
    deleteLocalSelection,
    undo,
    uiScale,
    context,
    matchedIds,
    view,
    libraryOpening,
    updateOpen,
    themeDeleteTarget,
  ]);
  return { searchRef };
}
export type Shortcuts = ReturnType<typeof useShortcuts>;
