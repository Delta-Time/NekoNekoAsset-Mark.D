import { categoryKeys } from '../../model';
import { configuredLibraryRoot } from '../library/library-root';
import type { UiPreferences } from '../preferences/useUiPreferences';
import type { Results } from '../results/useResults';
import type { Library } from '../library/useLibrary';
import type { SelectedProducts } from '../selection/useSelectedProducts';
import type { Appearance } from '../appearance/useAppearance';
import type { Native } from '../library/useNative';
import type { Navigation } from '../shell/useNavigation';
import type { Notices } from '../shell/useNotice';
import type { LocalFiles } from '../local-files/useLocalFiles';
import type { Filters } from '../filters/useFilters';
import type { PreferenceSync } from '../preferences/usePreferenceSync';

/** What the workflow dialogs and settings pages receive: library and setup state, the theme and
 * locale, accounts, and the callbacks that open the library and navigate. */
export function useWorkflowProps({
  uiPrefs,
  results,
  library,
  selectedProducts,
  appearance,
  native,
  nav,
  notices,
  localFiles,
  filters,
  preferenceSync,
}: {
  uiPrefs: UiPreferences;
  results: Results;
  library: Library;
  selectedProducts: SelectedProducts;
  appearance: Appearance;
  native: Native;
  nav: Navigation;
  notices: Notices;
  localFiles: LocalFiles;
  filters: Filters;
  preferenceSync: PreferenceSync;
}) {
  const { locale, setLocale, theme, setTheme } = uiPrefs;
  const { products } = results;
  const {
    snapshotVersion,
    libraryReady,
    libraryState,
    libraryRefreshPending,
    loadingError,
    libraryStateRef,
    nativeState,
    setAutoAll,
    setSetupDone,
    autoAll,
    accounts,
  } = library;
  const { chosen } = selectedProducts;
  const { themeOptions } = appearance;
  const { openSelectedLibrary, onNative } = native;
  const { unitySeed, setFlow, setView, flow, importSeed } = nav;
  const { notify } = notices;
  const { latestLocalImport } = localFiles;
  const { clearFilters, updateQuery } = filters;
  const { onPreferences } = preferenceSync;
  return {
    locale,
    products,
    catalogueVersion: snapshotVersion,
    selected: chosen,
    onLocaleChange: setLocale,
    themeOptions,
    themeId: theme,
    onThemeChange: setTheme,
    libraryReady,
    setupReady: libraryState?.state === 'setup' && !libraryRefreshPending.current,
    defaultLibraryPath:
      (libraryRefreshPending.current || libraryState?.state === 'setup'
        ? libraryState?.path
        : libraryState?.defaultPath) ?? undefined,
    defaultLibraryError: libraryRefreshPending.current
      ? loadingError
      : ['setup', 'unconfigured'].includes(libraryState?.state ?? '')
        ? (libraryState?.errorCode ?? undefined)
        : undefined,
    missingCredentials: libraryState?.missingCredentials,
    onLibraryReady: async (path: string, _rootId?: string, newLibrary = false) => {
      if (
        libraryStateRef.current?.state === 'ready' &&
        configuredLibraryRoot(nativeState)?.path === path
      )
        return;
      await openSelectedLibrary(path, newLibrary);
    },
    unitySeed,
    onUnitySubmitted: () => {
      setFlow(null);
      setView('activity');
    },
    onClose: () => setFlow(null),
    onNotice: notify,
    onNewAssets: (ids: string[], category?: string, jobId?: string) => {
      latestLocalImport.current = jobId ? { jobId, plannedIds: ids, category } : null;
      clearFilters();
      if (category && categoryKeys.some((key) => key === category)) updateQuery({ category });
      setView('library');
    },
    onPolicyChange: (all: boolean) => {
      setAutoAll(all);
      if (flow === 'setup') setSetupDone(true);
    },
    autoDownloadAll: autoAll,
    accounts,
    importSeed,
    nativeState,
    onNative,
    onPreferences,
  };
}
export type WorkflowProps = ReturnType<typeof useWorkflowProps>;
