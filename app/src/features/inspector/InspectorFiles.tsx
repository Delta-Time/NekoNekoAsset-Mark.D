import {
  Check,
  Download,
  FileArchive,
  FileBox,
  FolderOpen,
  KeyRound,
  Plus,
  RefreshCw,
  Trash2,
  X,
} from 'lucide-react';
import { useState } from 'react';
import type { Locale, Product } from '../../model';
import { byteLabel, fileCountLabel, isFileDownloaded, isProductDownloaded } from '../../model';
import { table } from '../../messages';
import { hasLocalCopy, provenanceLines } from './inspector-facts';
import type { InspectorPanes } from './useInspectorPanes';
import type { InspectorFacts } from './useInspectorFacts';

/** A password for a file's archive: Enter extracts with it, Escape closes. */
function ArchivePassword({
  fileName,
  L,
  onExtract,
  onClose,
}: {
  fileName: string;
  L: (key: string) => string;
  onExtract: (password: string) => void;
  onClose: () => void;
}) {
  const [password, setPassword] = useState('');
  const extract = () => {
    if (password) onExtract(password);
  };
  // Focus goes back to the row's key, which stays when the field closes unsent.
  const close = (from: HTMLElement) => {
    from
      .closest('.asset-inspector-file')
      ?.querySelector<HTMLButtonElement>('.asset-inspector-file-unlock')
      ?.focus();
    onClose();
  };
  return (
    <div className="asset-inspector-file-password">
      <input
        type="password"
        autoFocus
        autoComplete="off"
        spellCheck={false}
        maxLength={256}
        placeholder={L('enterArchivePassword')}
        aria-label={L('enterArchivePassword') + ' ' + fileName}
        value={password}
        onChange={(event) => setPassword(event.target.value)}
        onKeyDown={(event) => {
          if (event.nativeEvent.isComposing) return;
          if (event.key === 'Escape') {
            event.preventDefault();
            event.stopPropagation();
            close(event.currentTarget);
          } else if (event.key === 'Enter') {
            event.preventDefault();
            extract();
          }
        }}
      />
      <button
        type="button"
        title={L('extractArchive')}
        aria-label={L('extractArchive') + ' ' + fileName}
        disabled={!password}
        onClick={extract}
      >
        <Check size={13} />
      </button>
      <button
        type="button"
        title={L('cancel')}
        aria-label={L('cancel')}
        onClick={(event) => close(event.currentTarget)}
      >
        <X size={13} />
      </button>
    </div>
  );
}

/** The files pane: count, Download all, cancel, open folder and delete, then each file with its
 * status and its own open, retry, cancel and delete, and a password for an archive that asks. */
export default function InspectorFiles({
  panes,
  facts,
  p,
  locale,
  onDownloadAll,
  onCancelDownload,
  cancelPending,
  onDeleteDownloads,
  deleteDisabled,
  onOpenFolder,
  folderAvailable,
  onLocalFilesAdd,
  localFilesBusy,
  onFileOpen,
  openableFileIds,
  onFileRetry,
  retryDisabled,
  busyFileIds,
  onFileCancel,
  cancellableFileIds,
  onFileDelete,
  onLocalFileRemove,
  onArchivePassword,
}: {
  panes: InspectorPanes;
  facts: InspectorFacts;
  p: Product;
  locale: Locale;
  onDownloadAll: (() => void) | undefined;
  onCancelDownload: (() => void) | undefined;
  cancelPending: boolean;
  onDeleteDownloads: (() => void) | undefined;
  deleteDisabled: boolean;
  onOpenFolder: (() => void) | undefined;
  folderAvailable: boolean;
  onLocalFilesAdd: ((folder: boolean) => void) | undefined;
  localFilesBusy: boolean;
  onFileOpen: ((fileId: string) => void) | undefined;
  openableFileIds: readonly string[];
  onFileRetry: ((fileId: string, redownload: boolean) => void) | undefined;
  retryDisabled: boolean;
  busyFileIds: readonly string[];
  onFileCancel: ((fileId: string) => void) | undefined;
  cancellableFileIds: readonly string[];
  onFileDelete: ((fileId: string) => void) | undefined;
  onLocalFileRemove: ((fileId: string) => void) | undefined;
  onArchivePassword: ((fileId: string, password: string) => void) | undefined;
}) {
  const { fileScroll, removingFile, setRemovingFile, unlockingFile, setUnlockingFile } = panes;
  const { L, sourceAccounts } = facts;
  return (
    <div className="asset-inspector-files" ref={fileScroll}>
      <div className="asset-inspector-file-heading">
        <strong>{L('files')}</strong>
        <span title={p.manifestComplete === true ? undefined : L('totalUnverified')}>
          {fileCountLabel(p)}
        </span>
        <div className="asset-inspector-file-actions">
          {p.provider === 'local' && onLocalFilesAdd && (
            <>
              <button
                type="button"
                title={L('addLocalFiles')}
                aria-label={L('addLocalFiles')}
                disabled={localFilesBusy}
                onClick={() => onLocalFilesAdd(false)}
              >
                <Plus size={14} />
                <span>{L('addLocalFiles')}</span>
              </button>
              <button
                type="button"
                title={L('addLocalFolder')}
                aria-label={L('addLocalFolder')}
                disabled={localFilesBusy}
                onClick={() => onLocalFilesAdd(true)}
              >
                <FolderOpen size={14} />
              </button>
            </>
          )}
          {p.provider !== 'local' && onLocalFilesAdd && (
            <button
              type="button"
              title={L('addLocalFiles')}
              aria-label={L('addLocalFiles')}
              disabled={localFilesBusy}
              onClick={() => onLocalFilesAdd(false)}
            >
              <Plus size={14} />
            </button>
          )}
          {p.provider !== 'local' &&
            (retryDisabled && onCancelDownload ? (
              <button
                type="button"
                title={L('cancelDownload')}
                aria-label={L('cancelDownload')}
                disabled={cancelPending}
                onClick={onCancelDownload}
              >
                <X size={14} />
                <span>{L('cancelDownload')}</span>
              </button>
            ) : (
              onDownloadAll &&
              !isProductDownloaded(p) && (
                <button
                  type="button"
                  title={L('downloadAll')}
                  aria-label={L('downloadAll')}
                  disabled={retryDisabled || !p.files.length}
                  onClick={onDownloadAll}
                >
                  <Download size={14} />
                  <span>{L('downloadAll')}</span>
                </button>
              )
            ))}
          {p.provider !== 'local' &&
            onDeleteDownloads &&
            p.files.some((file) => !file.kept && hasLocalCopy(file)) && (
              <button
                type="button"
                className="asset-inspector-delete-downloads"
                title={L('deleteDownloads')}
                aria-label={L('deleteDownloads')}
                disabled={deleteDisabled}
                onClick={onDeleteDownloads}
              >
                <Trash2 size={14} />
              </button>
            )}
          <button
            type="button"
            className="asset-inspector-open-folder"
            aria-label={table(locale, 'fileAction').openFolder}
            title={table(locale, 'fileAction').openFolder}
            disabled={folderAvailable !== true || !onOpenFolder}
            onClick={() => {
              if (folderAvailable === true) onOpenFolder?.();
            }}
          >
            <FolderOpen size={14} />
            <span>{table(locale, 'fileAction').openFolder}</span>
          </button>
        </div>
      </div>
      {p.files.map((file) => {
        const openable = !!onFileOpen && openableFileIds.includes(file.id),
          sources = provenanceLines(locale, file.provenance ?? [], sourceAccounts),
          busy = busyFileIds.includes(file.id),
          // The archive asked for a password, or the one given was wrong.
          locked =
            file.status === 'extractionFailed' &&
            ['archive_password_required', 'archive_password_wrong'].includes(file.errorCode ?? ''),
          unlockable =
            !!onArchivePassword &&
            locked &&
            file.originalAvailable &&
            p.provider !== 'local' &&
            !file.kept &&
            !busy;
        return (
          <div className="asset-inspector-file" key={file.id}>
            <span className="asset-inspector-file-icon">
              {file.type.toLowerCase() === 'unitypackage' ? (
                <FileBox size={17} />
              ) : (
                <FileArchive size={17} />
              )}
            </span>
            <div className="asset-inspector-file-meta">
              {openable ? (
                <button
                  type="button"
                  className="asset-inspector-file-name"
                  aria-label={table(locale, 'fileAction').openFile + ' ' + file.name}
                  title={table(locale, 'fileAction').openFile}
                  onClick={() => {
                    if (openableFileIds.includes(file.id)) onFileOpen?.(file.id);
                  }}
                >
                  {file.name}
                </button>
              ) : (
                <span title={file.name}>{file.name}</span>
              )}
              <small>
                {[
                  typeof file.sizeBytes === 'number' && byteLabel(locale, file.sizeBytes),
                  L(file.status),
                  file.kept && L(file.userAdded ? 'fileAddedByYou' : 'fileRemovedFromStore'),
                ]
                  .filter(Boolean)
                  .join(' · ')}
              </small>
              {['failed', 'extractionFailed', 'missing'].includes(file.status) &&
                !(locked && busy) && (
                  <small className="asset-inspector-file-error">
                    {L(
                      file.status === 'extractionFailed'
                        ? locked
                          ? file.errorCode === 'archive_password_wrong'
                            ? 'archivePasswordWrong'
                            : 'archivePasswordRequired'
                          : 'failedExtraction'
                        : file.status === 'failed'
                          ? file.offerEnded || file.errorCode === 'store_file_unavailable'
                            ? 'fileUnavailable'
                            : 'failedTransfer'
                          : 'fileMissing',
                    )}
                  </small>
                )}
              {file.offerEnded && file.status !== 'failed' && (
                <small className="asset-inspector-file-error">{L('fileUnavailable')}</small>
              )}
              {sources.length > 0 && (
                <details>
                  <summary>{L('sourceDetails')}</summary>
                  {sources.map((line, index) => (
                    <small key={index}>{line}</small>
                  ))}
                </details>
              )}
            </div>
            {unlockable && removingFile !== file.id && (
              <button
                type="button"
                className="asset-inspector-file-unlock"
                aria-label={L('enterArchivePassword') + ' ' + file.name}
                aria-expanded={unlockingFile === file.id}
                title={L('enterArchivePassword')}
                onClick={() => setUnlockingFile(unlockingFile === file.id ? null : file.id)}
              >
                <KeyRound size={13} />
              </button>
            )}
            {onFileRetry &&
              p.provider !== 'local' &&
              !file.kept &&
              (onFileCancel && cancellableFileIds.includes(file.id) ? (
                <button
                  type="button"
                  aria-label={L('cancelDownload') + ' ' + file.name}
                  title={L('cancelDownload')}
                  disabled={cancelPending}
                  onClick={() => onFileCancel(file.id)}
                >
                  <X size={13} />
                </button>
              ) : (
                <button
                  type="button"
                  aria-label={
                    L(isFileDownloaded(file) ? 'redownload' : 'download') + ' ' + file.name
                  }
                  title={L(isFileDownloaded(file) ? 'redownload' : 'download')}
                  disabled={busyFileIds.includes(file.id)}
                  onClick={() => onFileRetry(file.id, isFileDownloaded(file))}
                >
                  {isFileDownloaded(file) ? <RefreshCw size={13} /> : <Download size={13} />}
                </button>
              ))}
            {p.provider !== 'local' &&
              onFileDelete &&
              hasLocalCopy(file) &&
              !busyFileIds.includes(file.id) && (
                <span className="asset-inspector-local-remove">
                  {removingFile === file.id ? (
                    <>
                      <button
                        type="button"
                        title={L('confirmDeleteFile')}
                        aria-label={L('confirmDeleteFile') + ' ' + file.name}
                        disabled={deleteDisabled}
                        onClick={() => {
                          onFileDelete(file.id);
                          setRemovingFile(null);
                        }}
                      >
                        <Check size={13} />
                      </button>
                      <button
                        type="button"
                        title={L('cancel')}
                        aria-label={L('cancel')}
                        onClick={() => setRemovingFile(null)}
                      >
                        <X size={13} />
                      </button>
                    </>
                  ) : (
                    <button
                      type="button"
                      title={L(file.userAdded ? 'removeLocalFile' : 'deleteDownloadedFile')}
                      aria-label={
                        L(file.userAdded ? 'removeLocalFile' : 'deleteDownloadedFile') +
                        ' ' +
                        file.name
                      }
                      disabled={deleteDisabled}
                      onClick={() => {
                        setRemovingFile(file.id);
                        setUnlockingFile(null);
                      }}
                    >
                      <Trash2 size={13} />
                    </button>
                  )}
                </span>
              )}
            {p.provider === 'local' && onLocalFileRemove && (
              <span className="asset-inspector-local-remove">
                {removingFile === file.id ? (
                  <>
                    <button
                      type="button"
                      title={L('confirmRemoveLocalFile')}
                      aria-label={L('confirmRemoveLocalFile') + ' ' + file.name}
                      disabled={localFilesBusy}
                      onClick={() => {
                        onLocalFileRemove(file.id);
                        setRemovingFile(null);
                      }}
                    >
                      <Check size={13} />
                    </button>
                    <button
                      type="button"
                      title={L('cancel')}
                      aria-label={L('cancel')}
                      disabled={localFilesBusy}
                      onClick={() => setRemovingFile(null)}
                    >
                      <X size={13} />
                    </button>
                  </>
                ) : (
                  <button
                    type="button"
                    title={L('removeLocalFile')}
                    aria-label={L('removeLocalFile') + ' ' + file.name}
                    disabled={localFilesBusy}
                    onClick={() => setRemovingFile(file.id)}
                  >
                    <Trash2 size={13} />
                  </button>
                )}
              </span>
            )}
            {unlockable && unlockingFile === file.id && (
              <ArchivePassword
                fileName={file.name}
                L={L}
                onExtract={(password) => {
                  onArchivePassword?.(file.id, password);
                  setUnlockingFile(null);
                }}
                onClose={() => setUnlockingFile(null)}
              />
            )}
          </div>
        );
      })}
    </div>
  );
}
