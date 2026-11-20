import { useState, useEffect, useRef, useMemo, useCallback } from 'react';
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
  categoryName,
  statusKeys,
  isProductDownloaded,
  getProductDisplayStatus,
  defaultSortDirection,
  thumbnailSizes,
  isLightTheme,
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
import { resumesAfterRestart, updateJobBlocker, updateText } from './update-i18n';
import { resetText } from './settings-reset-i18n';
import './settings-reset.css';
import AiSuggestions from './AiSuggestions';
import FeaturesSettings from './FeaturesSettings';
import SpendingDialog from './SpendingDialog';
import { LibraryContextMenu, ResponsiveLibraryToolbar } from './LibraryMenus';
import type { ContextAction } from './LibraryMenus';
import ActivityHistory from './ActivityHistory';
import { aiSuggestionsText } from './ai-suggestions-i18n';
import ThemePicker, { defaultThemeOptions, themeDisplayName } from './ThemePicker';
import Workflows, { WorkflowContent, LibraryAccess, isStoreJob } from './Workflows';
import { isTauri } from '@tauri-apps/api/core';
import { getCurrentWebview } from '@tauri-apps/api/webview';
import EditDialogs, { exportThemeConfig } from './EditDialogs';
import { LicenseFilterDialog, licenseSummary } from './Licenses';
import { licenseText } from './license-i18n';
import {
  invokeAction,
  dialog as nativeDialog,
  errorCode,
  nativeFileAction,
  nativeZoom,
  openExternal,
  fetchProductMetadata,
  updateLocalFiles,
  cancelAi,
  retryAi,
  cancelUnity,
  retryUnityInventory,
  type LocalFileChanges,
} from './backend';
import type { NativeJob, ProductOverlayChange } from './backend';
import { nativeWorkflowText } from './native-workflow-i18n';
import Button from './shared/Button';
import SmallDialog from './shared/SmallDialog';
import { appText } from './shared/app-text';
import ProductStatus from './features/results/ProductStatus';
import Results from './features/results/Results';
import { configuredLibraryRoot } from './features/library/library-root';
import { acquisitionKinds, fresh, leavingHidden } from './features/filters/query';
import { neutralTokens } from './features/appearance/themes';
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
export default function App() {
  const uiPrefs = useUiPreferences();
  const {
    locale,
    setLocale,
    theme,
    setTheme,
    presets,
    setPresets,
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
    avatarSettings,
    setAvatarSettings,
    uiScale,
    setUiScale,
    zoomTarget,
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
    settingsReturn,
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
    libraryEpoch,
    blockedLibraryRequest,
    initialLoad,
    saved,
    customCategories,
    setCustomCategories,
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
    setProducts,
    catalogueTotal,
    scrollState,
    queryResult,
    setQueryResult,
    queryBusy,
    summaryCache,
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
    setProductDetails,
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
  const {
    reload,
    openSelectedLibrary,
    chooseLibrary,
    refreshState,
    refreshAvatarOptions,
    onNative,
  } = native;
  const preferenceSync = usePreferenceSync({ library, notices, uiPrefs, q });
  const { displayedPreferences, pendingPreferences, onPreferences, drainPreferenceWrites } =
    preferenceSync;
  const localFileLocks = useRef(new Set<string>()),
    [localFilePending, setLocalFilePending] = useState(new Set<string>());
  const [themePreview, setThemePreview] = useState<Record<string, string> | null>(null);
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
  const [deleteLocalSelection, setDeleteLocalSelection] = useState<{
    ids: string[];
    names: string[];
  } | null>(null);
  const [productConfirm, setProductConfirm] = useState<{
      kind: 'downloadAll' | 'deleteDownloads';
      ids: string[];
      names: string[];
    } | null>(null),
    [deletingDownloads, setDeletingDownloads] = useState<ReadonlySet<string>>(new Set());
  const watchedDownloads = useRef(new Set<string>()),
    staleProducts = useRef(new Set<string>());
  const [undo, setUndo] = useState<string | null>(null);
  const avatarSettingsRef = useRef(avatarSettings);
  avatarSettingsRef.current = avatarSettings;
  const latestLocalImport = useRef<{
      jobId: string;
      plannedIds: string[];
      category?: string;
      handled?: boolean;
    } | null>(null),
    importNavigation = useRef({ query, collection, view });
  importNavigation.current = { query, collection, view };
  useEffect(() => {
    if (!isTauri()) return;
    let disposed = false;
    let unlisten: (() => void) | undefined;
    void getCurrentWebview()
      .onDragDropEvent((event) => {
        if (event.payload.type !== 'drop' || !event.payload.paths.length) return;
        const point = event.payload.position;
        const target = document.elementFromPoint(
          point.x / devicePixelRatio,
          point.y / devicePixelRatio,
        );
        if (!target?.closest('.result-workspace,.wf-import')) return;
        const paths = event.payload.paths;
        setImportSeed({
          name: paths[0]
            .replace(/[\\/]+$/, '')
            .split(/[\\/]/)
            .pop()
            ?.replace(/\.[^.]+$/, ''),
          sources: paths,
          sourceKind: 'local',
        });
        setFlow('import');
      })
      .then((stop) => {
        if (disposed) stop();
        else unlisten = stop;
      })
      .catch(() => {});
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, []);
  const zoomQueue = useRef(Promise.resolve());
  const searchRef = useRef<HTMLInputElement>(null);
  const jobs = useMemo(
    () =>
      new Map(
        nativeState.jobs
          .filter(
            (j) =>
              ['download', 'storage:import_local'].includes(j.type) &&
              ['queued', 'running'].includes(j.state),
          )
          .flatMap((j) =>
            [
              ...new Set([j.productId, ...(j.productIds ?? [])].filter((id): id is string => !!id)),
            ].map((id) => [id, j] as const),
          ),
      ),
    [nativeState.jobs],
  );
  async function changeLocalFiles(
    productId: string,
    changes: LocalFileChanges | (() => Promise<LocalFileChanges | undefined>),
  ) {
    if (localFileLocks.current.has(productId) || jobs.has(productId))
      throw new Error('storage_busy');
    localFileLocks.current.add(productId);
    setLocalFilePending(new Set(localFileLocks.current));
    try {
      const args = typeof changes === 'function' ? await changes() : changes;
      if (!args) return;
      const result = await updateLocalFiles(productId, args);
      await refreshState();
      return result;
    } finally {
      localFileLocks.current.delete(productId);
      setLocalFilePending(new Set(localFileLocks.current));
    }
  }
  const addLocalFiles = (productId: string, folder: boolean) =>
    run(
      changeLocalFiles(productId, async () => {
        const result = await nativeDialog(folder ? 'folder' : 'open_file', { multiple: true });
        const sources = result?.paths ?? (result?.path ? [result.path] : []);
        return sources.length
          ? { draftId: crypto.randomUUID(), addSources: sources, removeFileIds: [] }
          : undefined;
      }),
    );
  const changeAvatarDetails = async (
    id: string,
    patch: { displayName: string; shopName: string },
  ) => {
    const epoch = libraryEpoch.current;
    await drainPreferenceWrites();
    if (epoch !== libraryEpoch.current) throw new Error('library_not_open');
    const next = {
      ...avatarSettingsRef.current,
      [id]: { ...avatarSettingsRef.current[id], ...patch },
    };
    await onPreferences({ avatarSettings: next });
    if (epoch !== libraryEpoch.current) return;
    avatarSettingsRef.current = next;
    if (displayedPreferences.current) displayedPreferences.current.avatarSettings = next;
    setAvatarSettings(next);
    await refreshAvatarOptions().catch((error) => notify(q(errorCode(error))));
  };
  const categoryLabel = (key: string) => categoryName(key, customCategories, q),
    categoryList = [...customCategories].sort((a, b) =>
      a.name.localeCompare(b.name, locale, { numeric: true }),
    );
  const changeCategories = async (args: Record<string, unknown>) => {
    const result = await onNative('catalogue', 'custom_category', args);
    if (Array.isArray(result?.customCategories)) setCustomCategories(result.customCategories);
    if (args.op === 'delete')
      setQuery((old) => (old.category === args.id ? { ...old, category: 'all' } : old));
  };
  const avatarOptions = (nativeState.avatarOptions ?? []).map((option) => {
      const settings = avatarSettings[option.id];
      return {
        ...option,
        displayName:
          settings?.displayName === undefined
            ? option.displayName || option.sourceDisplayName
            : settings.displayName.trim() || option.sourceDisplayName || option.displayName,
        shopName:
          settings?.shopName === undefined
            ? option.shopName || option.sourceShopName
            : settings.shopName.trim() || option.sourceShopName || option.shopName,
      };
    }),
    avatarLabel = (id: string) => {
      const option =
        avatarOptions.find((option) => option.id === id) ??
        avatarOptions.find((option) => option.aliases.includes(id));
      return (
        option?.displayName ||
        [option?.officialName, option?.englishName].filter(Boolean).join(' / ') ||
        id
      );
    };
  const avatarEnabled = (id: string) => {
    const option = avatarOptions.find((option) => option.id === id),
      explicit = avatarSettings[id]?.enabled ?? option?.explicitEnabled;
    if (typeof explicit === 'boolean') return explicit;
    const automatic = nativeState.preferences.avatarAutomaticEnabled;
    if (automatic && typeof automatic === 'object' && !Array.isArray(automatic))
      return Reflect.get(automatic, id) === true;
    return option?.enabled ?? false;
  };
  const changeAvatars = (ids: string[], enabled: boolean) => {
    setAvatarSettings((old) => ({
      ...old,
      ...Object.fromEntries(ids.map((id) => [id, { ...old[id], enabled }])),
    }));
    if (!enabled && ids.includes(query.avatar)) updateQuery({ avatar: 'all' });
  };
  const avatarChoices = avatarOptions
    .filter((option) => avatarEnabled(option.id))
    .map((option) => option.id);
  const selectedTheme =
      presets.find((p) => p.id === theme) ||
      defaultThemeOptions.find((p) => p.id === theme) ||
      defaultThemeOptions[0],
    selectedThemeTokens = { ...neutralTokens, ...selectedTheme.tokens },
    selectedThemeName = ['dark', 'light'].includes(selectedTheme.id)
      ? q(selectedTheme.id)
      : themeDisplayName(locale, selectedTheme),
    themeTokens = themePreview || selectedThemeTokens;
  const batchPatch = async (changes: ProductOverlayChange[]) => {
    const prepared = [];
    for (const change of changes) {
      const patch = { ...change.patch },
        clearFields: string[] = [];
      if (patch.thumbnailSourcePath) {
        const copied = await nativeFileAction<{ relativePath: string }>('copy_image', {
          path: patch.thumbnailSourcePath,
          productId: change.id,
        });
        patch.thumbnailPath = copied.relativePath;
        clearFields.push('thumbnailHidden');
      } else if (patch.thumbnailSourcePath === null) {
        clearFields.push('thumbnailPath', 'thumbnailHidden');
        delete patch.thumbnailOverrideUrl;
        delete patch.previewUrl;
      }
      delete patch.thumbnailSourcePath;
      if (patch.titleOverride === false) {
        clearFields.push('title');
        delete patch.title;
        delete patch.titleOverride;
      }
      if (patch.thumbnailOverrideUrl === '' && patch.previewUrl === '')
        patch.thumbnailHidden = true;
      if (patch.sourceUrlOverride === false) {
        clearFields.push('sourceUrl');
        delete patch.sourceUrl;
      }
      delete patch.sourceUrlOverride;
      const editable = [
        'title',
        'category',
        'tags',
        'note',
        'ownedAvatars',
        'sensitive',
        'rejectedTags',
        'thumbnailPath',
        'thumbnailHidden',
        'sourceUrl',
        'sourceThumbnailUrl',
      ];
      const nativePatch = Object.fromEntries(
        Object.entries(patch).filter(([key]) => editable.includes(key)),
      );
      prepared.push({
        id: change.id,
        patch: nativePatch,
        clearFields,
        ...(change.ownedAvatarsDelta ? { ownedAvatarsDelta: change.ownedAvatarsDelta } : {}),
      });
    }
    const result = await onNative('catalogue', 'overlay', { changes: prepared });
    if (result?.changed !== 0) setUndo(result?.undoToken ?? result?.token ?? null);
    await refreshAvatarOptions().catch((error) => notify(q(errorCode(error))));
  };
  const userPatch = async (ids: string[], value: Partial<Product>) =>
    batchPatch(ids.map((id) => ({ id, patch: value })));
  const localSelection = (ids: string[]) =>
    ids.length > 0 &&
    ids.every((id) => {
      const product = productMap.get(id);
      return product
        ? product.provider === 'local'
        : query.includeOffline && query.accountIds?.length === 0;
    });
  const requestLocalDelete = (ids: string[]) => {
    if (localSelection(ids))
      setDeleteLocalSelection({
        ids: [...new Set(ids)],
        names: ids.map((id) => productMap.get(id)?.title ?? '').filter(Boolean),
      });
  };
  const onlineSelection = (ids: string[]) =>
    ids.length > 0 &&
    ids.every((id) => {
      const product = productMap.get(id);
      return product
        ? !!product.provider && product.provider !== 'local'
        : query.quick === 'hidden' || !query.includeOffline;
    });
  const hiddenSelection = (ids: string[]) =>
    query.quick === 'hidden' || ids.every((id) => productMap.get(id)?.hidden === true);
  const hideProducts = async (ids: string[], hidden: boolean) => {
    if (!onlineSelection(ids)) return;
    const result = await onNative('catalogue', 'overlay', {
      changes: [...new Set(ids)].map((id) => ({ id, patch: { hidden }, clearFields: [] })),
    });
    if (result?.changed !== 0) setUndo(result?.undoToken ?? result?.token ?? null);
    setQueryResult((result) =>
      result
        ? {
            ...result,
            products: result.products.filter((product) => !ids.includes(product.id)),
            matchedIds: result.matchedIds.filter((id) => !ids.includes(id)),
          }
        : null,
    );
    setProducts((rows) =>
      rows.map((product) => (ids.includes(product.id) ? { ...product, hidden } : product)),
    );
    setSelected((rows) => rows.filter((id) => !ids.includes(id)));
    setFocusId((id) => (ids.includes(id) ? '' : id));
    setProductDetails(null);
    await reload();
  };
  const deleteLocalProducts = async (ids: string[]) => {
    try {
      const result = (await onNative('storage', 'delete_local_products', { productIds: ids })) as {
        deletedProductIds: string[];
      };
      const removed = new Set(result.deletedProductIds);
      for (const id of removed) summaryCache.current.delete(id);
      setQueryResult((result) =>
        result
          ? {
              ...result,
              products: result.products.filter((product) => !removed.has(product.id)),
              matchedIds: result.matchedIds.filter((id) => !removed.has(id)),
              total: Math.max(0, result.total - removed.size),
            }
          : null,
      );
      setProducts((rows) => rows.filter((product) => !removed.has(product.id)));
      setSelected((rows) => rows.filter((id) => !removed.has(id)));
      setFocusId((id) => (removed.has(id) ? '' : id));
      setProductDetails(null);
      setUndo(null);
    } finally {
      await reload();
    }
  };
  const undoAction = async () => {
    if (!undo) return;
    await onNative('catalogue', 'undo', { token: undo });
    setUndo(null);
  };
  const activeDownloads = (productId: string) =>
    nativeState.jobs.filter(
      (job) =>
        job.type === 'download' &&
        job.productId === productId &&
        ['queued', 'running'].includes(job.state),
    );
  const cancelDownloads = (ids?: string[]) =>
    run(onNative('storage', 'cancel_downloads', ids ? { productIds: ids } : {}));
  /** The store lists no file for these products and none is on disk. */
  const allUnavailable = (ids: string[]) =>
    ids.length > 0 && ids.every((id) => productMap.get(id)?.unavailable === true);
  const hasDownloadedFiles = (ids: string[]) =>
    ids.some((id) => {
      const product = productMap.get(id);
      return (
        !product || product.downloaded > (product.keptFiles ?? 0) || product.status === 'attention'
      );
    });
  const requestDeleteDownloads = (ids: string[]) => {
    const unique = [...new Set(ids)];
    if (onlineSelection(unique))
      setProductConfirm({
        kind: 'deleteDownloads',
        ids: unique,
        names: unique.map((id) => productMap.get(id)?.title ?? '').filter(Boolean),
      });
  };
  const deleteDownloads = async (args: Record<string, unknown>, ids: string[]) => {
    setDeletingDownloads((old) => new Set([...old, ...ids]));
    try {
      await invokeAction('storage', 'delete_downloads', args);
    } finally {
      setDeletingDownloads((old) => new Set([...old].filter((id) => !ids.includes(id))));
    }
  };
  /** Downloads started here update their product when the store no longer offers a file. */
  const watchDownloads = (result: unknown) => {
    for (const id of (result as { jobIds?: unknown[] } | undefined)?.jobIds ?? [])
      if (typeof id === 'string') watchedDownloads.current.add(id);
  };
  // One update per store covers every such product once those downloads settle, because a
  // store update slows that store's transfers to one. An asset downloaded whole then gets
  // its updated list through the download request it already holds.
  useEffect(() => {
    let active = false;
    for (const job of nativeState.jobs as (NativeJob & {
      failures?: Record<string, { errorCode?: string } | undefined>;
    })[]) {
      if (!watchedDownloads.current.has(job.id)) continue;
      // A download waiting for its account's sign-in holds up no other update.
      if (['queued', 'running'].includes(job.state)) {
        if (job.errorCode !== 'auth_required') active = true;
        continue;
      }
      watchedDownloads.current.delete(job.id);
      if (
        job.productId &&
        (job.errorCode === 'store_file_unavailable' ||
          Object.values(job.failures ?? {}).some(
            (item) => item?.errorCode === 'store_file_unavailable',
          ))
      )
        staleProducts.current.add(job.productId);
    }
    if (active || !staleProducts.current.size) return;
    // Store IDs carry their store (BOOTH's are bare numbers), so a busy or signed-out
    // account of one store cannot fail another store's update.
    const stores = new Map<string, string[]>();
    for (const id of staleProducts.current) {
      const store = id.includes(':') ? id.slice(0, id.indexOf(':')) : 'booth';
      stores.set(store, [...(stores.get(store) ?? []), id]);
    }
    staleProducts.current.clear();
    for (const productIds of stores.values())
      run(onNative('booth', 'update_products', { productIds }));
  }, [nativeState.jobs]);
  const download = (ids: string[]) => {
    if (ids.length)
      run(onNative('storage', 'enqueue_products', { productIds: ids }).then(watchDownloads));
  };
  const downloadFile = (fileId: string, redownload: boolean) =>
    run(onNative('storage', 'enqueue_file', { fileId, redownload }).then(watchDownloads));
  const update = (ids: string[]) => {
    if (ids.length) run(onNative('booth', 'update_products', { productIds: ids }));
  };
  const syncLicenses = (ids: string[]) => {
    if (ids.length)
      run(
        invokeAction<{ jobId?: string | null }>('licenses', 'sync', { productIds: ids }).then(
          (result) => {
            if (!result?.jobId) notify(licenseText(locale, 'noLicenseWork')!);
          },
        ),
      );
  };
  const needsAttention = (ids: string[]) =>
    ids.some((id) => productMap.get(id)?.status === 'attention');
  const dismissAttention = (ids: string[]) => {
    const targets = ids.filter((id) => productMap.get(id)?.status === 'attention');
    if (targets.length) run(onNative('catalogue', 'dismiss_attention', { productIds: targets }));
  };
  /** Files the latest finished download of a product found the store no longer offers. */
  const refusedFiles = (productId: string) => {
    const job = nativeState.jobs
      .filter(
        (job) =>
          job.type === 'download' &&
          job.productId === productId &&
          !['queued', 'running'].includes(job.state),
      )
      .sort((a, b) => (b.updatedAt ?? '').localeCompare(a.updatedAt ?? ''))[0] as
      | (NativeJob & { failures?: Record<string, { errorCode?: string } | undefined> })
      | undefined;
    return Object.entries(job?.failures ?? {})
      .filter(([, failure]) =>
        ['store_file_unavailable', 'free_offer_unavailable'].includes(failure?.errorCode ?? ''),
      )
      .map(([id]) => id);
  };
  const exportZip = async (ids: string[]) => {
    if (!ids.length) return;
    try {
      const destination = await nativeDialog('save_file', {
        defaultName: 'NekoNekoAsset.zip',
        filters: [{ name: 'ZIP', extensions: ['zip'] }],
      });
      if (destination)
        await onNative('storage', 'export_zip', { productIds: ids, path: destination.path });
    } catch (error) {
      notify(q(errorCode(error)));
    }
  };
  const edit = () => setDialog(visibleSelected.length > 1 ? 'batch' : 'edit');
  const choosePreset = async (tokens: Record<string, string>, name = 'Custom') => {
    let unique = name.trim() || 'Custom',
      suffix = 2;
    while (presets.some((p) => p.name.toLocaleLowerCase() === unique.toLocaleLowerCase()))
      unique = `${name} (${suffix++})`;
    const preset = { id: 'theme-' + Date.now(), name: unique, tokens };
    const next = [...presets, preset];
    await onPreferences({ presets: next, theme: preset.id });
    setPresets(next);
    setTheme(preset.id);
    setThemePreview(null);
  };
  const themeDeleting = useRef(false),
    [themeDeleteBusy, setThemeDeleteBusy] = useState(false);
  const [themeDeleteTarget, setThemeDeleteTarget] = useState<{ id: string; name: string } | null>(
    null,
  );
  const deleteTheme = async (id: string) => {
    if (themeDeleting.current) return;
    if (!presets.some((p) => p.id === id)) throw new Error('entryUnavailable');
    themeDeleting.current = true;
    setThemeDeleteBusy(true);
    const next = presets.filter((p) => p.id !== id);
    try {
      await onPreferences({ presets: next, ...(theme === id ? { theme: 'dark' } : {}) });
      setPresets(next);
      setTheme((current) => (current === id ? 'dark' : current));
      setThemePreview(null);
    } finally {
      themeDeleting.current = false;
      setThemeDeleteBusy(false);
    }
  };
  const openSettings = (route: string) => {
    if (view !== 'settings')
      settingsReturn.current = {
        inspector,
        tab,
        view: view === 'activity' || view === 'ai-suggestions' ? view : 'library',
      };
    setSettingsRoute(route === 'ai-settings' && !aiEnabled ? 'features' : route);
    setView('settings');
    setInspector(true);
  };
  const toggleSettings = () => {
    if (view === 'settings') {
      setView(
        settingsReturn.current.view === 'ai-suggestions' && !aiEnabled
          ? 'library'
          : settingsReturn.current.view,
      );
      setTab(settingsReturn.current.tab);
      setInspector(settingsReturn.current.inspector);
    } else openSettings(settingsRoute);
  };
  const changeScale = (scale: number) => {
    const bounded = Math.max(80, Math.min(300, Math.round(scale)));
    zoomTarget.current = bounded;
    zoomQueue.current = zoomQueue.current.then(async () => {
      if (bounded !== zoomTarget.current) return;
      try {
        const result = await nativeZoom(bounded);
        if (bounded === zoomTarget.current) {
          pendingPreferences.current.uiScale = result.scale;
          setUiScale(result.scale);
        }
      } catch (error) {
        if (bounded === zoomTarget.current) zoomTarget.current = uiScale;
        notify(q(errorCode(error)));
      }
    });
  };
  useEffect(() => {
    if (loaded && initialLoad.current) changeScale(zoomTarget.current);
  }, [loaded]);
  useEffect(() => {
    const accepted = latestLocalImport.current;
    if (!accepted || accepted.handled) return;
    const job = nativeState.jobs.find(
      (job) => job.id === accepted.jobId && job.type === 'storage:import_local',
    );
    if (job?.state !== 'completed') return;
    accepted.handled = true;
    const reusedId = job.productIds?.find((id) => !accepted.plannedIds.includes(id));
    if (!reusedId) return;
    notify(q('assetExists'));
    void invokeAction<Product>('catalogue', 'product_details', { id: reusedId })
      .then((product) => {
        const current = importNavigation.current;
        if (
          latestLocalImport.current !== accepted ||
          current.view !== 'library' ||
          current.collection ||
          JSON.stringify(current.query) !==
            JSON.stringify({ ...fresh, category: accepted.category ?? 'all' })
        )
          return;
        if (categoryKeys.some((key) => key === product.category))
          updateQuery({ category: product.category });
      })
      .catch((error) => notify(q(errorCode(error))));
  }, [nativeState.jobs, locale, flow]);
  useEffect(() => {
    document.documentElement.lang = locale;
  }, [locale]);
  // Decode the UI-language face and the Japanese face for product titles while the library loads, so CJK text is not blank on first paint (fonts.css uses font-display:block).
  useEffect(() => {
    if (!loaded || !bundledFonts) return;
    const ui =
      locale === 'zh-Hant'
        ? 'Noto Sans TC'
        : locale === 'zh-Hans'
          ? 'Noto Sans SC'
          : locale === 'ko'
            ? 'Noto Sans KR'
            : 'Noto Sans JP';
    for (const family of new Set([ui, 'Noto Sans JP']))
      void document.fonts.load(`13px "${family}"`, 'あ一').catch(() => {});
  }, [loaded, bundledFonts, locale]);
  useEffect(() => {
    const handler = (e: globalThis.KeyboardEvent) => {
      if (updateOpen || themeDeleteTarget) return;
      if (!['ready', 'setup'].includes(libraryStateRef.current?.state ?? '') || libraryOpening)
        return;
      if (e.isComposing || e.keyCode === 229) return;
      const editing =
        e.target instanceof Element &&
        !!e.target.closest('input,textarea,select,[contenteditable="true"]');
      if ((e.ctrlKey || e.metaKey) && ['+', '=', '-', 'Add', 'Subtract'].includes(e.key)) {
        e.preventDefault();
        changeScale(zoomTarget.current + (['-', 'Subtract'].includes(e.key) ? -10 : 10));
        return;
      }
      if (libraryStateRef.current?.state !== 'ready') return;
      if (deleteLocalSelection) return;
      if (
        (e.ctrlKey || e.metaKey) &&
        e.key.toLowerCase() === 'a' &&
        !editing &&
        !flow &&
        !dialog &&
        !context &&
        ['library', 'settings'].includes(view)
      ) {
        e.preventDefault();
        setSelected(matchedIds);
        return;
      }
      if ((e.ctrlKey || e.metaKey) && e.key === 'f' && !flow && !dialog) {
        e.preventDefault();
        searchRef.current?.focus();
      }
      if ((e.ctrlKey || e.metaKey) && e.key === 'z' && !flow && !dialog && !editing) {
        e.preventDefault();
        run(undoAction());
      }
      if (e.key === 'Escape' && !flow && !dialog) {
        if (context) setContext(null);
        else clearSelection();
      }
    };
    window.addEventListener('keydown', handler);
    return () => window.removeEventListener('keydown', handler);
  }, [
    flow,
    dialog,
    deleteLocalSelection,
    undo,
    uiScale,
    context,
    matchedIds,
    view,
    libraryOpening,
    updateOpen,
    themeDeleteTarget,
  ]);
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
  const appearance = (
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
                appearance
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
  const appTheme = themeTokens['--bg']
    ? isLightTheme(themeTokens)
      ? 'light'
      : 'dark'
    : theme === 'light'
      ? 'light'
      : 'dark';
  const appClass =
    'app corrected-shell variant-a' +
    (roundedCorners ? '' : ' square-corners') +
    (reduceMotion ? ' reduce-motion' : '') +
    (bundledFonts ? '' : ' system-fonts');
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
