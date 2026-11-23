import type { ReactNode } from 'react';
import type { Product, Locale, Account } from './model';
import { fileCountLabel } from './model';
import './inspector.css';
import { useInspectorPanes } from './features/inspector/useInspectorPanes';
import { useTagFold } from './features/inspector/useTagFold';
import { useInspectorFacts } from './features/inspector/useInspectorFacts';
import InspectorDetails from './features/inspector/InspectorDetails';
import InspectorFiles from './features/inspector/InspectorFiles';

interface AssetInspectorProps {
  product: Product;
  categoryLabel?: string;
  locale: Locale;
  wide: boolean;
  tab: 'details' | 'files';
  onTabChange: (tab: 'details' | 'files') => void;
  sensitiveMode?: string;
  revealed?: boolean;
  onReveal: () => void;
  accountIds?: string[] | null;
  accounts?: Account[];
  actions: ReactNode;
  onDismissAttention?: () => void;
  offerEndedFileIds?: readonly string[];
  onEdit: () => void;
  onTagClick?: (tag: string) => void;
  onFileRetry?: (fileId: string, redownload: boolean) => void;
  retryDisabled?: boolean;
  busyFileIds?: readonly string[];
  onFileOpen?: (fileId: string) => void;
  onOpenFolder?: () => void;
  folderAvailable?: boolean;
  openableFileIds?: readonly string[];
  onOpenProductPage?: (url: string) => void;
  onDownloadAll?: () => void;
  /** Whole-product download work can be cancelled while it is active. */
  onCancelDownload?: () => void;
  cancelPending?: boolean;
  /** Files with their own single-file download job. */
  cancellableFileIds?: readonly string[];
  onFileCancel?: (fileId: string) => void;
  onDeleteDownloads?: () => void;
  onFileDelete?: (fileId: string) => void;
  deleteDisabled?: boolean;
  onLocalFilesAdd?: (folder: boolean) => void;
  onLocalFileRemove?: (fileId: string) => void;
  localFilesBusy?: boolean;
}
export default function AssetInspector({
  product: p,
  categoryLabel,
  locale,
  wide,
  tab,
  onTabChange,
  sensitiveMode = 'show',
  revealed = false,
  onReveal,
  accountIds = null,
  accounts = [],
  actions,
  onDismissAttention,
  offerEndedFileIds = [],
  onEdit,
  onTagClick,
  onFileRetry,
  retryDisabled = false,
  busyFileIds = [],
  onFileOpen,
  onOpenFolder,
  folderAvailable = false,
  openableFileIds = [],
  onOpenProductPage,
  onDownloadAll,
  onCancelDownload,
  cancelPending = false,
  cancellableFileIds = [],
  onFileCancel,
  onDeleteDownloads,
  onFileDelete,
  deleteDisabled = false,
  onLocalFilesAdd,
  onLocalFileRemove,
  localFilesBusy = false,
}: AssetInspectorProps) {
  const panes = useInspectorPanes({ p });
  const tagFold = useTagFold({ p });
  const facts = useInspectorFacts({
    p,
    locale,
    accountIds,
    accounts,
    sensitiveMode,
    revealed,
    onOpenProductPage,
  });
  const { L } = facts;
  const details = (
    <InspectorDetails
      facts={facts}
      panes={panes}
      tagFold={tagFold}
      p={p}
      categoryLabel={categoryLabel}
      actions={actions}
      onReveal={onReveal}
      onEdit={onEdit}
      onTagClick={onTagClick}
      onDismissAttention={onDismissAttention}
      locale={locale}
    />
  );
  const files = (
    <InspectorFiles
      panes={panes}
      facts={facts}
      p={p}
      locale={locale}
      onDownloadAll={onDownloadAll}
      onCancelDownload={onCancelDownload}
      cancelPending={cancelPending}
      onDeleteDownloads={onDeleteDownloads}
      deleteDisabled={deleteDisabled}
      onOpenFolder={onOpenFolder}
      folderAvailable={folderAvailable}
      onLocalFilesAdd={onLocalFilesAdd}
      localFilesBusy={localFilesBusy}
      onFileOpen={onFileOpen}
      openableFileIds={openableFileIds}
      offerEndedFileIds={offerEndedFileIds}
      onFileRetry={onFileRetry}
      retryDisabled={retryDisabled}
      busyFileIds={busyFileIds}
      onFileCancel={onFileCancel}
      cancellableFileIds={cancellableFileIds}
      onFileDelete={onFileDelete}
      onLocalFileRemove={onLocalFileRemove}
    />
  );
  return (
    <div className={'asset-inspector' + (wide ? ' asset-inspector-wide' : '')}>
      {!wide && (
        <div className="asset-inspector-tabs" role="tablist">
          <button
            type="button"
            role="tab"
            aria-selected={tab === 'details'}
            onClick={() => onTabChange('details')}
          >
            {L('details')}
          </button>
          <button
            type="button"
            role="tab"
            aria-selected={tab === 'files'}
            onClick={() => onTabChange('files')}
          >
            {L('files')}{' '}
            <span title={p.manifestComplete === true ? undefined : L('totalUnverified')}>
              {fileCountLabel(p, false)}
            </span>
          </button>
        </div>
      )}
      <div className="asset-inspector-content">
        {(wide || tab === 'details') && details}
        {(wide || tab === 'files') && files}
      </div>
    </div>
  );
}
