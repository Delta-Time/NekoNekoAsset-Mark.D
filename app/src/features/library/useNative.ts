import { useEffect, useRef } from 'react';
import type { Locale, SensitiveMode } from '../../model';
import { localeNames } from '../../i18n';
import {
  dialog as nativeDialog,
  emptyState,
  errorCode,
  invokeAction,
  libraryStatus,
  openLibrary,
  subscribeChanged,
} from '../../backend';
import type { NativeCall, Collection, NativeState, Preferences, Service } from '../../backend';
import { coalescedRefresh } from '../../refresh';
import { libraryAvailable } from './library-root';
import { restoreQuery } from '../filters/query';
import type { Saved } from '../filters/query';
import type { Library } from './useLibrary';
import type { Filters } from '../filters/useFilters';
import type { CollectionStore } from '../collections/useCollections';
import type { UiPreferences } from '../preferences/useUiPreferences';
import type { Navigation } from '../shell/useNavigation';
import type { Results } from '../results/useResults';
import type { Notices } from '../shell/useNotice';
import type { DialogKind, DialogOptions } from '../../bindings';

type LooseInvoke = (
  service: Service,
  action: string,
  args: Record<string, unknown>,
) => Promise<any>;
/** A library without stored preferences reads as having none. */
const withPreferences = <T extends { preferences: Preferences | null }>(state: T) => ({
  ...state,
  preferences: state.preferences ?? {},
});
/** Native calls for the open library: load and reload it, open another one, refresh its state
 * and avatar options, run actions, and follow `nna:changed` events and preference changes.
 * `reload`, `refreshState` and `refreshAvatarOptions` keep the closure of their first call, so
 * everything they use is a ref or a state setter. */
export function useNative({
  library,
  filters,
  collectionStore,
  uiPrefs,
  nav,
  results,
  notices,
  q,
}: {
  library: Library;
  filters: Filters;
  collectionStore: CollectionStore;
  uiPrefs: UiPreferences;
  nav: Navigation;
  results: Results;
  notices: Notices;
  q: (key: string) => string;
}) {
  const {
    licenseEnabled,
    reloadSequence,
    applyLibraryStatus,
    initialLoad,
    setLibraryReady,
    setLoaded,
    setNativeState,
    setLoadingError,
    setAccounts,
    preferencesRef,
    setSnapshotVersion,
    setSaved,
    setCustomCategories,
    libraryRefreshPending,
    setAutoAll,
    setSetupDone,
    credentialsPromptSeen,
    libraryStateRef,
    reloadTask,
    setLibraryOpening,
    blockedLibraryRequest,
    libraryEpoch,
    loaded,
    nativeState,
    libraryState,
    aiEnabled,
  } = library;
  const { query, setQuery } = filters;
  const {
    collectionEpoch,
    collectionPending,
    applyCollections,
    collectionRefreshNeeded,
    changeCollection,
    refreshCollections,
  } = collectionStore;
  const { setLocale, hydrate, locale } = uiPrefs;
  const { setFlow, flow, view, setView, settingsRoute, setSettingsRoute } = nav;
  const {
    setCatalogueTotal,
    setCatalogueCounts,
    setProducts,
    summaryCache,
    setQueryResult,
    setQueryBusy,
    setFailedQueryKey,
  } = results;
  const { notify } = notices;
  const licenseActive = useRef(false);
  const eventText = useRef(q);
  eventText.current = q;
  licenseActive.current = licenseEnabled && !!query.license;
  const reloadOnce = async () => {
    const sequence = ++reloadSequence.current,
      collectionsAtStart = collectionEpoch.current;
    try {
      const status = await libraryStatus();
      if (sequence !== reloadSequence.current) return;
      applyLibraryStatus(status);
      if (!initialLoad.current && Object.hasOwn(localeNames, String(status.locale)))
        setLocale(status.locale as Locale);
      if (status.state === 'blocked') {
        setLibraryReady(false);
        setLoaded(true);
        return;
      }
      const state: NativeState = withPreferences(
        await invokeAction('catalogue', 'bootstrap', { includeProducts: false }),
      );
      if (sequence !== reloadSequence.current) return;
      if (status.state === 'unconfigured') {
        setNativeState(state);
        setLibraryReady(false);
        setFlow('setup');
        setLoaded(true);
        setLoadingError('');
        return;
      }
      setNativeState(state);
      setCatalogueTotal(state.total ?? 0);
      setCatalogueCounts(state.counts ?? { all: 0, downloaded: 0, attention: 0 });
      setAccounts(state.accounts);
      preferencesRef.current = state.preferences ?? {};
      if (!collectionPending.current && collectionsAtStart === collectionEpoch.current)
        applyCollections((state.collections ?? []) as unknown as Collection[]);
      else collectionRefreshNeeded.current = true;
      setSnapshotVersion((version) => version + 1);
      setSaved(
        (state.savedSearches ?? []).map((search) => ({
          ...search,
          query: restoreQuery(search.query),
        })) as Saved[],
      );
      setCustomCategories(state.customCategories ?? []);
      libraryRefreshPending.current = false;
      setLibraryReady(libraryAvailable(state));
      if (status.state === 'setup') setFlow('setup');
      if (!initialLoad.current) {
        const p = state.preferences ?? {};
        hydrate(p);
        setQuery((old) => ({
          ...old,
          sensitive: ['blur', 'hide', 'show', 'only'].includes(String(p.sensitiveMode))
            ? (p.sensitiveMode as SensitiveMode)
            : 'blur',
        }));
        setAutoAll(typeof p.autoDownloadAll === 'boolean' ? p.autoDownloadAll : false);
        setSetupDone(p.setupComplete === true);
        if (!p.setupComplete) setFlow('setup');
        else if (!credentialsPromptSeen.current && status.missingCredentials?.length)
          setFlow('signInOnDevice');
        credentialsPromptSeen.current = true;
        initialLoad.current = true;
      }
      setLoaded(true);
      setLoadingError('');
    } catch (error) {
      const code = errorCode(error);
      setLoadingError(code);
      if (libraryRefreshPending.current) setLibraryReady(false);
      if (!initialLoad.current)
        applyLibraryStatus({
          state: 'blocked',
          path: libraryStateRef.current?.path,
          errorCode: code,
        });
      setLoaded(true);
    }
  };
  const reload = () => {
    if (!reloadTask.current) reloadTask.current = coalescedRefresh(reloadOnce);
    return reloadTask.current();
  };
  const openSelectedLibrary = async (path: string, newLibrary = false) => {
    const previous = libraryStateRef.current;
    let activated = false;
    setLibraryOpening(true);
    try {
      const result = await openLibrary(path, locale, newLibrary);
      blockedLibraryRequest.current = null;
      if (!result.restarting) {
        // Native selection has committed. Retain the setup draft and the new
        // candidate or library if its bootstrap fails, then retry that selection.
        activated = true;
        initialLoad.current = true;
        reloadSequence.current++;
        libraryEpoch.current++;
        libraryRefreshPending.current = true;
        setLibraryReady(false);
        setNativeState(emptyState);
        setAccounts([]);
        setProducts([]);
        summaryCache.current.clear();
        setQueryResult(null);
        setQueryBusy(false);
        setFailedQueryKey('');
        applyLibraryStatus({ state: 'unconfigured', path, defaultPath: path, locale });
        await reload();
        setLibraryOpening(false);
      }
    } catch (error) {
      if (!activated) {
        if (
          previous?.state === 'ready' ||
          previous?.state === 'setup' ||
          previous?.state === 'unconfigured'
        )
          applyLibraryStatus(previous);
        else {
          blockedLibraryRequest.current = { path, newLibrary };
          applyLibraryStatus({ state: 'blocked', path, errorCode: errorCode(error), locale });
        }
      }
      setLibraryOpening(false);
      throw error;
    }
  };
  const chooseLibrary = async () => {
    setLibraryOpening(true);
    try {
      const selected = await nativeDialog('folder', { defaultPath: libraryStateRef.current?.path });
      if (selected?.path) await openSelectedLibrary(selected.path, true);
      else setLibraryOpening(false);
    } catch (error) {
      setLibraryOpening(false);
      if (libraryStateRef.current?.state !== 'blocked')
        applyLibraryStatus({
          state: 'blocked',
          path: libraryStateRef.current?.path,
          errorCode: errorCode(error),
          locale,
        });
    }
  };
  const stateTask = useRef<(() => Promise<void>) | null>(null);
  const refreshState = () => {
    if (!stateTask.current)
      stateTask.current = coalescedRefresh(async () => {
        if (libraryRefreshPending.current) {
          await reload();
          return;
        }
        const sequence = reloadSequence.current,
          status = await libraryStatus();
        if (sequence !== reloadSequence.current) return;
        applyLibraryStatus(status);
        if (status.state !== 'ready' && status.state !== 'setup') return;
        const state = withPreferences(await invokeAction('catalogue', 'state'));
        if (sequence !== reloadSequence.current) return;
        setNativeState((old) => ({ ...old, ...state }));
        setAccounts(state.accounts);
        preferencesRef.current = state.preferences;
        setLibraryReady(libraryAvailable(state));
      });
    return stateTask.current();
  };
  const avatarOptionsTask = useRef<(() => Promise<void>) | null>(null);
  const refreshAvatarOptions = () => {
    if (!avatarOptionsTask.current)
      avatarOptionsTask.current = coalescedRefresh(async () => {
        if (libraryRefreshPending.current || libraryStateRef.current?.state !== 'ready') return;
        const epoch = libraryEpoch.current,
          sequence = reloadSequence.current;
        const state = await invokeAction('catalogue', 'avatar_options');
        if (
          epoch !== libraryEpoch.current ||
          sequence !== reloadSequence.current ||
          libraryRefreshPending.current
        )
          return;
        setNativeState((old) => ({ ...old, avatarOptions: state.avatarOptions ?? [] }));
      });
    return avatarOptionsTask.current();
  };
  /** Typed by `NativeCall`; the body serves every service and the native dialogs. */
  const onNative = (async (
    service: Service | 'dialog',
    action: string,
    args: Record<string, unknown> = {},
  ) => {
    try {
      const setupAction =
        libraryStateRef.current?.state === 'setup' &&
        ((service === 'booth' &&
          [
            'account_add',
            'account_login',
            'account_logout',
            'account_rename',
            'account_remove',
            'cancel_sync',
          ].includes(action)) ||
          (service === 'catalogue' && action === 'preferences'));
      if (
        service !== 'dialog' &&
        ((!setupAction && libraryStateRef.current?.state !== 'ready') ||
          libraryRefreshPending.current)
      )
        throw new Error('library_not_open');
      const result =
        service === 'catalogue' && action === 'collection'
          ? await changeCollection(args)
          : service === 'dialog'
            ? await nativeDialog(action as DialogKind, args as DialogOptions)
            : await (invokeAction as LooseInvoke)(service, action, args);
      if (result?.restartRequired === true) {
        setLibraryOpening(true);
        return result;
      }
      if (service === 'booth' && !action.startsWith('fetch_') && !action.startsWith('diagnose_'))
        await refreshState();
      return result;
    } catch (error) {
      notify(q(errorCode(error)));
      throw error;
    }
  }) as NativeCall;
  // A refresh refused while the update installer launches reruns on the next change; nothing failed.
  useEffect(() => {
    let dispose: (() => void) | undefined;
    let active = true;
    const report = (error: unknown) => {
      const code = errorCode(error);
      if (code !== 'updates_busy') notify(eventText.current(code));
    };
    let timer: ReturnType<typeof setTimeout> | undefined,
      registryTimer: ReturnType<typeof setTimeout> | undefined,
      productTimer: ReturnType<typeof setTimeout> | undefined,
      licenseTimer: ReturnType<typeof setTimeout> | undefined;
    void subscribeChanged(({ scope, productIds }) => {
      if (!active) return;
      if (scope === 'licenses') {
        if (licenseActive.current && !licenseTimer)
          licenseTimer = setTimeout(() => {
            licenseTimer = undefined;
            if (active) setSnapshotVersion((version) => version + 1);
          }, 1000);
        return;
      }
      if (scope === 'collections') {
        void refreshCollections().catch(report);
        return;
      }
      if (scope === 'jobs' || scope === 'state') {
        void refreshState().catch(report);
        return;
      }
      if (scope === 'products' || (scope === 'catalogue' && productIds?.length)) {
        if (!registryTimer)
          registryTimer = setTimeout(() => {
            registryTimer = undefined;
            if (active) void refreshAvatarOptions().catch(report);
          }, 500);
        if (!productTimer)
          productTimer = setTimeout(() => {
            productTimer = undefined;
            if (active) {
              setSnapshotVersion((version) => version + 1);
              void refreshState().catch(report);
            }
          }, 150);
        return;
      }
      if (registryTimer) {
        clearTimeout(registryTimer);
        registryTimer = undefined;
      }
      if (!timer)
        timer = setTimeout(() => {
          timer = undefined;
          if (active) void reload();
        }, 250);
    })
      .then((stop) => {
        if (active) dispose = stop;
        else stop();
      })
      .catch(report);
    void reload();
    return () => {
      active = false;
      dispose?.();
      if (timer) clearTimeout(timer);
      if (registryTimer) clearTimeout(registryTimer);
      if (productTimer) clearTimeout(productTimer);
      if (licenseTimer) clearTimeout(licenseTimer);
    };
  }, []);
  useEffect(() => {
    if (!loaded) return;
    const prefs = nativeState.preferences;
    setAutoAll(typeof prefs.autoDownloadAll === 'boolean' ? prefs.autoDownloadAll : false);
    const complete = prefs.setupComplete === true;
    setSetupDone(complete);
    if (complete) setFlow((current) => (current === 'setup' ? null : current));
  }, [loaded, nativeState.preferences]);
  useEffect(() => {
    if (flow === 'signInOnDevice' && !libraryState?.missingCredentials?.length) setFlow(null);
  }, [flow, libraryState?.missingCredentials]);
  useEffect(() => {
    const mode = nativeState.preferences.sensitiveMode;
    if (['blur', 'hide', 'show', 'only'].includes(String(mode)))
      setQuery((old) =>
        old.sensitive === mode ? old : { ...old, sensitive: mode as SensitiveMode },
      );
    if (!aiEnabled) {
      if (view === 'ai-suggestions') setView('library');
      if (settingsRoute === 'ai-settings') setSettingsRoute('features');
      if (flow === 'ai') setFlow(null);
    }
  }, [nativeState.preferences.sensitiveMode, aiEnabled]);
  return {
    reload,
    openSelectedLibrary,
    chooseLibrary,
    refreshState,
    refreshAvatarOptions,
    onNative,
  };
}
export type Native = ReturnType<typeof useNative>;
