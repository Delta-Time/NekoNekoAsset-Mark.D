import FeaturesSettings from '../../FeaturesSettings';
import { WorkflowContent } from '../../Workflows';
import AssetInspector from '../../AssetInspector';
import { openExternal } from '../../backend';
import type { WorkflowProps } from '../workflows/useWorkflowProps';
import AppearanceSettings from '../appearance/AppearanceSettings';
import BatchSummary from './BatchSummary';
import SingleActions from '../products/SingleActions';
import type { UiPreferences } from '../preferences/useUiPreferences';
import type { Appearance } from '../appearance/useAppearance';
import type { Navigation } from '../shell/useNavigation';
import type { Notices } from '../shell/useNotice';
import type { SelectedProducts } from '../selection/useSelectedProducts';
import type { Library } from '../library/useLibrary';
import type { ProductEdits } from '../products/useProductEdits';
import type { ProductActions } from '../products/useProductActions';
import type { Results } from '../results/useResults';
import type { Filters } from '../filters/useFilters';
import type { Selection } from '../selection/useSelection';
import type { Layout } from '../shell/useLayout';
import type { PreferenceSync } from '../preferences/usePreferenceSync';
import type { Categories } from '../categories/useCategories';
import type { Native } from '../library/useNative';
import type { LocalFiles } from '../local-files/useLocalFiles';

/** The details pane: settings pages, several selected products, or the active product. */
export default function InspectorPane({
  uiPrefs,
  appearance,
  nav,
  notices,
  selectedProducts,
  library,
  edits,
  actions,
  results,
  filters,
  selection,
  layout,
  preferenceSync,
  categories,
  native,
  localFiles,
  q,
  workflowProps,
}: {
  uiPrefs: UiPreferences;
  appearance: Appearance;
  nav: Navigation;
  notices: Notices;
  selectedProducts: SelectedProducts;
  library: Library;
  edits: ProductEdits;
  actions: ProductActions;
  results: Results;
  filters: Filters;
  selection: Selection;
  layout: Layout;
  preferenceSync: PreferenceSync;
  categories: Categories;
  native: Native;
  localFiles: LocalFiles;
  q: (key: string) => string;
  workflowProps: WorkflowProps;
}) {
  const { setInspectorWidth, locale } = uiPrefs;
  const { view, settingsRoute, setSettingsRoute, tab, setTab } = nav;
  const { notify, run } = notices;
  const { visibleSelected, active, availability, availabilityKey } = selectedProducts;
  const { aiEnabled, nativeState, accounts } = library;
  const { edit } = edits;
  const {
    dismissAttention,
    refusedFiles,
    downloadFile,
    download,
    cancelDownloads,
    activeDownloads,
    requestDeleteDownloads,
    deleteDownloads,
    deletingDownloads,
  } = actions;
  const { query, updateQuery } = filters;
  const { revealed, setRevealed } = selection;
  const { resize, wide, paneWidth } = layout;
  const { onPreferences } = preferenceSync;
  const { categoryLabel } = categories;
  const { onNative } = native;
  const { addLocalFiles, changeLocalFiles, localFilePending, jobs } = localFiles;
  return (
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
        className={`inspector inspector-pane ${wide ? 'wide' : ''}`}
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
                <AppearanceSettings
                  uiPrefs={uiPrefs}
                  appearance={appearance}
                  nav={nav}
                  notices={notices}
                  q={q}
                />
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
            actions={
              <SingleActions
                p={active}
                actions={actions}
                nav={nav}
                edits={edits}
                library={library}
                selectedProducts={selectedProducts}
                notices={notices}
                q={q}
              />
            }
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
}
