import { useRef, useState } from 'react';

/** Selected and focused products, the shift-click anchor, revealed R18 thumbnails and a suggested
 * product that is selected once its results arrive. The selection outlives the view: browsing to
 * another category, collection, search or sort keeps it, so one action can cover products from
 * several views. */
export function useSelection() {
  const [selected, setSelected] = useState<string[]>([]),
    [focusId, setFocusId] = useState('');
  const [revealed, setRevealed] = useState<Set<string>>(new Set());
  const anchor = useRef('');
  const pendingSuggestionProduct = useRef('');
  const clearSelection = () => {
    setSelected([]);
    setFocusId('');
    setRevealed(new Set());
  };
  /** Browsing elsewhere keeps the selection; the focus and revealed images stay with the view. */
  const leaveView = () => {
    setFocusId('');
    setRevealed(new Set());
  };
  /** Adds products, such as everything in view, keeping what is already selected. */
  const addToSelection = (ids: readonly string[]) =>
    setSelected((old) => [...new Set([...old, ...ids])]);
  return {
    selected,
    setSelected,
    focusId,
    setFocusId,
    revealed,
    setRevealed,
    anchor,
    pendingSuggestionProduct,
    clearSelection,
    leaveView,
    addToSelection,
  };
}
export type Selection = ReturnType<typeof useSelection>;
