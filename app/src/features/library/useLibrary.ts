import { useRef, useState } from 'react';
import { emptyState } from '../../backend';
import type { LibraryStatus, NativeState } from '../../backend';
import type { Account, CustomCategory } from '../../model';
import { batchActionPreferences } from '../../FeaturesSettings';
import { isStoreSyncJob } from '../../Workflows';
import type { Saved } from '../filters/query';

/** The open library as the UI holds it: native state, load status, accounts, saved searches and
 * custom categories, and the refs async work checks so a reply for an older library or reload is
 * dropped. */
export function useLibrary() {
  const [nativeState, setNativeState] = useState<NativeState>(emptyState),
    [loaded, setLoaded] = useState(false),
    [loadingError, setLoadingError] = useState('');
  const [libraryState, setLibraryState] = useState<LibraryStatus | null>(null),
    [libraryOpening, setLibraryOpening] = useState(false),
    libraryStateRef = useRef<LibraryStatus | null>(null),
    credentialsPromptSeen = useRef(false),
    libraryRefreshPending = useRef(false),
    libraryEpoch = useRef(0);
  const blockedLibraryRequest = useRef<{ path: string; newLibrary: boolean } | null>(null);
  const applyLibraryStatus = (status: LibraryStatus) => {
    libraryStateRef.current = status;
    setLibraryState(status);
  };
  const preferencesRef = useRef<Record<string, unknown>>({}),
    initialLoad = useRef(false),
    reloadSequence = useRef(0),
    reloadTask = useRef<(() => Promise<void>) | null>(null);
  const [saved, setSaved] = useState<Saved[]>([]),
    [customCategories, setCustomCategories] = useState<CustomCategory[]>([]);
  const [snapshotVersion, setSnapshotVersion] = useState(0);
  const [libraryReady, setLibraryReady] = useState(false);
  const [autoAll, setAutoAll] = useState(false),
    [setupDone, setSetupDone] = useState(false);
  const [accounts, setAccounts] = useState<Account[]>([]);
  const syncing = nativeState.jobs.some(
    (job) => isStoreSyncJob(job.type) && ['queued', 'running'].includes(job.state),
  );
  const aiEnabled = nativeState.preferences.aiFeaturesEnabled === true,
    licenseEnabled = nativeState.preferences.licenseFilter === true,
    batchEnabled = batchActionPreferences(nativeState.preferences.batchActions);
  return {
    nativeState,
    setNativeState,
    loaded,
    setLoaded,
    loadingError,
    setLoadingError,
    libraryState,
    libraryOpening,
    setLibraryOpening,
    libraryStateRef,
    credentialsPromptSeen,
    libraryRefreshPending,
    libraryEpoch,
    blockedLibraryRequest,
    applyLibraryStatus,
    preferencesRef,
    initialLoad,
    reloadSequence,
    reloadTask,
    saved,
    setSaved,
    customCategories,
    setCustomCategories,
    snapshotVersion,
    setSnapshotVersion,
    libraryReady,
    setLibraryReady,
    autoAll,
    setAutoAll,
    setupDone,
    setSetupDone,
    accounts,
    setAccounts,
    syncing,
    aiEnabled,
    licenseEnabled,
    batchEnabled,
  };
}
export type Library = ReturnType<typeof useLibrary>;
