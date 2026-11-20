import { useState } from 'react';
import type { Product } from '../../model';
import { errorCode, nativeFileAction } from '../../backend';
import type { ProductOverlayChange } from '../../backend';
import type { Native } from '../library/useNative';
import type { Notices } from '../shell/useNotice';
import type { SelectedProducts } from '../selection/useSelectedProducts';
import type { Filters } from '../filters/useFilters';
import type { Results } from '../results/useResults';
import type { Selection } from '../selection/useSelection';
import type { Navigation } from '../shell/useNavigation';

/** Edits to products: overlay patches with undo, hiding and unhiding store products, deleting
 * local ones, and opening the edit dialog for the selection. */
export function useProductEdits({
  native,
  notices,
  selectedProducts,
  filters,
  results,
  selection,
  nav,
  q,
}: {
  native: Native;
  notices: Notices;
  selectedProducts: SelectedProducts;
  filters: Filters;
  results: Results;
  selection: Selection;
  nav: Navigation;
  q: (key: string) => string;
}) {
  const { onNative, refreshAvatarOptions, reload } = native;
  const { notify } = notices;
  const { productMap, setProductDetails, visibleSelected } = selectedProducts;
  const { query } = filters;
  const { setQueryResult, setProducts, summaryCache } = results;
  const { setSelected, setFocusId } = selection;
  const { setDialog } = nav;
  const [deleteLocalSelection, setDeleteLocalSelection] = useState<{
    ids: string[];
    names: string[];
  } | null>(null);
  const [undo, setUndo] = useState<string | null>(null);
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
  const edit = () => setDialog(visibleSelected.length > 1 ? 'batch' : 'edit');
  return {
    deleteLocalSelection,
    setDeleteLocalSelection,
    undo,
    batchPatch,
    userPatch,
    localSelection,
    requestLocalDelete,
    onlineSelection,
    hiddenSelection,
    hideProducts,
    deleteLocalProducts,
    undoAction,
    edit,
  };
}
export type ProductEdits = ReturnType<typeof useProductEdits>;
