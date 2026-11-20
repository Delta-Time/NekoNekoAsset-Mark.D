import type { CSSProperties } from 'react';
import {
  Search,
  Grid2X2,
  List,
  Columns2,
  RefreshCw,
  Download,
  Folder,
  Plus,
  MoreHorizontal,
  X,
  FileArchive,
  FileBox,
  CheckCircle2,
  AlertCircle,
  Bookmark,
  Package,
  SlidersHorizontal,
  Tags,
  Settings,
  Eye,
  EyeOff,
  Trash2,
  LoaderCircle,
  BellOff,
  Scale,
} from 'lucide-react';
import {
  categoryKeys,
  statusKeys,
  isProductDownloaded,
  getProductDisplayStatus,
  defaultSortDirection,
  thumbnailSizes,
  currentPriceLabel,
  cardPriceLabel,
  fileCountLabel,
} from './model';
import type { Product, Locale, SortDirection } from './model';
import { version as appVersion } from '../package.json';
import { localeNames } from './i18n';
import Preview from './Preview';
import AvatarManager from './AvatarManager';
import AssetInspector from './AssetInspector';
import DeleteLocalProducts from './DeleteLocalProducts';
import ConfirmProducts from './ConfirmProducts';
import ConfirmDelete from './ConfirmDelete';
import ScaleControl from './ScaleControl';
import Toggle from './Toggle';
import { contentLang } from './content-lang';
import { CategoryIcon } from './LibraryIcons';
import UpdatePrompt from './UpdatePrompt';
import { resumesAfterRestart, updateText } from './update-i18n';
import { resetText } from './settings-reset-i18n';
import './settings-reset.css';
import AiSuggestions from './AiSuggestions';
import FeaturesSettings from './FeaturesSettings';
import SpendingDialog from './SpendingDialog';
import { LibraryContextMenu, ResponsiveLibraryToolbar } from './LibraryMenus';
import type { ContextAction } from './LibraryMenus';
import ActivityHistory from './ActivityHistory';
import { aiSuggestionsText } from './ai-suggestions-i18n';
import ThemePicker, { defaultThemeOptions } from './ThemePicker';
import Workflows, { WorkflowContent, LibraryAccess, isStoreJob } from './Workflows';
import EditDialogs, { exportThemeConfig } from './EditDialogs';
import { LicenseFilterDialog, licenseSummary } from './Licenses';
import { licenseText } from './license-i18n';
import {
  invokeAction,
  errorCode,
  openExternal,
  fetchProductMetadata,
  cancelAi,
  retryAi,
  cancelUnity,
  retryUnityInventory,
} from './backend';
import { nativeWorkflowText } from './native-workflow-i18n';
import Button from './shared/Button';
import SmallDialog from './shared/SmallDialog';
import { appText } from './shared/app-text';
import ProductStatus from './features/results/ProductStatus';
import Results from './features/results/Results';
import { configuredLibraryRoot } from './features/library/library-root';
import { acquisitionKinds, fresh, leavingHidden } from './features/filters/query';
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
export default function App() {
  const uiPrefs = useUiPreferences();
  const {
    locale,
    setLocale,
    theme,
    setTheme,
    presets,
    thumb,
    setThumb,
    density,
    setDensity,
    navWidth,
    setNavWidth,
    inspectorWidth,
    setInspectorWidth,
    cardColumns,
    columns,
    setColumns,
    uiScale,
    roundedCorners,
    setRoundedCorners,
    reduceMotion,
    setReduceMotion,
    bundledFonts,
    setBundledFonts,
    oneHandedDrag,
    setOneHandedDrag,
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
    setInspector,
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
    batchEnabled,
  } = library;
  const selection = useSelection();
  const { setSelected, focusId, setFocusId, revealed, setRevealed, anchor, clearSelection } =
    selection;
  const filters = useFilters({ selection, notices, nav, library });
  const {
    query,
    setQuery,
    sort,
    setSort,
    direction,
    setDirection,
    collection,
    setCollection,
    savedScope,
    setSavedScope,
    updateQuery,
    openSuggestedProduct,
    clearFilters,
    changeSort,
    defaultAccounts,
    filtersActive,
  } = filters;
  const collectionStore = useCollections({ notices, filters, q });
  const { collections, collectionRows, collectionBusy } = collectionStore;
  const results = useResults({ library, filters, collectionStore, notices, q });
  const {
    products,
    catalogueTotal,
    scrollState,
    queryResult,
    queryBusy,
    queryKey,
    filtered,
    matchedIds,
    resultsPending,
    quickCounts,
  } = results;
  const selectedProducts = useSelectedProducts({ results, selection, library, nav, notices, q });
  const {
    visibleSelected,
    selectedSet,
    productMap,
    active,
    chosen,
    availability,
    availabilityKey,
    select,
    keySelect,
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
  const { openSelectedLibrary, chooseLibrary, refreshState, refreshAvatarOptions, onNative } =
    native;
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
    selectedTheme,
    selectedThemeTokens,
    selectedThemeName,
    themeTokens,
    choosePreset,
    themeDeleteBusy,
    themeDeleteTarget,
    setThemeDeleteTarget,
    deleteTheme,
    changeScale,
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
  const cardArt = (p: Product, compact = false) => (
    <div
      className={`thumb ${p.sensitive && query.sensitive === 'blur' && !revealed.has(p.id) ? 'blurred' : ''}`}
    >
      {!compact && columns.includes('category') && (
        <span className="card-category" title={categoryLabel(p.category)}>
          {categoryLabel(p.category)}
        </span>
      )}
      <Preview
        src={p.thumbnailHidden ? undefined : p.thumbnailOverrideUrl || p.previewUrl}
        title={p.title}
        className="asset-art"
        lazy
      />{' '}
      {p.sensitive && query.sensitive === 'blur' && !revealed.has(p.id) && (
        <button
          type="button"
          className="reveal"
          title={q('revealImage')}
          aria-label={q('showR18')}
          onClick={(e) => {
            if (e.shiftKey || e.ctrlKey || e.metaKey) return;
            e.stopPropagation();
            setRevealed((old) => new Set([...old, p.id]));
          }}
          onKeyDown={(e) => {
            if (e.key === 'Enter' || e.key === ' ') e.stopPropagation();
          }}
        >
          <span className="reveal-label">{compact ? <Eye size={14} /> : q('showR18')}</span>
        </button>
      )}
    </div>
  );
  // Import joins Download once a selected file is on disk, and takes its place when every product is ready.
  const batchActions = (ids: string[]) => {
    const ready =
        ids.length > 0 &&
        ids.every((id) => {
          const p = productMap.get(id);
          return !!p && getProductDisplayStatus(p) === 'ready' && isProductDownloaded(p);
        }),
      importable = ids.some((id) => (productMap.get(id)?.downloaded ?? 0) > 0);
    return (
      <div className="scope-actions" role="group" aria-label={q('selected')}>
        <div className="scope-actions-left">
          {batchEnabled.edit && (
            <Button title={q('edit')} onClick={edit} disabled={!ids.length}>
              <SlidersHorizontal size={14} />
              <span>{q('edit')}</span>
            </Button>
          )}
          {batchEnabled.collection && (
            <Button
              title={q('addToCollection')}
              onClick={() => setDialog('collection')}
              disabled={!ids.length}
            >
              <Folder size={14} />
              <span>{q('addToCollection')}</span>
            </Button>
          )}
          {batchEnabled.export && (
            <Button title={q('exportZip')} onClick={() => exportZip(ids)} disabled={!ids.length}>
              <FileArchive size={14} />
              <span>{q('exportZip')}</span>
            </Button>
          )}
          {aiEnabled && batchEnabled.ai && (
            <Button title={q('aiTags')} onClick={() => setFlow('ai')} disabled={!ids.length}>
              <Tags size={14} />
              <span>{q('aiTags')}</span>
            </Button>
          )}
          {batchEnabled.spending && (
            <Button
              title={q('spending')}
              onClick={() => setDialog('spending')}
              disabled={!ids.length}
            >
              <Package size={14} />
              <span>{q('spending')}</span>
            </Button>
          )}
          {batchEnabled.delete && localSelection(ids) && (
            <Button title={q('delete')} className="danger" onClick={() => requestLocalDelete(ids)}>
              <Trash2 size={14} />
              <span>{q('delete')}</span>
            </Button>
          )}
          {batchEnabled.hide && onlineSelection(ids) && (
            <Button
              title={q(hiddenSelection(ids) ? 'unhide' : 'hide')}
              onClick={() => run(hideProducts(ids, !hiddenSelection(ids)))}
            >
              <EyeOff size={14} />
              <span>{q(hiddenSelection(ids) ? 'unhide' : 'hide')}</span>
            </Button>
          )}
        </div>
        <div className="scope-actions-right">
          {batchEnabled.update && (
            <Button title={q('update')} onClick={() => update(ids)} disabled={!ids.length}>
              <RefreshCw size={14} />
              <span>{q('update')}</span>
            </Button>
          )}
          {licenseEnabled && batchEnabled.licenses && (
            <Button
              title={licenseText(locale, 'syncLicenses')}
              onClick={() => syncLicenses(ids)}
              disabled={!ids.length}
            >
              <Scale size={14} />
              <span>{licenseText(locale, 'syncLicenses')}</span>
            </Button>
          )}
          {batchEnabled.download && !ready && (
            <Button
              title={q('download')}
              primary
              onClick={() => download(ids)}
              disabled={!ids.length || allUnavailable(ids)}
            >
              <Download size={14} />
              <span>{q('download')}</span>
            </Button>
          )}
          {batchEnabled.unity && importable && (
            <Button
              title={q('importUnity')}
              className="unity-action"
              primary={ready}
              onClick={() => openUnity(ids)}
            >
              <FileBox size={14} />
              <span>{q('importUnity')}</span>
            </Button>
          )}
        </div>
      </div>
    );
  };
  const singleActions = (p: Product) => {
    const ready = getProductDisplayStatus(p) === 'ready' && isProductDownloaded(p),
      downloads = activeDownloads(p.id);
    return (
      <div className="detail-actions">
        {!downloads.length && allUnavailable([p.id]) ? (
          <Button className="unavailable-action" title={q('downloadUnavailable')} disabled>
            <span>{q('downloadUnavailable')}</span>
          </Button>
        ) : downloads.length ? (
          <Button
            className="cancel-download"
            title={q('cancelDownload')}
            disabled={downloads.every((job) => job.cancelRequested)}
            onClick={() => cancelDownloads([p.id])}
          >
            <X size={14} />
            <span>{q('cancel')}</span>
          </Button>
        ) : (
          <Button
            primary={!ready}
            className={ready ? 'unity-action' : ''}
            title={q(ready ? 'importUnity' : 'download')}
            onClick={() => (ready ? openUnity([p.id]) : download([p.id]))}
          >
            {ready ? <FileBox size={14} /> : <Download size={14} />}
            <span>{q(ready ? 'importUnity' : 'download')}</span>
          </Button>
        )}
        <Button onClick={edit}>
          <SlidersHorizontal size={14} />
          {q('edit')}
        </Button>
        <details className="action-menu">
          <summary aria-label={q('more')}>
            <MoreHorizontal size={16} />
          </summary>
          <div className="popup-menu">
            {[
              'update',
              ...(licenseEnabled ? ['syncLicenses'] : []),
              'download',
              ...(chosen.some((id) => activeDownloads(id).length) ? ['cancelDownload'] : []),
              ...(aiEnabled ? ['ai'] : []),
              'importUnity',
              'exportZip',
              ...(needsAttention(chosen) ? ['dismissAttention'] : []),
              ...(p.provider === 'local'
                ? ['delete']
                : [
                    p.hidden ? 'unhide' : 'hide',
                    ...(onlineSelection(chosen) && hasDownloadedFiles(chosen)
                      ? ['deleteDownloads']
                      : []),
                  ]),
            ].map((key) => (
              <button
                key={key}
                className={key === 'delete' || key === 'deleteDownloads' ? 'danger' : undefined}
                disabled={key === 'download' && allUnavailable(chosen)}
                onClick={(e) => {
                  e.currentTarget.closest('details')?.removeAttribute('open');
                  if (key === 'delete') requestLocalDelete([p.id]);
                  else if (key === 'hide' || key === 'unhide')
                    run(hideProducts([p.id], key === 'hide'));
                  else if (key === 'update') update(chosen);
                  else if (key === 'syncLicenses') syncLicenses(chosen);
                  else if (key === 'download') download(chosen);
                  else if (key === 'cancelDownload') cancelDownloads(chosen);
                  else if (key === 'deleteDownloads') requestDeleteDownloads(chosen);
                  else if (key === 'dismissAttention') dismissAttention(chosen);
                  else if (key === 'ai') setFlow('ai');
                  else if (key === 'importUnity') openUnity(chosen);
                  else exportZip(chosen);
                }}
              >
                {q(key)}
              </button>
            ))}
          </div>
        </details>
      </div>
    );
  };
  const batchSummary = (
    <div className="revised-batch">
      <div className="batch-stack" aria-hidden="true">
        {products
          .filter((p) => selectedSet.has(p.id))
          .slice(0, 5)
          .map((p, k) => (
            <span
              key={p.id}
              className={
                p.sensitive && query.sensitive === 'blur' && !revealed.has(p.id)
                  ? 'blurred'
                  : undefined
              }
              style={{ '--k': k } as CSSProperties}
            >
              <Preview
                src={p.thumbnailHidden ? undefined : p.thumbnailOverrideUrl || p.previewUrl}
                title=""
                className="asset-art"
              />
            </span>
          ))}
      </div>
      <h2 className="batch-count">
        <b>{visibleSelected.length.toLocaleString(locale)}</b>
        <span>{q('selected')}</span>
      </h2>
      {batchActions(visibleSelected)}
      <div className="selected-products">
        {products
          .filter((p) => selectedSet.has(p.id))
          .slice(0, 12)
          .map((p) => (
            <div key={p.id} title={p.title}>
              <span lang={contentLang(p.title)}>{p.title}</span>
              <ProductStatus p={p} q={q} />
            </div>
          ))}
      </div>
    </div>
  );
  const sidebar = (
    <aside className="sidebar" style={{ width: navWidth, flexBasis: navWidth }}>
      <div className="quick-scopes">
        <button
          className={`nav-item quick-scope ${query.category === 'all' && !collection && query.avatar === 'all' && query.quick === 'all' ? 'active' : ''}`}
          onClick={clearFilters}
        >
          <Grid2X2 size={15} />
          <span>{q('allAssets')}</span>
          <span className="count">{quickCounts.all.toLocaleString(locale)}</span>
        </button>
        <button
          className={`nav-item quick-scope ${query.quick === 'downloaded' ? 'active' : ''}`}
          onClick={() =>
            updateQuery({ quick: 'downloaded', category: 'all', avatar: 'all', status: 'all' })
          }
        >
          <CheckCircle2 size={15} />
          <span>{q('downloaded')}</span>
          <span className="count">{quickCounts.downloaded.toLocaleString(locale)}</span>
        </button>
        <button
          className={`nav-item quick-scope ${query.quick === 'attention' ? 'active' : ''}`}
          onClick={() =>
            updateQuery({ quick: 'attention', category: 'all', avatar: 'all', status: 'all' })
          }
        >
          <AlertCircle size={15} />
          <span>{q('attention')}</span>
          <span className="count">{quickCounts.attention.toLocaleString(locale)}</span>
        </button>
        <button
          className={`nav-item quick-scope ${query.quick === 'hidden' ? 'active' : ''}`}
          onClick={() => {
            updateQuery({ ...fresh, sensitive: query.sensitive, quick: 'hidden' });
            setCollection('');
          }}
        >
          <EyeOff size={15} />
          <span>{q('hidden')}</span>
          <span className="count">{(quickCounts.hidden ?? 0).toLocaleString(locale)}</span>
        </button>
      </div>
      <details className="nav-foldout" open>
        <summary>{q('categories')}</summary>
        {categoryKeys.map((key) => (
          <button
            className={`nav-item category-item ${query.category === key ? 'active' : ''}`}
            key={key}
            data-drop-category={key}
            onClick={() => updateQuery({ category: key })}
          >
            <CategoryIcon category={key} />
            <span>{q(key)}</span>
          </button>
        ))}
      </details>
      <section className="nav-avatar-section">
        <div className="nav-section-heading">
          <details className="nav-foldout" open>
            <summary>{q('customCategories')}</summary>
            {categoryList.map((category) => (
              <button
                className={`nav-item category-item custom-category ${query.category === category.id ? 'active' : ''}`}
                key={category.id}
                title={category.name}
                data-drop-category={category.id}
                onClick={() => updateQuery({ category: category.id })}
              >
                <CategoryIcon category={category.id} />
                <span>{category.name}</span>
              </button>
            ))}
          </details>
          <button
            className="avatar-manager"
            title={q('customCategories')}
            aria-label={q('customCategories')}
            onClick={() => setDialog('manageCategories')}
          >
            +
          </button>
        </div>
      </section>
      <section className="nav-avatar-section">
        <div className="nav-section-heading">
          <details className="nav-foldout" open>
            <summary>{q('compatibleAvatars')}</summary>
            {avatarChoices.map((avatar) => (
              <button
                key={avatar}
                title={avatarLabel(avatar)}
                className={`nav-item avatar-row ${query.avatar === avatar ? 'active' : ''}`}
                onClick={() => updateQuery({ avatar })}
              >
                <span lang={contentLang(avatarLabel(avatar))}>{avatarLabel(avatar)}</span>
                <small>
                  {avatarOptions.find((option) => option.id === avatar)?.matchingProductCount ?? 0}
                </small>
              </button>
            ))}
          </details>
          <button
            className="avatar-manager"
            title={q('manageAvatars')}
            aria-label={q('manageAvatars')}
            onClick={() => {
              setDialog('avatars');
              run(refreshAvatarOptions());
            }}
          >
            +
          </button>
        </div>
      </section>
      <section className="nav-avatar-section">
        <div className="nav-section-heading">
          <details className="nav-foldout" open>
            <summary>{q('collections')}</summary>
            {Object.keys(collections).map((name) => (
              <button
                className={`nav-item ${collection === name ? 'active' : ''}`}
                key={name}
                title={name}
                onClick={() => {
                  setCollection(name);
                  updateQuery(leavingHidden(query));
                }}
              >
                <Folder size={13} />
                <span>{name}</span>
              </button>
            ))}
          </details>
          <button
            className="avatar-manager"
            title={q('collections')}
            aria-label={q('collections')}
            onClick={() => setDialog('manageCollections')}
          >
            +
          </button>
        </div>
      </section>
      <section className="nav-avatar-section">
        <div className="nav-section-heading">
          <details className="nav-foldout" open>
            <summary>{q('savedSearches')}</summary>
            {saved.map((search, i) => (
              <button
                className={`nav-item ${savedScope === i ? 'active' : ''}`}
                key={i}
                title={search.name}
                onClick={() => {
                  setQuery({
                    ...fresh,
                    ...search.query,
                    sensitive: query.sensitive,
                    accountIds: search.query.accountIds ?? null,
                  });
                  setSort(search.sort || 'acquired');
                  setDirection(search.direction || defaultSortDirection(search.sort || 'acquired'));
                  setCollection('');
                  setSavedScope(i);
                  clearSelection();
                }}
              >
                <Bookmark size={13} />
                <span>{search.name}</span>
              </button>
            ))}
          </details>
          <button
            className="avatar-manager"
            title={q('savedSearches')}
            aria-label={q('savedSearches')}
            onClick={() => setDialog('manageSearches')}
          >
            +
          </button>
        </div>
      </section>
    </aside>
  );
  const filterChip = (id: string, label: string, value: string, onClear: () => void) => (
    <span key={id} className="filter-chip">
      <span title={label + ': ' + value}>
        {label}: {value}
      </span>
      <button
        aria-label={q('clearFilters') + ' · ' + label}
        title={q('clearFilters') + ' · ' + label}
        onClick={onClear}
      >
        <X size={12} />
      </button>
    </span>
  );
  const accountControl = (
    <details className="account-scope">
      <summary aria-label={q('accountScope')}>
        {query.accountIds === null
          ? q('accountAll')
          : query.accountIds.length
            ? q('selectedAccounts') + ' · ' + query.accountIds.length
            : query.includeOffline
              ? nativeWorkflowText(locale, 'offlineFiles')
              : q('noAccounts')}
      </summary>
      <div className="popup-menu">
        <label className="check-row">
          <input
            type="checkbox"
            checked={query.accountIds === null}
            onChange={(e) => updateQuery({ accountIds: e.target.checked ? null : [] })}
          />
          {q('accountAll')}
        </label>
        {accounts.map((account) => (
          <label className="check-row" key={account.id}>
            <input
              type="checkbox"
              checked={query.accountIds === null || query.accountIds.includes(account.id)}
              onChange={(e) => {
                const ids = query.accountIds ?? accounts.map((account) => account.id);
                updateQuery({
                  accountIds: e.target.checked
                    ? [...new Set([...ids, account.id])]
                    : ids.filter((id) => id !== account.id),
                });
              }}
            />
            <span>
              {account.store} · {account.name}
            </span>
          </label>
        ))}
        <label className="check-row">
          <input
            type="checkbox"
            checked={query.includeOffline}
            onChange={(e) => updateQuery({ includeOffline: e.target.checked })}
          />
          <span>{nativeWorkflowText(locale, 'offlineFiles')}</span>
        </label>
      </div>
    </details>
  );
  const acquisitionControl = (
    <details className="account-scope acquisition-scope">
      <summary aria-label={q('acquisitionFilter')}>
        {(query.acquisition ?? acquisitionKinds)
          .map((kind) => q(kind + (query.acquisition?.length === 1 ? 'Assets' : 'Short')))
          .join(' · ') || q('noAssets')}
      </summary>
      <div className="popup-menu">
        {acquisitionKinds.map((kind) => (
          <label className="check-row" key={kind}>
            <input
              type="checkbox"
              checked={!query.acquisition || query.acquisition.includes(kind)}
              onChange={(e) => {
                const kinds = acquisitionKinds.filter((item) =>
                  item === kind
                    ? e.target.checked
                    : !query.acquisition || query.acquisition.includes(item),
                );
                updateQuery({
                  acquisition: kinds.length === acquisitionKinds.length ? null : kinds,
                });
              }}
            />
            <span>{q(kind + 'Assets')}</span>
          </label>
        ))}
      </div>
    </details>
  );
  const sortControls = (
    <>
      <select aria-label={q('sort')} value={sort} onChange={(e) => changeSort(e.target.value)}>
        {['acquired', 'title', 'price', 'status'].map((key) => (
          <option key={key} value={key}>
            {q(key)}
          </option>
        ))}
      </select>
      <select
        className="sort-direction"
        title={q(direction === 'asc' ? 'ascending' : 'descending')}
        aria-label={q('sortDirection')}
        value={direction}
        onChange={(e) => {
          setDirection(e.target.value as SortDirection);
          setSavedScope(null);
          clearSelection();
        }}
      >
        <option value="asc">{q('ascending')}</option>
        <option value="desc">{q('descending')}</option>
      </select>
      {sort === 'status' && (
        <select
          aria-label={q('status')}
          value={query.status}
          onChange={(e) => updateQuery({ status: e.target.value })}
        >
          <option value="all">{q('allStates')}</option>
          {statusKeys.map((status) => (
            <option key={status} value={status}>
              {q(status)}
            </option>
          ))}
        </select>
      )}
      <span className="toolbar-spacer" />
      <span className="result-total">
        <b>{resultsPending ? '…' : matchedIds.length.toLocaleString(locale)}</b> {q('products')}
      </span>
      <Button title={q('details')} onClick={() => setInspector((value) => !value)}>
        <Columns2 size={14} />
      </Button>
    </>
  );
  const licenseControl = licenseEnabled && (
    <button
      type="button"
      ref={licenseButton}
      className="license-scope"
      aria-pressed={!!query.license}
      onClick={() => setLicenseOpen(true)}
    >
      {licenseText(locale, 'license')}
      {query.license ? ` · ${Object.keys(query.license).length.toLocaleString(locale)}` : ''}
    </button>
  );
  const chips = (
    <>
      {query.search &&
        filterChip('search', q('searchFilter'), query.search, () => updateQuery({ search: '' }))}
      {query.category !== 'all' &&
        filterChip('category', q('categoryFilter'), categoryLabel(query.category), () =>
          updateQuery({ category: 'all' }),
        )}
      {query.avatar !== 'all' &&
        filterChip('avatar', q('avatarFilter'), avatarLabel(query.avatar), () =>
          updateQuery({ avatar: 'all' }),
        )}
      {collection &&
        filterChip('collection', q('collectionFilter'), collection, () => {
          setCollection('');
          clearSelection();
        })}
      {!defaultAccounts &&
        filterChip(
          'accounts',
          q('accountFilter'),
          [
            ...(query.accountIds === null
              ? [q('accountAll')]
              : accounts
                  .filter((account) => query.accountIds?.includes(account.id))
                  .map((account) => account.name)),
            ...(query.includeOffline ? [nativeWorkflowText(locale, 'offlineFiles')] : []),
          ].join(', ') || q('noAccounts'),
          () => updateQuery({ accountIds: null, includeOffline: true }),
        )}
      {query.acquisition &&
        filterChip(
          'acquisition',
          q('acquisitionFilter'),
          query.acquisition.map((kind) => q(kind + 'Short')).join(', ') || q('noAssets'),
          () => updateQuery({ acquisition: null }),
        )}
      {licenseEnabled &&
        query.license &&
        filterChip(
          'license',
          licenseText(locale, 'license')!,
          licenseSummary(locale, query.license),
          () => updateQuery({ license: null }),
        )}
      {query.quick !== 'all' &&
        filterChip('quick', q('stateFilter'), q(query.quick), () => updateQuery({ quick: 'all' }))}
      {query.status !== 'all' &&
        filterChip('status', q('stateFilter'), q(query.status), () =>
          updateQuery({ status: 'all' }),
        )}
      {(sort !== 'acquired' || direction !== 'desc') &&
        filterChip(
          'sort',
          q('sortFilter'),
          q(sort) + ' · ' + q(direction === 'asc' ? 'ascending' : 'descending'),
          () => {
            setSort('acquired');
            setDirection('desc');
            clearSelection();
          },
        )}
      {filtersActive && (
        <Button className="filter-clear-all" title={q('clearFilters')} onClick={clearFilters}>
          <X size={14} />
        </Button>
      )}
    </>
  );
  const toolbar = (
    <ResponsiveLibraryToolbar
      accounts={
        <>
          {accountControl}
          {acquisitionControl}
          {licenseControl}
        </>
      }
      controls={sortControls}
      chips={chips}
      label={q('sortingMenu')}
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
    <div
      id={'product-' + p.id}
      data-product-id={p.id}
      key={p.id}
      role="option"
      aria-selected={selectedSet.has(p.id)}
      tabIndex={0}
      className={
        (list ? 'revised-row' : 'product-card') + ' ' + (selectedSet.has(p.id) ? 'selected' : '')
      }
      onClick={(e) => select(p, e)}
      onContextMenu={(e) => {
        e.preventDefault();
        const ids = selectedSet.has(p.id) ? visibleSelected : [p.id];
        if (!selectedSet.has(p.id)) {
          setSelected(ids);
          setFocusId(p.id);
        }
        setContext({ x: e.clientX, y: e.clientY, ids });
      }}
      onDragStart={(e) => e.preventDefault()}
      onMouseDown={(e) => {
        if (e.shiftKey || e.ctrlKey || e.metaKey) e.preventDefault();
      }}
      onKeyDown={(e) => keySelect(e, p)}
    >
      {(!list || density !== 40) && (
        <div className={list ? 'row-thumb' : ''}>{cardArt(p, list)}</div>
      )}
      <div className="product-info">
        <strong className="product-title" lang={contentLang(p.title)} title={p.title}>
          {p.title}
        </strong>
        {columns.includes('shop') && (!list || density !== 40) && (
          <span
            className="product-shop"
            lang={p.provider === 'local' ? undefined : contentLang(p.shop)}
            title={p.provider === 'local' ? q('localProductShop') : p.shop || q('unknownShop')}
          >
            {p.provider === 'local' ? q('localProductShop') : p.shop || q('unknownShop')}
          </span>
        )}
        {list ? (
          <ProductStatus p={p} q={q} />
        ) : (
          <>
            {columns.includes('acquired') && (
              <span
                className="card-acquired"
                title={q('acquired') + ': ' + (p.acquired || q('unknown'))}
              >
                {p.acquired || q('unknown')}
              </span>
            )}
            <div
              className={
                'card-bottom ' +
                ((columns.includes('price') && cardPriceLabel(locale, p)) ||
                (columns.includes('files') && p.downloaded > 0)
                  ? 'has-metrics'
                  : '')
              }
            >
              <ProductStatus p={p} q={q} />
              <span className="card-metrics">
                {columns.includes('price') && cardPriceLabel(locale, p) && (
                  <span className="card-price" title={cardPriceLabel(locale, p)!}>
                    {cardPriceLabel(locale, p)}
                  </span>
                )}
                {columns.includes('files') && p.downloaded > 0 && (
                  <span className="card-file-count" title={q('downloaded') + ': ' + p.downloaded}>
                    <span aria-hidden="true">●</span> {p.downloaded}
                  </span>
                )}
              </span>
            </div>
          </>
        )}
      </div>
      {list && columns.includes('category') && (
        <span className="list-extra" title={categoryLabel(p.category)}>
          {categoryLabel(p.category)}
        </span>
      )}
      {list && columns.includes('files') && (
        <span className="list-extra file-count">{fileCountLabel(p)}</span>
      )}
      {list && columns.includes('acquired') && (
        <span className="list-extra">{p.acquired || q('unknown')}</span>
      )}
      {list && columns.includes('price') && (
        <span className="list-extra">{currentPriceLabel(locale, p) || '—'}</span>
      )}
      <input
        type="checkbox"
        aria-label={q('select') + ' ' + p.title}
        checked={selectedSet.has(p.id)}
        onClick={(e) => e.stopPropagation()}
        onChange={() => select(p, { ctrlKey: true, metaKey: false, shiftKey: false })}
      />
    </div>
  );
  const footer = (
    <footer className="result-footer">
      {visibleSelected.length > 0 && (
        <div
          className="selection-actions"
          onKeyUpCapture={(e) => {
            const row = e.currentTarget,
              b = (e.target as HTMLElement).getBoundingClientRect(),
              r = row.getBoundingClientRect();
            if (b.right > r.right) row.scrollLeft += b.right - r.right + 2;
            else if (b.left < r.left) row.scrollLeft -= r.left - b.left + 2;
          }}
          onFocusCapture={(e) => {
            const row = e.currentTarget,
              target = e.target as HTMLElement;
            requestAnimationFrame(() => {
              const b = target.getBoundingClientRect(),
                r = row.getBoundingClientRect();
              if (b.right > r.right) row.scrollLeft += b.right - r.right + 2;
              else if (b.left < r.left) row.scrollLeft -= r.left - b.left + 2;
            });
          }}
        >
          <div className="selection-group">
            <Button title={q('clearSelection')} onClick={() => setSelected([])}>
              <X size={13} />
            </Button>
            <strong className="selection-count">
              <b key={visibleSelected.length}>{visibleSelected.length.toLocaleString(locale)}</b>
              <span>{q('selected')}</span>
            </strong>
            {batchEnabled.selectAll && (
              <Button
                title={q('selectAll')}
                onClick={() => setSelected(matchedIds)}
                disabled={
                  !matchedIds.length || queryBusy || visibleSelected.length >= matchedIds.length
                }
              >
                <CheckCircle2 size={14} />
                <span>{q('selectAll')}</span>
              </Button>
            )}
          </div>
          {batchActions(visibleSelected)}
        </div>
      )}
      <div className="result-controls">
        <label className="thumbnail-control" title={q('thumbnail')}>
          <List size={14} />
          <input
            aria-label={q('thumbnail')}
            type="range"
            min={0}
            max={thumbnailSizes.length - 1}
            step={1}
            value={thumbnailSizes.indexOf(thumb as (typeof thumbnailSizes)[number])}
            style={
              {
                '--fill': `${(Math.max(0, thumbnailSizes.indexOf(thumb as (typeof thumbnailSizes)[number])) / (thumbnailSizes.length - 1)) * 100}%`,
              } as CSSProperties
            }
            aria-valuetext={thumb === 0 ? q('listView') : `${q('thumbnail')} ${thumb}px`}
            onChange={(e) => setThumb(thumbnailSizes[+e.target.value])}
          />
          <Grid2X2 size={14} />
        </label>
        <select
          aria-label={q('density')}
          title={q(density === 40 ? 'compact' : density === 74 ? 'standard' : 'comfortable')}
          value={density}
          onChange={(e) => setDensity(+e.target.value)}
        >
          <option value={40}>{q('compact')}</option>
          <option value={74}>{q('standard')}</option>
          <option value={90}>{q('comfortable')}</option>
        </select>
        <details className="column-menu">
          <summary aria-label={q('columns')} title={q('columns')}>
            <Columns2 size={14} />
            <span>{q('columns')}</span>
          </summary>
          <div className="popup-menu">
            {['shop', 'category', 'files', 'acquired', 'price'].map((column) => (
              <label className="check-row" key={column}>
                <input
                  type="checkbox"
                  checked={columns.includes(column)}
                  onChange={(e) =>
                    setColumns(
                      e.target.checked
                        ? [...columns, column]
                        : columns.filter((key) => key !== column),
                    )
                  }
                />
                {q(column)}
              </label>
            ))}
          </div>
        </details>
        {queryResult?.key === queryKey && queryResult.allDownloaded !== true && (
          <Button
            primary
            title={q('downloadAll')}
            disabled={!matchedIds.length || queryBusy}
            onClick={() => setProductConfirm({ kind: 'downloadAll', ids: matchedIds, names: [] })}
          >
            <Download size={14} />
            <span>{q('downloadAll')}</span>
          </Button>
        )}
      </div>
    </footer>
  );
  const localeOptions = Object.entries(localeNames).map(([id, name]) => (
    <option key={id} value={id}>
      {name}
    </option>
  ));
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
    <div className="appearance-content">
      <label className="field">
        <span>{q('language')}</span>
        <select value={locale} onChange={(e) => setLocale(e.target.value as Locale)}>
          {localeOptions}
        </select>
      </label>
      <label className="field">
        <span>{q('uiScale')}</span>
        <ScaleControl label={q('uiScale')} value={uiScale} onCommit={changeScale} />
      </label>
      <div className="appearance-toggles">
        <Toggle label={q('roundedCorners')} checked={roundedCorners} onChange={setRoundedCorners} />
        <Toggle label={q('reduceMotion')} checked={reduceMotion} onChange={setReduceMotion} />
        <Toggle label={q('oneHandedDrag')} checked={oneHandedDrag} onChange={setOneHandedDrag} />
        <Toggle label={q('bundledFonts')} checked={bundledFonts} onChange={setBundledFonts} />
      </div>
      <label className="field">
        <span>{q('themePreset')}</span>
      </label>
      <ThemePicker locale={locale} options={themeOptions} value={theme} onChange={setTheme} />
      <div className="theme-actions">
        <Button disabled={themeDeleteBusy} onClick={() => setDialog('theme')}>
          {q('editTheme')}
        </Button>
        <Button
          className="danger"
          disabled={themeDeleteBusy || !presets.some((p) => p.id === theme)}
          onClick={() => setThemeDeleteTarget({ id: selectedTheme.id, name: selectedThemeName })}
        >
          {resetText(locale, 'deleteTheme')}
        </Button>
        <div className="theme-transfer-actions">
          <Button
            disabled={themeDeleteBusy}
            onClick={() => {
              setThemePreview(null);
              setDialog('theme');
            }}
          >
            {q('importTheme')}
          </Button>
          <Button
            disabled={themeDeleteBusy}
            onClick={() => run(exportThemeConfig(selectedThemeTokens, selectedThemeName))}
          >
            {q('exportTheme')}
          </Button>
        </div>
      </div>
    </div>
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
    <ActivityHistory
      locale={locale}
      jobs={nativeState.jobs}
      accounts={accounts}
      products={products}
      aiEnabled={aiEnabled}
      onCancelDownloads={() => cancelDownloads()}
      onClear={async () => {
        await onNative('catalogue', 'clear_activity');
        await refreshState();
      }}
      onUnityResults={(job) => setFlow('unity-results:' + job.id)}
      onUnityRetry={(job) => {
        void retryUnityInventory(job.id)
          .then((seed) => openUnity(seed.productIds, seed.leafRefs))
          .catch((error) => notify(q(errorCode(error))));
      }}
      onCancel={(job) =>
        run(
          job.type === 'licenses:sync'
            ? invokeAction('licenses', 'cancel', { jobId: job.id })
            : job.type === 'unity:import'
              ? cancelUnity(job.id)
              : job.type.startsWith('ai:')
                ? cancelAi(job.id)
                : onNative(
                    isStoreJob(job.type) ? 'booth' : 'storage',
                    isStoreJob(job.type) ? 'cancel_sync' : 'cancel_job',
                    { jobId: job.id },
                  ),
        )
      }
      onRetry={(job) =>
        run(
          job.type === 'licenses:sync'
            ? invokeAction('licenses', 'retry', { jobId: job.id })
            : job.type.startsWith('ai:')
              ? retryAi(job.id)
              : onNative(
                  isStoreJob(job.type) ? 'booth' : 'storage',
                  isStoreJob(job.type) ? 'retry_sync' : 'retry_job',
                  { jobId: job.id },
                ),
        )
      }
      onSignIn={(job) => {
        if (job.accountId) run(onNative('booth', 'account_login', { accountId: job.accountId }));
      }}
      onOpenStorage={() => openSettings('storage')}
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
