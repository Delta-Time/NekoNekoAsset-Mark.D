import { useEffect, useRef, useState } from 'react';
import { dialog as nativeDialog, errorCode, invokeAction } from '../../backend';
import type { Job } from '../../backend';
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
    [deletingDownloads, setDeletingDownloads] = useState<ReadonlySet<string>>(new Set());
  const watchedDownloads = useRef(new Set<string>()),
    staleProducts = useRef(new Set<string>());
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
    for (const job of nativeState.jobs as (Job & {
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
        invokeAction('licenses', 'sync', { productIds: ids }).then(
          (result: { jobId?: string | null }) => {
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
      | (Job & { failures?: Record<string, { errorCode?: string } | undefined> })
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
    watchDownloads,
    download,
    downloadFile,
    update,
    syncLicenses,
    needsAttention,
    dismissAttention,
    refusedFiles,
    exportZip,
  };
}
export type ProductActions = ReturnType<typeof useProductActions>;
