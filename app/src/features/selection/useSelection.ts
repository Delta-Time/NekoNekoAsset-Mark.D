import { useRef, useState } from 'react';

/** Selected and focused products, the shift-click anchor, revealed R18 thumbnails and a suggested
 * product that is selected once its results arrive. */
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
  };
}
export type Selection = ReturnType<typeof useSelection>;
