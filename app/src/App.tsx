import type { CSSProperties } from 'react';
import './settings-reset.css';
import { LibraryAccess } from './Workflows';
import { appText } from './shared/app-text';
import { useUiPreferences } from './features/preferences/useUiPreferences';
import { useLayout } from './features/shell/useLayout';
import { useNavigation } from './features/shell/useNavigation';
import { useNotice } from './features/shell/useNotice';
import { useLibrary } from './features/library/useLibrary';
import { useSelection } from './features/selection/useSelection';
import { useFilters } from './features/filters/useFilters';
import { useCollections } from './features/collections/useCollections';
import { useResults } from './features/results/useResults';
import { useSelectedProducts } from './features/selection/useSelectedProducts';
import { useNative } from './features/library/useNative';
import { usePreferenceSync } from './features/preferences/usePreferenceSync';
import { useCategories } from './features/categories/useCategories';
import { useAvatars } from './features/avatars/useAvatars';
import { useLocalFiles } from './features/local-files/useLocalFiles';
import { useProductEdits } from './features/products/useProductEdits';
import { useProductActions } from './features/products/useProductActions';
import { useAppearance } from './features/appearance/useAppearance';
import { useSettingsPane } from './features/settings/useSettingsPane';
import { useUpdates } from './features/updates/useUpdates';
import { useShortcuts } from './features/shell/useShortcuts';
import Sidebar from './features/sidebar/Sidebar';
import ActivityView from './features/activity/ActivityView';
import { useWorkflowProps } from './features/workflows/useWorkflowProps';
import Topbar from './features/shell/Topbar';
import NavDivider from './features/sidebar/NavDivider';
import AiPane from './features/ai/AiPane';
import LibraryView from './features/results/LibraryView';
import Statusbar from './features/shell/Statusbar';
import AppDialogs from './features/shell/AppDialogs';
export default function App() {
  const uiPrefs = useUiPreferences();
  const { locale, navWidth, setNavWidth, inspectorWidth, setInspectorWidth } = uiPrefs;
  const layout = useLayout({
    navWidth,
    setNavWidth,
    inspectorWidth,
    setInspectorWidth,
  });
  const nav = useNavigation();
  const { view, flow, dialog } = nav;
  const q = (key: string) => appText(locale, key);
  const notices = useNotice({ q, flow, dialog });
  const library = useLibrary();
  const { loaded, libraryState, libraryOpening, blockedLibraryRequest } = library;
  const selection = useSelection();
  const filters = useFilters({ selection, notices, nav, library });
  const collectionStore = useCollections({ notices, filters, q });
  const results = useResults({ library, filters, collectionStore, notices, q });
  const selectedProducts = useSelectedProducts({ results, selection, library, nav, notices, q });
  const native = useNative({
    library,
    filters,
    collectionStore,
    uiPrefs,
    nav,
    results,
    notices,
    q,
  });
  const { openSelectedLibrary, chooseLibrary } = native;
  const preferenceSync = usePreferenceSync({ library, notices, uiPrefs, q });
  const categories = useCategories({ library, uiPrefs, native, filters, q });
  const avatars = useAvatars({ uiPrefs, library, preferenceSync, native, notices, filters, q });
  const localFiles = useLocalFiles({ filters, nav, library, native, notices, uiPrefs, q });
  const edits = useProductEdits({
    native,
    notices,
    selectedProducts,
    filters,
    results,
    selection,
    nav,
    q,
  });
  const actions = useProductActions({
    library,
    notices,
    native,
    selectedProducts,
    edits,
    uiPrefs,
    q,
  });
  const appearance = useAppearance({ uiPrefs, preferenceSync, notices, library, q });
  const { themeTokens, themeDeleteTarget, appTheme, appClass } = appearance;
  const settingsPane = useSettingsPane({ nav, library });
  const updates = useUpdates({
    nav,
    edits,
    appearance,
    library,
    collectionStore,
    localFiles,
    preferenceSync,
  });
  const { updateOpen } = updates;
  const shortcuts = useShortcuts({
    updates,
    appearance,
    library,
    uiPrefs,
    edits,
    nav,
    selection,
    results,
    notices,
  });
  const workflowProps = useWorkflowProps({
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
    edits,
    settingsPane,
    preferenceSync,
  });
  const sidebar = (
    <Sidebar
      uiPrefs={uiPrefs}
      filters={filters}
      results={results}
      categories={categories}
      nav={nav}
      avatars={avatars}
      notices={notices}
      native={native}
      collectionStore={collectionStore}
      library={library}
      selection={selection}
      q={q}
    />
  );
  const activity = (
    <ActivityView
      uiPrefs={uiPrefs}
      library={library}
      results={results}
      actions={actions}
      native={native}
      nav={nav}
      notices={notices}
      settingsPane={settingsPane}
      q={q}
    />
  );
  if (
    !libraryState ||
    libraryState.state === 'blocked' ||
    !loaded ||
    (libraryOpening && !['ready', 'setup', 'unconfigured'].includes(libraryState.state))
  )
    return (
      <div className={appClass} data-theme={appTheme} style={themeTokens as CSSProperties}>
        <LibraryAccess
          locale={locale}
          status={libraryState}
          busy={libraryOpening || !loaded}
          onChoose={() => void chooseLibrary()}
          onRetry={() => {
            if (libraryState?.path) {
              const request = blockedLibraryRequest.current;
              void openSelectedLibrary(
                libraryState.path,
                request?.path === libraryState.path && request.newLibrary,
              ).catch(() => {});
            }
          }}
        />
      </div>
    );
  const topbar = (
    <Topbar
      updates={updates}
      appearance={appearance}
      library={library}
      nav={nav}
      uiPrefs={uiPrefs}
      shortcuts={shortcuts}
      filters={filters}
      settingsPane={settingsPane}
      q={q}
    />
  );
  const navDivider = <NavDivider layout={layout} uiPrefs={uiPrefs} q={q} />;
  const aiPane = (
    <AiPane
      library={library}
      nav={nav}
      updates={updates}
      appearance={appearance}
      edits={edits}
      uiPrefs={uiPrefs}
      avatars={avatars}
      filters={filters}
    />
  );
  const libraryView = (
    <LibraryView
      filters={filters}
      uiPrefs={uiPrefs}
      library={library}
      nav={nav}
      selection={selection}
      results={results}
      categories={categories}
      avatars={avatars}
      selectedProducts={selectedProducts}
      edits={edits}
      actions={actions}
      notices={notices}
      appearance={appearance}
      layout={layout}
      preferenceSync={preferenceSync}
      native={native}
      localFiles={localFiles}
      q={q}
      workflowProps={workflowProps}
    />
  );
  const statusbar = (
    <Statusbar
      updates={updates}
      appearance={appearance}
      uiPrefs={uiPrefs}
      results={results}
      collectionStore={collectionStore}
      notices={notices}
      library={library}
      q={q}
    />
  );
  const dialogs = (
    <AppDialogs
      nav={nav}
      actions={actions}
      library={library}
      selection={selection}
      results={results}
      uiPrefs={uiPrefs}
      edits={edits}
      notices={notices}
      updates={updates}
      selectedProducts={selectedProducts}
      appearance={appearance}
      collectionStore={collectionStore}
      avatars={avatars}
      native={native}
      filters={filters}
      categories={categories}
      localFiles={localFiles}
      q={q}
      workflowProps={workflowProps}
    />
  );
  return (
    <div
      className={appClass}
      data-view={view}
      data-theme={appTheme}
      style={{ ...themeTokens, '--nav-width': navWidth + 'px' } as CSSProperties}
    >
      {topbar}
      <div className="shell" inert={updateOpen || !!themeDeleteTarget}>
        {view !== 'activity' && view !== 'ai-suggestions' && (
          <>
            {sidebar}
            {navDivider}
          </>
        )}
        <main className="workspace">
          {aiPane}
          {view === 'activity' ? activity : view === 'ai-suggestions' ? null : libraryView}
        </main>
      </div>
      {statusbar}
      {dialogs}
    </div>
  );
}
