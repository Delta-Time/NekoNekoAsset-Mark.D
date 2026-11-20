import { useCallback, useRef, useState } from 'react';
import { updateJobBlocker } from '../../update-i18n';
import type { Navigation } from '../shell/useNavigation';
import type { ProductEdits } from '../products/useProductEdits';
import type { Appearance } from '../appearance/useAppearance';
import type { Library } from '../library/useLibrary';
import type { CollectionStore } from '../collections/useCollections';
import type { LocalFiles } from '../local-files/useLocalFiles';
import type { PreferenceSync } from '../preferences/usePreferenceSync';

/** The update dialog and what blocks installing an update: the library not ready, a running job,
 * an open window, an unsent AI chat or a save in progress. */
export function useUpdates({
  nav,
  edits,
  appearance,
  library,
  collectionStore,
  localFiles,
  preferenceSync,
}: {
  nav: Navigation;
  edits: ProductEdits;
  appearance: Appearance;
  library: Library;
  collectionStore: CollectionStore;
  localFiles: LocalFiles;
  preferenceSync: PreferenceSync;
}) {
  const { flow, dialog, context } = nav;
  const { deleteLocalSelection } = edits;
  const { themeDeleteTarget, themeDeleteBusy, zoomQueue } = appearance;
  const { libraryOpening, setupDone, libraryState, nativeState } = library;
  const { collectionBusy } = collectionStore;
  const { localFilePending } = localFiles;
  const { drainPreferenceWrites } = preferenceSync;
  const [updateOpen, setUpdateOpen] = useState(false),
    [updateRequest, setUpdateRequest] = useState(0),
    [updateAvailable, setUpdateAvailable] = useState(false),
    [aiChatUpdateBlocked, setAiChatUpdateBlocked] = useState(false);
  const aiChatUpdateBlockedRef = useRef(false),
    updateBlockerRef = useRef('');
  const reportAiChatUpdateBlocked = useCallback((blocked: boolean) => {
    aiChatUpdateBlockedRef.current = blocked;
    setAiChatUpdateBlocked(blocked);
  }, []);
  const updateCanOpen =
    !flow && !dialog && !deleteLocalSelection && !themeDeleteTarget && !libraryOpening && !context;
  const updateBlocker =
    !setupDone || libraryState?.state !== 'ready' || libraryOpening
      ? 'updates_busy_library'
      : updateJobBlocker(nativeState.jobs) ||
        (!updateCanOpen
          ? 'updates_busy_window'
          : aiChatUpdateBlocked
            ? 'updates_busy_ai_chat'
            : collectionBusy || localFilePending.size > 0 || themeDeleteBusy
              ? 'updates_busy_saving'
              : '');
  updateBlockerRef.current = updateBlocker;
  const currentUpdateBlocker = () =>
    updateBlockerRef.current || (aiChatUpdateBlockedRef.current ? 'updates_busy_ai_chat' : '');
  const prepareUpdate = async () => {
    let blocker = currentUpdateBlocker();
    if (blocker) throw new Error(blocker);
    await zoomQueue.current;
    await drainPreferenceWrites();
    blocker = currentUpdateBlocker();
    if (blocker) throw new Error(blocker);
  };
  return {
    updateOpen,
    setUpdateOpen,
    updateRequest,
    setUpdateRequest,
    updateAvailable,
    setUpdateAvailable,
    reportAiChatUpdateBlocked,
    updateCanOpen,
    updateBlocker,
    currentUpdateBlocker,
    prepareUpdate,
  };
}
export type Updates = ReturnType<typeof useUpdates>;
