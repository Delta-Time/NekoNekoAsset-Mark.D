import { version as appVersion } from '../../../package.json';
import { fetchProductMetadata, invokeAction } from '../../backend';
import { resumesAfterRestart } from '../../update-i18n';
import UpdatePrompt from '../../UpdatePrompt';
import Workflows, { LibraryAccess } from '../../Workflows';
import EditDialogs from '../../EditDialogs';
import SpendingDialog from '../../SpendingDialog';
import ConfirmDelete from '../../ConfirmDelete';
import DeleteLocalProducts from '../../DeleteLocalProducts';
import ConfirmProducts from '../../ConfirmProducts';
import { LicenseFilterDialog } from '../../Licenses';
import AvatarManager from '../../AvatarManager';
import SmallDialog from '../../shared/SmallDialog';
import type { WorkflowProps } from '../workflows/useWorkflowProps';
import ProductContextMenu from '../products/ProductContextMenu';
import type { Navigation } from './useNavigation';
import type { ProductActions } from '../products/useProductActions';
import type { Library } from '../library/useLibrary';
import type { Selection } from '../selection/useSelection';
import type { Results } from '../results/useResults';
import type { UiPreferences } from '../preferences/useUiPreferences';
import type { ProductEdits } from '../products/useProductEdits';
import type { Notices } from './useNotice';
import type { Updates } from '../updates/useUpdates';
import type { SelectedProducts } from '../selection/useSelectedProducts';
import type { Appearance } from '../appearance/useAppearance';
import type { CollectionStore } from '../collections/useCollections';
import type { Avatars } from '../avatars/useAvatars';
import type { Native } from '../library/useNative';
import type { Filters } from '../filters/useFilters';
import type { Categories } from '../categories/useCategories';
import type { LocalFiles } from '../local-files/useLocalFiles';

/** Dialogs, flows and menus that open over the App. */
export default function AppDialogs({
  nav,
  actions,
  library,
  selection,
  results,
  uiPrefs,
  edits,
  notices,
  updates,
  selectedProducts,
  appearance,
  collectionStore,
  avatars,
  native,
  filters,
  categories,
  localFiles,
  q,
  workflowProps,
}: {
  nav: Navigation;
  actions: ProductActions;
  library: Library;
  selection: Selection;
  results: Results;
  uiPrefs: UiPreferences;
  edits: ProductEdits;
  notices: Notices;
  updates: Updates;
  selectedProducts: SelectedProducts;
  appearance: Appearance;
  collectionStore: CollectionStore;
  avatars: Avatars;
  native: Native;
  filters: Filters;
  categories: Categories;
  localFiles: LocalFiles;
  q: (key: string) => string;
  workflowProps: WorkflowProps;
}) {
  const { flow, dialog, setDialog, licenseOpen, setLicenseOpen, licenseButton } = nav;
  const { productConfirm, deleteDownloads, setProductConfirm } = actions;
  const {
    loaded,
    setupDone,
    libraryState,
    nativeState,
    aiEnabled,
    licenseEnabled,
    saved,
    libraryOpening,
  } = library;
  const { products } = results;
  const { locale } = uiPrefs;
  const {
    userPatch,
    batchPatch,
    deleteLocalSelection,
    deleteLocalProducts,
    setDeleteLocalSelection,
  } = edits;
  const { notify } = notices;
  const {
    updateBlocker,
    currentUpdateBlocker,
    updateOpen,
    updateRequest,
    setUpdateOpen,
    setUpdateAvailable,
    prepareUpdate,
  } = updates;
  const { chosen, active } = selectedProducts;
  const {
    setThemePreview,
    selectedThemeTokens,
    selectedThemeName,
    choosePreset,
    themeDeleteTarget,
    deleteTheme,
    setThemeDeleteTarget,
  } = appearance;
  const { collections, collectionRows, collectionBusy } = collectionStore;
  const { avatarOptions, avatarLabel, avatarEnabled, changeAvatars, changeAvatarDetails } = avatars;
  const { refreshAvatarOptions, onNative } = native;
  const {
    query,
    sort,
    direction,
    collection,
    setCollection,
    clearFilters,
    savedScope,
    setSavedScope,
    updateQuery,
  } = filters;
  const { categoryList, changeCategories } = categories;
  const { changeLocalFiles } = localFiles;
  return (
    <>
      <UpdatePrompt
        locale={locale}
        currentVersion={appVersion}
        ready={loaded && setupDone && libraryState?.state === 'ready'}
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
              await invokeAction('storage', 'enqueue_products', {
                productIds: productConfirm.ids,
              });
            else await deleteDownloads({ productIds: productConfirm.ids }, productConfirm.ids);
          }}
          onClose={() => setProductConfirm(null)}
        />
      )}
      <ProductContextMenu
        nav={nav}
        actions={actions}
        library={library}
        selection={selection}
        results={results}
        uiPrefs={uiPrefs}
        edits={edits}
        notices={notices}
        q={q}
      />
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
    </>
  );
}
