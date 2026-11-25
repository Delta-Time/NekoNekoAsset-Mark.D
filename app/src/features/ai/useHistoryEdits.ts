import { useEffect, useRef, useState } from 'react';
import type { DeleteState } from './chat-parts';

/** Renaming a chat in the history list, and deleting one with a second click that arms after a
 * short delay. */
export function useHistoryEdits() {
  const [renaming, setRenaming] = useState<string | null>(null);
  const [renameDraft, setRenameDraft] = useState('');
  const [deleting, setDeleting] = useState<DeleteState | null>(null);
  const [historyBusy, setHistoryBusy] = useState(false);
  const deleteTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const deleteButton = useRef<HTMLButtonElement>(null);
  useEffect(() => {
    if (deleting?.ready) deleteButton.current?.focus();
  }, [deleting?.id, deleting?.ready]);
  function resetConfirmation() {
    if (deleteTimer.current) clearTimeout(deleteTimer.current);
    deleteTimer.current = null;
    setDeleting(null);
    setRenaming(null);
  }
  return {
    renaming,
    setRenaming,
    renameDraft,
    setRenameDraft,
    deleting,
    setDeleting,
    historyBusy,
    setHistoryBusy,
    deleteTimer,
    deleteButton,
    resetConfirmation,
  };
}
export type HistoryEdits = ReturnType<typeof useHistoryEdits>;
