import { useState } from 'react';
import { dialog as nativeDialog, errorCode, invokeAction } from '../../backend';
import { licenseText } from '../../license-i18n';
import type { Library } from '../library/useLibrary';
import type { Notices } from '../shell/useNotice';
import type { Native } from '../library/useNative';
import type { SelectedProducts } from '../selection/useSelectedProducts';
import type { ProductEdits } from './useProductEdits';
import type { UiPreferences } from '../preferences/useUiPreferences';

/** Downloads and other product actions: queue, cancel and delete downloads, update from the store,
 * read licenses, export a ZIP and dismiss attention. */
export function useProductActions({
  library,
  notices,
  native,
  selectedProducts,
  edits,
  uiPrefs,
  q,
}: {
  library: Library;
  notices: Notices;
  native: Native;
  selectedProducts: SelectedProducts;
  edits: ProductEdits;
  uiPrefs: UiPreferences;
  q: (key: string) => string;
}) {
  const { nativeState } = library;
  const { run, notify } = notices;
  const { onNative } = native;
  const { productMap } = selectedProducts;
  const { onlineSelection } = edits;
  const { locale } = uiPrefs;
  const [productConfirm, setProductConfirm] = useState<{
      kind: 'downloadAll' | 'deleteDownloads';
      ids: string[];
      names: string[];
    } | null>(null),
    [requestedDeletes, setRequestedDeletes] = useState<ReadonlySet<string>>(new Set());
  // Asked for until their jobs show, then waiting or running.
  const deletingDownloads: ReadonlySet<string> = new Set([
    ...requestedDeletes,
    ...nativeState.jobs.flatMap((job) =>
      job.type === 'storage:delete_downloads' &&
      ['queued', 'running'].includes(job.state) &&
      job.productId
        ? [job.productId]
        : [],
    ),
  ]);
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
    setRequestedDeletes((old) => new Set([...old, ...ids]));
    try {
      await invokeAction('storage', 'delete_downloads', args);
    } finally {
      setRequestedDeletes((old) => new Set([...old].filter((id) => !ids.includes(id))));
    }
  };
  const download = (ids: string[]) => {
    if (ids.length) run(onNative('storage', 'enqueue_products', { productIds: ids }));
  };
  const downloadFile = (fileId: string, redownload: boolean) =>
    run(onNative('storage', 'enqueue_file', { fileId, redownload }));
  const update = (ids: string[]) => {
    if (ids.length) run(onNative('booth', 'update_products', { productIds: ids }));
  };
  const syncLicenses = (ids: string[]) => {
    if (ids.length)
      run(
        invokeAction('licenses', 'sync', { productIds: ids }).then((result) => {
          if (!result?.jobId) notify(licenseText(locale, 'noLicenseWork')!);
        }),
      );
  };
  const needsAttention = (ids: string[]) =>
    ids.some((id) => productMap.get(id)?.status === 'attention');
  const dismissAttention = (ids: string[]) => {
    const targets = ids.filter((id) => productMap.get(id)?.status === 'attention');
    if (targets.length) run(onNative('catalogue', 'dismiss_attention', { productIds: targets }));
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
  return {
    productConfirm,
    setProductConfirm,
    deletingDownloads,
    activeDownloads,
    cancelDownloads,
    allUnavailable,
    hasDownloadedFiles,
    requestDeleteDownloads,
    deleteDownloads,
    download,
    downloadFile,
    update,
    syncLicenses,
    needsAttention,
    dismissAttention,
    exportZip,
  };
}
export type ProductActions = ReturnType<typeof useProductActions>;
