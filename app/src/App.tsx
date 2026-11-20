import type { CSSProperties } from 'react';
import {
  Search,
  RefreshCw,
  Download,
  Folder,
  Plus,
  X,
  FileArchive,
  FileBox,
  CheckCircle2,
  Bookmark,
  Package,
  SlidersHorizontal,
  Tags,
  Settings,
  EyeOff,
  Trash2,
  LoaderCircle,
  BellOff,
  Scale,
} from 'lucide-react';
import { categoryKeys } from './model';
import type { Product } from './model';
import { version as appVersion } from '../package.json';
import AvatarManager from './AvatarManager';
import AssetInspector from './AssetInspector';
import DeleteLocalProducts from './DeleteLocalProducts';
import ConfirmProducts from './ConfirmProducts';
import ConfirmDelete from './ConfirmDelete';
import UpdatePrompt from './UpdatePrompt';
import { resumesAfterRestart, updateText } from './update-i18n';
import './settings-reset.css';
import AiSuggestions from './AiSuggestions';
import FeaturesSettings from './FeaturesSettings';
import SpendingDialog from './SpendingDialog';
import { LibraryContextMenu } from './LibraryMenus';
import type { ContextAction } from './LibraryMenus';
import { aiSuggestionsText } from './ai-suggestions-i18n';
import { defaultThemeOptions } from './ThemePicker';
import Workflows, { WorkflowContent, LibraryAccess } from './Workflows';
import EditDialogs from './EditDialogs';
import { LicenseFilterDialog } from './Licenses';
import { licenseText } from './license-i18n';
import { invokeAction, openExternal, fetchProductMetadata } from './backend';
import Button from './shared/Button';
import SmallDialog from './shared/SmallDialog';
import { appText } from './shared/app-text';
import Results from './features/results/Results';
import { configuredLibraryRoot } from './features/library/library-root';
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
import ProductItem from './features/results/ProductItem';
import SingleActions from './features/products/SingleActions';
import ResultsFooter from './features/results/ResultsFooter';
import LibraryToolbar from './features/filters/LibraryToolbar';
import Sidebar from './features/sidebar/Sidebar';
import BatchSummary from './features/inspector/BatchSummary';
import AppearanceSettings from './features/appearance/AppearanceSettings';
import ActivityView from './features/activity/ActivityView';
export default function App() {
  const uiPrefs = useUiPreferences();
  const {
    locale,
    setLocale,
    theme,
    setTheme,
    presets,
    thumb,
    density,
    navWidth,
    setNavWidth,
    inspectorWidth,
    setInspectorWidth,
    cardColumns,
    oneHandedDrag,
  } = uiPrefs;
  const layout = useLayout({
    navWidth,
    setNavWidth,
    inspectorWidth,
    setInspectorWidth,
  });
  const { paneWidth, wide, resize } = layout;
  const nav = useNavigation();
  const {
    view,
    setView,
    settingsRoute,
    setSettingsRoute,
    unitySeed,
    openUnity,
    flow,
    setFlow,
    dialog,
    setDialog,
    tab,
    setTab,
    inspector,
    context,
    setContext,
    licenseOpen,
    setLicenseOpen,
    licenseButton,
    importSeed,
    setImportSeed,
  } = nav;
  const q = (key: string) => appText(locale, key);
  const notices = useNotice({ q, flow, dialog });
  const { notice, setNotice, notify, run } = notices;
  const library = useLibrary();
  const {
    nativeState,
    loaded,
    loadingError,
    setLoadingError,
    libraryState,
    libraryOpening,
    libraryStateRef,
    libraryRefreshPending,
    blockedLibraryRequest,
    saved,
    snapshotVersion,
    libraryReady,
    autoAll,
    setAutoAll,
    setupDone,
    setSetupDone,
    accounts,
    syncing,
    aiEnabled,
    licenseEnabled,
  } = library;
  const selection = useSelection();
  const { setSelected, focusId, revealed, setRevealed, anchor } = selection;
  const filters = useFilters({ selection, notices, nav, library });
  const {
    query,
    sort,
    direction,
    collection,
    setCollection,
    savedScope,
    setSavedScope,
    updateQuery,
    openSuggestedProduct,
    clearFilters,
  } = filters;
  const collectionStore = useCollections({ notices, filters, q });
  const { collections, collectionRows, collectionBusy } = collectionStore;
  const results = useResults({ library, filters, collectionStore, notices, q });
  const { products, catalogueTotal, scrollState, filtered, matchedIds, resultsPending } = results;
  const selectedProducts = useSelectedProducts({ results, selection, library, nav, notices, q });
  const {
    visibleSelected,
    selectedSet,
    active,
    chosen,
    availability,
    availabilityKey,
    resultKeys,
  } = selectedProducts;
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
  const { openSelectedLibrary, chooseLibrary, refreshAvatarOptions, onNative } = native;
  const preferenceSync = usePreferenceSync({ library, notices, uiPrefs, q });
  const { onPreferences } = preferenceSync;
  const categories = useCategories({ library, uiPrefs, native, filters, q });
  const { categoryLabel, categoryList, changeCategories } = categories;
  const avatars = useAvatars({ uiPrefs, library, preferenceSync, native, notices, filters, q });
  const {
    changeAvatarDetails,
    avatarOptions,
    avatarLabel,
    avatarEnabled,
    changeAvatars,
    avatarChoices,
  } = avatars;
  const localFiles = useLocalFiles({ filters, nav, library, native, notices, uiPrefs, q });
  const { localFilePending, latestLocalImport, jobs, changeLocalFiles, addLocalFiles } = localFiles;
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
  const {
    deleteLocalSelection,
    setDeleteLocalSelection,
    batchPatch,
    userPatch,
    localSelection,
    requestLocalDelete,
    onlineSelection,
    hiddenSelection,
    hideProducts,
    deleteLocalProducts,
    edit,
  } = edits;
  const actions = useProductActions({
    library,
    notices,
    native,
    selectedProducts,
    edits,
    uiPrefs,
    q,
  });
  const {
    productConfirm,
    setProductConfirm,
    deletingDownloads,
    activeDownloads,
    cancelDownloads,
    allUnavailable,
    hasDownloadedFiles,
    requestDeleteDownloads,
    deleteDownloads,
    watchDownloads,
    download,
    downloadFile,
    update,
    syncLicenses,
    needsAttention,
    dismissAttention,
    refusedFiles,
    exportZip,
  } = actions;
  const appearance = useAppearance({ uiPrefs, preferenceSync, notices, library, q });
  const {
    setThemePreview,
    selectedThemeTokens,
    selectedThemeName,
    themeTokens,
    choosePreset,
    themeDeleteTarget,
    setThemeDeleteTarget,
    deleteTheme,
    appTheme,
    appClass,
  } = appearance;
  const settingsPane = useSettingsPane({ nav, library });
  const { openSettings, toggleSettings } = settingsPane;
  const updates = useUpdates({
    nav,
    edits,
    appearance,
    library,
    collectionStore,
    localFiles,
    preferenceSync,
  });
  const {
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
  } = updates;
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
  const { searchRef } = shortcuts;
  const singleActions = (p: Product) => (
    <SingleActions
      p={p}
      actions={actions}
      nav={nav}
      edits={edits}
      library={library}
      selectedProducts={selectedProducts}
      notices={notices}
      q={q}
    />
  );
  const batchSummary = (
    <BatchSummary
      selectedProducts={selectedProducts}
      library={library}
      edits={edits}
      nav={nav}
      actions={actions}
      notices={notices}
      uiPrefs={uiPrefs}
      results={results}
      filters={filters}
      selection={selection}
      q={q}
    />
  );
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
  const toolbar = (
    <LibraryToolbar
      filters={filters}
      uiPrefs={uiPrefs}
      library={library}
      nav={nav}
      selection={selection}
      results={results}
      categories={categories}
      avatars={avatars}
      q={q}
    />
  );
  const contextActions: ContextAction[] = context
    ? [
        {
          id: 'edit',
          label: q('edit'),
          icon: <SlidersHorizontal size={14} />,
          onClick: () => setDialog(context.ids.length > 1 ? 'batch' : 'edit'),
        },
        {
          id: 'collection',
          label: q('addToCollection'),
          icon: <Folder size={14} />,
          onClick: () => setDialog('collection'),
        },
        {
          id: 'unity',
          label: q('importUnity'),
          icon: <FileBox size={14} />,
          onClick: () => openUnity(context.ids),
        },
        {
          id: 'export',
          label: q('exportZip'),
          icon: <FileArchive size={14} />,
          onClick: () => void exportZip(context.ids),
        },
        {
          id: 'ai',
          label: q('aiTags'),
          icon: <Tags size={14} />,
          disabled: !aiEnabled,
          onClick: () => setFlow('ai'),
        },
        {
          id: 'spending',
          label: q('spending'),
          icon: <Package size={14} />,
          onClick: () => setDialog('spending'),
        },
        {
          id: 'selectAll',
          label: q('selectAll'),
          icon: <CheckCircle2 size={14} />,
          onClick: () => setSelected(matchedIds),
        },
        {
          id: 'update',
          label: q('update'),
          icon: <RefreshCw size={14} />,
          onClick: () => update(context.ids),
        },
        ...(licenseEnabled
          ? [
              {
                id: 'licenses',
                label: licenseText(locale, 'syncLicenses')!,
                icon: <Scale size={14} />,
                onClick: () => syncLicenses(context.ids),
              },
            ]
          : []),
        {
          id: 'download',
          label: q('download'),
          icon: <Download size={14} />,
          disabled: allUnavailable(context.ids),
          onClick: () => download(context.ids),
        },
        ...(context.ids.some((id) => activeDownloads(id).length)
          ? [
              {
                id: 'cancelDownload',
                label: q('cancelDownload'),
                icon: <X size={14} />,
                onClick: () => cancelDownloads(context.ids),
              },
            ]
          : []),
        ...(needsAttention(context.ids)
          ? [
              {
                id: 'dismissAttention',
                label: q('dismissAttention'),
                icon: <BellOff size={14} />,
                onClick: () => dismissAttention(context.ids),
              },
            ]
          : []),
        ...(localSelection(context.ids)
          ? [
              {
                id: 'delete',
                label: q('delete'),
                icon: <Trash2 size={14} />,
                danger: true,
                onClick: () => requestLocalDelete(context.ids),
              },
            ]
          : []),
        ...(onlineSelection(context.ids)
          ? [
              {
                id: 'hide',
                label: q(hiddenSelection(context.ids) ? 'unhide' : 'hide'),
                icon: <EyeOff size={14} />,
                onClick: () => run(hideProducts(context.ids, !hiddenSelection(context.ids))),
              },
            ]
          : []),
        ...(onlineSelection(context.ids) && hasDownloadedFiles(context.ids)
          ? [
              {
                id: 'deleteDownloads',
                label: q('deleteDownloads'),
                icon: <Trash2 size={14} />,
                danger: true,
                onClick: () => requestDeleteDownloads(context.ids),
              },
            ]
          : []),
      ]
    : [];
  const item = (p: Product, list: boolean) => (
    <ProductItem
      key={p.id}
      p={p}
      list={list}
      selectedProducts={selectedProducts}
      selection={selection}
      nav={nav}
      uiPrefs={uiPrefs}
      categories={categories}
      filters={filters}
      q={q}
    />
  );
  const footer = (
    <ResultsFooter
      selectedProducts={selectedProducts}
      library={library}
      edits={edits}
      nav={nav}
      actions={actions}
      notices={notices}
      uiPrefs={uiPrefs}
      selection={selection}
      results={results}
      q={q}
    />
  );
  const themeOptions = [...defaultThemeOptions, ...presets];
  const workflowProps = {
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
      libraryRefreshPending.current || libraryState?.state === 'setup'
        ? libraryState?.path
        : libraryState?.defaultPath,
    defaultLibraryError: libraryRefreshPending.current
      ? loadingError
      : ['setup', 'unconfigured'].includes(libraryState?.state ?? '')
        ? libraryState?.errorCode
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
    onUserPatch: userPatch,
    onBatchUserPatch: batchPatch,
    sensitiveMode: query.sensitive,
    accounts,
    importSeed,
    onNavigate: (route: string) => {
      setFlow(null);
      openSettings(route);
    },
    nativeState,
    onNative,
    onPreferences,
  };
  const appearanceContent = (
    <AppearanceSettings
      uiPrefs={uiPrefs}
      appearance={appearance}
      nav={nav}
      notices={notices}
      q={q}
      themeOptions={themeOptions}
    />
  );
  const inspectorPane = (
    <>
      <div
        role="separator"
        aria-orientation="vertical"
        aria-label={q('resizeDetails')}
        className="drag-divider right-divider"
        onPointerDown={(e) => resize('right', e)}
        tabIndex={0}
        onKeyDown={(e) => {
          if (['ArrowLeft', 'ArrowRight'].includes(e.key)) {
            e.preventDefault();
            setInspectorWidth((value) =>
              Math.max(240, Math.min(900, value + (e.key === 'ArrowLeft' ? 32 : -32))),
            );
          }
        }}
      />
      <aside
        className={`inspector revised-inspector ${wide ? 'wide' : ''}`}
        style={{ width: paneWidth, flexBasis: paneWidth }}
      >
        <header className="inspector-header">
          <strong>{view === 'settings' ? q('settings') : q('details')}</strong>
        </header>
        {view === 'settings' ? (
          <>
            <nav className="embedded-settings-nav">
              {[
                'appearance',
                'features',
                'accounts',
                'storage',
                'download',
                ...(aiEnabled ? ['ai-settings'] : []),
              ].map((route) => (
                <button
                  key={route}
                  className={settingsRoute === route ? 'active' : ''}
                  onClick={() => setSettingsRoute(route)}
                >
                  {route === 'ai-settings' ? 'AI' : q(route === 'download' ? 'downloads' : route)}
                </button>
              ))}
            </nav>
            <div key={settingsRoute} className="embedded-settings-content">
              {settingsRoute === 'appearance' ? (
                appearanceContent
              ) : settingsRoute === 'features' ? (
                <FeaturesSettings
                  locale={locale}
                  preferences={nativeState.preferences}
                  onPreferences={onPreferences}
                  licenseJob={nativeState.jobs.find(
                    (job) =>
                      job.type === 'licenses:sync' && ['queued', 'running'].includes(job.state),
                  )}
                  onNotice={notify}
                />
              ) : (
                <WorkflowContent {...workflowProps} flow={settingsRoute} />
              )}
            </div>
          </>
        ) : visibleSelected.length > 1 ? (
          batchSummary
        ) : active ? (
          <AssetInspector
            product={active}
            categoryLabel={categoryLabel(active.category)}
            locale={locale}
            wide={wide}
            tab={tab}
            onTabChange={setTab}
            sensitiveMode={query.sensitive}
            revealed={revealed.has(active.id)}
            onReveal={() => setRevealed((old) => new Set([...old, active.id]))}
            accountIds={query.accountIds}
            accounts={accounts}
            onOpenProductPage={(url) => run(openExternal(url))}
            actions={singleActions(active)}
            onDismissAttention={() => dismissAttention([active.id])}
            offerEndedFileIds={refusedFiles(active.id)}
            onEdit={edit}
            onTagClick={(tag) => updateQuery({ search: tag })}
            onFileRetry={downloadFile}
            onDownloadAll={() => download([active.id])}
            onCancelDownload={() => cancelDownloads([active.id])}
            cancelPending={activeDownloads(active.id).every((job) => job.cancelRequested)}
            cancellableFileIds={activeDownloads(active.id).flatMap((job) =>
              job.selectedFileIds?.length === 1 && !job.cancelRequested ? job.selectedFileIds : [],
            )}
            onFileCancel={(fileId) => {
              const job = activeDownloads(active.id).find(
                (job) => job.selectedFileIds?.length === 1 && job.selectedFileIds[0] === fileId,
              );
              if (job) run(onNative('storage', 'cancel_job', { jobId: job.id }));
            }}
            onDeleteDownloads={() => requestDeleteDownloads([active.id])}
            onFileDelete={(fileId) => run(deleteDownloads({ fileId }, [active.id]))}
            deleteDisabled={
              deletingDownloads.has(active.id) || activeDownloads(active.id).length > 0
            }
            onLocalFilesAdd={(folder) => addLocalFiles(active.id, folder)}
            onLocalFileRemove={(fileId) =>
              run(
                changeLocalFiles(active.id, {
                  draftId: crypto.randomUUID(),
                  addSources: [],
                  removeFileIds: [fileId],
                }),
              )
            }
            localFilesBusy={localFilePending.has(active.id) || jobs.has(active.id)}
            retryDisabled={nativeState.jobs.some(
              (job) =>
                job.type === 'download' &&
                job.productId === active.id &&
                ['queued', 'running'].includes(job.state) &&
                job.selectedFileIds == null,
            )}
            busyFileIds={nativeState.jobs
              .filter(
                (job) =>
                  job.type === 'download' &&
                  job.productId === active.id &&
                  ['queued', 'running'].includes(job.state),
              )
              .flatMap(
                (job) => job.fileIds ?? job.selectedFileIds ?? active.files.map((file) => file.id),
              )}
            folderAvailable={
              availability?.key === availabilityKey && availability.folderAvailable === true
            }
            openableFileIds={
              availability?.key === availabilityKey ? availability.openableFileIds : []
            }
            onOpenFolder={() =>
              run(onNative('storage', 'open_product_folder', { productId: active.id }))
            }
            onFileOpen={(id) => run(onNative('storage', 'open_file', { fileId: id }))}
          />
        ) : (
          <div className="empty-state">{q('noSelection')}</div>
        )}
      </aside>
    </>
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
  return (
    <div
      className={appClass}
      data-view={view}
      data-theme={appTheme}
      style={{ ...themeTokens, '--nav-width': navWidth + 'px' } as CSSProperties}
    >
      <header className="topbar" inert={updateOpen || !!themeDeleteTarget}>
        <div className="brand">
          <span className="brand-mark">
            <Package size={21} />
          </span>
          <span>
            NekoNeko<span className="brand-light">Asset</span>
          </span>
        </div>
        <nav className="main-nav">
          {['library', 'activity', ...(aiEnabled ? ['ai-suggestions'] : [])].map((name) => (
            <button
              key={name}
              className={view === name ? 'active' : ''}
              onClick={() => setView(name)}
            >
              {name === 'ai-suggestions' ? aiSuggestionsText(locale, 'aiSuggestions') : q(name)}
            </button>
          ))}
        </nav>
        <div className="searchbox">
          <Search size={16} />
          <input
            ref={searchRef}
            aria-label={q('search')}
            placeholder={q('searchPlaceholder')}
            value={query.search}
            onChange={(e) => {
              updateQuery({ search: e.target.value });
              if (view === 'activity' || view === 'ai-suggestions') setView('library');
            }}
          />
          <Button
            title={q('clearSearch')}
            disabled={!query.search}
            onMouseDown={(e) => e.preventDefault()}
            onClick={() => {
              const keyboard = !!document.activeElement?.closest('.searchbox button');
              updateQuery({ search: '' });
              if (keyboard) searchRef.current?.focus();
            }}
          >
            <X size={13} />
          </Button>
          <kbd>Ctrl F</kbd>
        </div>
        <Button title={q('saveSearch')} onClick={() => setDialog('saveSearch')}>
          <Bookmark size={15} />
        </Button>
        <div className="top-actions">
          <Button title={q(syncing ? 'syncing' : 'sync')} onClick={() => setFlow('sync')}>
            <RefreshCw size={15} className={syncing ? 'sync-spinning' : undefined} />
            <span>{q(syncing ? 'syncing' : 'sync')}</span>
          </Button>
          <Button
            title={q('import')}
            onClick={() => {
              setImportSeed(undefined);
              setFlow('import');
            }}
          >
            <Plus size={17} />
            <span>{q('importAssets')}</span>
          </Button>
          <Button title={q('settings')} onClick={toggleSettings}>
            <Settings size={17} />
          </Button>
        </div>
      </header>
      <div className="shell" inert={updateOpen || !!themeDeleteTarget}>
        {view !== 'activity' && view !== 'ai-suggestions' && (
          <>
            {sidebar}
            <div
              role="separator"
              aria-label={q('resizeNavigation')}
              aria-orientation="vertical"
              className="drag-divider left-divider"
              tabIndex={0}
              onPointerDown={(e) => resize('left', e)}
              onKeyDown={(e) => {
                if (['ArrowLeft', 'ArrowRight'].includes(e.key)) {
                  e.preventDefault();
                  setNavWidth((value) =>
                    Math.max(132, Math.min(300, value + (e.key === 'ArrowRight' ? 16 : -16))),
                  );
                }
              }}
            />
          </>
        )}
        <main className="workspace">
          {aiEnabled && (
            <div
              style={{
                display: view === 'ai-suggestions' ? 'flex' : 'none',
                flex: 1,
                minHeight: 0,
              }}
            >
              <AiSuggestions
                active={view === 'ai-suggestions'}
                frozen={
                  updateOpen ||
                  !!themeDeleteTarget ||
                  !!flow ||
                  !!dialog ||
                  !!deleteLocalSelection ||
                  libraryOpening
                }
                onUpdateBlockedChange={reportAiChatUpdateBlocked}
                locale={locale}
                avatarOptions={avatarChoices.map((id) => ({ id, label: avatarLabel(id) }))}
                onOpenProduct={openSuggestedProduct}
              />
            </div>
          )}
          {view === 'activity' ? (
            activity
          ) : view === 'ai-suggestions' ? null : (
            <div className="result-layout">
              <section
                className="result-workspace"
                onKeyDown={resultKeys}
                onDragOver={(e) => {
                  if (e.dataTransfer.types.includes('Files')) {
                    e.preventDefault();
                    e.dataTransfer.dropEffect = 'copy';
                  }
                }}
                onDrop={(e) => e.preventDefault()}
              >
                {toolbar}
                {filtered.length ? (
                  <Results
                    items={filtered}
                    size={thumb}
                    density={density}
                    state={scrollState.current}
                    activeId={focusId}
                    metadataCount={
                      Number(cardColumns.includes('shop')) +
                      Number(cardColumns.includes('acquired'))
                    }
                    render={item}
                    gestures={
                      oneHandedDrag
                        ? {
                            selected: selectedSet,
                            select: (ids, first) => {
                              setSelected(ids);
                              if (first) anchor.current = first;
                            },
                            drop: (ids, category) =>
                              run(
                                userPatch(ids, { category, categoryOverride: true }).then(() =>
                                  notify(
                                    q('movedToCategory')
                                      .replace('{count}', ids.length.toLocaleString(locale))
                                      .replace('{category}', categoryLabel(category)),
                                  ),
                                ),
                              ),
                          }
                        : null
                    }
                  />
                ) : resultsPending ? (
                  <div className="empty-state results-loading" role="status">
                    <LoaderCircle size={25} className="progress-spin" aria-hidden="true" />
                    <h2>{q(loaded && query.search.trim() ? 'searching' : 'loading')}</h2>
                  </div>
                ) : (
                  <div className="empty-state">
                    <Search size={25} />
                    <h2>{q(!catalogueTotal ? 'emptyLibrary' : 'noResults')}</h2>
                    {products.length > 0 && (
                      <Button onClick={clearFilters}>{q('clearFilters')}</Button>
                    )}
                  </div>
                )}
                {footer}
              </section>
              {inspector && inspectorPane}
            </div>
          )}
        </main>
      </div>
      <footer className="statusbar" inert={updateOpen || !!themeDeleteTarget}>
        <button
          type="button"
          className="app-version"
          title={updateText(locale, 'check')}
          aria-label={updateText(locale, 'check')}
          disabled={!updateCanOpen}
          onClick={() => {
            setUpdateOpen(true);
            setUpdateRequest((value) => value + 1);
          }}
        >
          v{appVersion}
          {updateAvailable && <span className="app-update-dot" />}
        </button>
        <span>{q('totalProducts').replace('{count}', catalogueTotal.toLocaleString(locale))}</span>
        <span className="spacer" />
        {(collectionBusy || notice || loadingError) && (
          <span role="status" className="inline-notice">
            <span title={notice || (loadingError ? q(loadingError) : q('processing'))}>
              {notice || (loadingError ? q(loadingError) : q('processing'))}
            </span>
            {!collectionBusy && (
              <button
                title={q('close')}
                onClick={() => {
                  setNotice('');
                  setLoadingError('');
                }}
              >
                <X size={12} />
              </button>
            )}
          </span>
        )}
      </footer>
      <UpdatePrompt
        locale={locale}
        currentVersion={appVersion}
        ready={loaded && setupDone && libraryState.state === 'ready'}
        canPrompt={!updateBlocker}
        installBlocker={updateBlocker}
        currentInstallBlocker={currentUpdateBlocker}
        tasksResume={nativeState.jobs.some(resumesAfterRestart)}
        open={updateOpen}
        request={updateRequest}
        onOpenChange={setUpdateOpen}
        onAvailableChange={setUpdateAvailable}
        onBeforeInstall={prepareUpdate}
      />
      <Workflows {...workflowProps} flow={flow} />
      <EditDialogs
        aiFeaturesEnabled={aiEnabled}
        licenseEnabled={licenseEnabled}
        dialog={
          [
            'edit',
            'batch',
            'saveSearch',
            'collection',
            'theme',
            'manageCollections',
            'manageCategories',
            'manageSearches',
          ].includes(dialog || '')
            ? dialog
            : null
        }
        locale={locale}
        products={products}
        selected={chosen}
        active={active}
        onClose={() => {
          setDialog(null);
          setThemePreview(null);
        }}
        onPatch={userPatch}
        onBatchPatch={batchPatch}
        onNotice={notify}
        collections={collections}
        collectionIds={Object.fromEntries(collectionRows.current.map((row) => [row.name, row.id]))}
        collectionBusy={collectionBusy}
        savedSearches={saved}
        tagOptions={nativeState.tagOptions}
        avatarOptions={avatarOptions}
        avatarLabel={avatarLabel}
        onRefreshAvatars={refreshAvatarOptions}
        onSaveSearch={async (name) => {
          await onNative('catalogue', 'save_search', {
            op: 'create',
            name,
            query: structuredClone(query),
            sort,
            direction,
          });
        }}
        customCategories={categoryList}
        onCreateCategory={(name) => changeCategories({ op: 'create', name })}
        onRenameCategory={(id, name) => changeCategories({ op: 'update', id, name })}
        onDeleteCategory={(id) => changeCategories({ op: 'delete', id })}
        onCreateCollection={async (name) => {
          await onNative('catalogue', 'collection', { op: 'create', name });
        }}
        onRenameCollection={async (oldName, name) => {
          const id = collectionRows.current.find((c) => c.name === oldName)?.id;
          await onNative('catalogue', 'collection', { op: 'update', id, name });
          if (collection === oldName) setCollection(name);
        }}
        onDeleteCollection={async (_name, id) => {
          if (!id) throw new Error('entryUnavailable');
          const active = collectionRows.current.some(
            (row) => row.id === id && row.name === collection,
          );
          await onNative('catalogue', 'collection', { op: 'delete', id });
          if (active) clearFilters();
        }}
        onRenameSearch={async (index, name) => {
          await onNative('catalogue', 'save_search', { op: 'update', id: saved[index]?.id, name });
        }}
        onDeleteSearch={async (_index, id) => {
          if (!id) throw new Error('entryUnavailable');
          const index = saved.findIndex((search) => search.id === id);
          await onNative('catalogue', 'save_search', { op: 'delete', id });
          if (index >= 0) {
            if (savedScope === index) clearFilters();
            else if (savedScope !== null && savedScope > index) setSavedScope(savedScope - 1);
          }
        }}
        onCollection={async (name, ids, included) => {
          const id = collectionRows.current.find((c) => c.name === name)?.id;
          if (!id) throw new Error('catalogue.not_found');
          await onNative(
            'catalogue',
            'collection',
            included
              ? { op: 'add_members', collectionIds: [id], ids }
              : { op: 'remove_members', id, ids },
          );
        }}
        onLocalFiles={changeLocalFiles}
        onFetchMetadata={fetchProductMetadata}
        themeTokens={selectedThemeTokens}
        themeName={selectedThemeName}
        onThemePreview={setThemePreview}
        onThemeSave={choosePreset}
      />
      {dialog === 'spending' && (
        <SpendingDialog
          locale={locale}
          productIds={chosen}
          accountIds={query.accountIds ?? undefined}
          onClose={() => setDialog(null)}
        />
      )}
      {themeDeleteTarget && (
        <ConfirmDelete
          locale={locale}
          kind="theme"
          name={themeDeleteTarget.name}
          onDelete={() => deleteTheme(themeDeleteTarget.id)}
          onClose={() => setThemeDeleteTarget(null)}
        />
      )}
      {deleteLocalSelection && (
        <DeleteLocalProducts
          locale={locale}
          {...deleteLocalSelection}
          onDelete={deleteLocalProducts}
          onClose={() => setDeleteLocalSelection(null)}
        />
      )}
      {productConfirm && (
        <ConfirmProducts
          locale={locale}
          {...(productConfirm.kind === 'downloadAll'
            ? { title: q('downloadAll'), message: q('confirmDownloadAll'), confirm: q('download') }
            : {
                title: q('deleteDownloads'),
                message: q('confirmDeleteDownloads'),
                confirm: q('delete'),
                danger: true,
              })}
          count={productConfirm.ids.length}
          names={productConfirm.names}
          onConfirm={async () => {
            if (productConfirm.kind === 'downloadAll')
              watchDownloads(
                await invokeAction('storage', 'enqueue_products', {
                  productIds: productConfirm.ids,
                }),
              );
            else await deleteDownloads({ productIds: productConfirm.ids }, productConfirm.ids);
          }}
          onClose={() => setProductConfirm(null)}
        />
      )}
      {context && (
        <LibraryContextMenu
          {...context}
          actions={contextActions}
          onClose={() => setContext(null)}
        />
      )}
      {licenseOpen && licenseEnabled && (
        <LicenseFilterDialog
          locale={locale}
          value={query.license ?? {}}
          onChange={(cells) => updateQuery({ license: Object.keys(cells).length ? cells : null })}
          onClose={() => {
            setLicenseOpen(false);
            requestAnimationFrame(() => licenseButton.current?.focus({ preventScroll: true }));
          }}
        />
      )}
      {dialog === 'avatars' && (
        <SmallDialog
          className="avatar-dialog"
          label={q('manageAvatars')}
          close={() => setDialog(null)}
          q={q}
        >
          <AvatarManager
            options={avatarOptions}
            enabled={avatarEnabled}
            label={avatarLabel}
            onChange={changeAvatars}
            onEdit={changeAvatarDetails}
            q={q}
          />
        </SmallDialog>
      )}
      {libraryOpening && (
        <div className="modal-backdrop">
          <LibraryAccess
            locale={locale}
            status={libraryState}
            busy
            onChoose={() => {}}
            onRetry={() => {}}
          />
        </div>
      )}
    </div>
  );
}
