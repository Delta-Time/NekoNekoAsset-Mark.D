import { useEffect, useMemo, useState } from 'react';
import type { KeyboardEvent, MouseEvent } from 'react';
import type { Product } from '../../model';
import { errorCode, invokeAction } from '../../backend';
import type { Results } from '../results/useResults';
import type { Selection } from './useSelection';
import type { Library } from '../library/useLibrary';
import type { Navigation } from '../shell/useNavigation';
import type { Notices } from '../shell/useNotice';

/** The selection within the current results, the active product with its full details and
 * openable files, and selection by click and keyboard. */
export function useSelectedProducts({
  results,
  selection,
  library,
  nav,
  notices,
  q,
}: {
  results: Results;
  selection: Selection;
  library: Library;
  nav: Navigation;
  notices: Notices;
  q: (key: string) => string;
}) {
  const { matchedIds, queryResult, queryKey, products, filtered, accountScopeKey } = results;
  const { selected, focusId, setFocusId, anchor, setSelected, pendingSuggestionProduct } =
    selection;
  const { nativeState, snapshotVersion } = library;
  const { view, setView, setTab, setInspector } = nav;
  const { notify } = notices;
  const selectedScope = useMemo(() => new Set(matchedIds), [queryResult, queryKey]);
  const visibleSelected = selected.filter((id) => selectedScope.has(id));
  const selectedSet = useMemo(() => new Set(visibleSelected), [selectedScope, selected]);
  const productMap = useMemo(
    () => new Map(products.map((product) => [product.id, product])),
    [products],
  );
  const activeSummary =
    filtered.find(
      (p) =>
        p.id ===
        (visibleSelected.length === 1 ? visibleSelected[0] : focusId || visibleSelected[0]),
    ) || filtered[0];
  const detailsKey = JSON.stringify([activeSummary?.id, accountScopeKey]);
  const [productDetails, setProductDetails] = useState<{ key: string; product: Product } | null>(
    null,
  );
  const active = productDetails?.key === detailsKey ? productDetails.product : activeSummary,
    chosen = visibleSelected.length ? visibleSelected : active ? [active.id] : [];
  const [availability, setAvailability] = useState<{
    key: string;
    folderAvailable: boolean;
    openableFileIds: string[];
  } | null>(null);
  const availabilityKey = JSON.stringify({
    id: active?.id,
    files: active?.files.map((file) => [
      file.id,
      file.status,
      file.originalAvailable,
      file.verifiedHash,
      file.relativePath,
    ]),
    roots: nativeState.roots.map((root) => [root.id, root.status, root.path]),
  });
  const select = (p: Product, e?: Pick<MouseEvent, 'shiftKey' | 'ctrlKey' | 'metaKey'>) => {
    if (view === 'settings') {
      setView('library');
      setTab('details');
      setInspector(true);
    } else if (view === 'activity') setView('library');
    setFocusId(p.id);
    if (e?.shiftKey && anchor.current) {
      const a = filtered.findIndex((p) => p.id === anchor.current),
        b = filtered.findIndex((item) => item.id === p.id);
      setSelected(filtered.slice(Math.max(0, Math.min(a, b)), Math.max(a, b) + 1).map((p) => p.id));
    } else if (e?.ctrlKey || e?.metaKey) {
      setSelected((ids) => (ids.includes(p.id) ? ids.filter((id) => id !== p.id) : [...ids, p.id]));
      anchor.current = p.id;
    } else {
      setSelected([p.id]);
      anchor.current = p.id;
    }
  };
  const keySelect = (e: KeyboardEvent, p: Product) => {
    if (e.nativeEvent.isComposing) return;
    if (e.key === 'Enter') {
      select(p);
      setInspector(true);
      return;
    }
    if (e.key === ' ') {
      e.preventDefault();
      select(p, { ctrlKey: true, metaKey: false, shiftKey: e.shiftKey });
    }
    if (['ArrowDown', 'ArrowUp', 'ArrowLeft', 'ArrowRight'].includes(e.key)) {
      e.preventDefault();
      const index = filtered.findIndex((item) => item.id === p.id),
        next =
          filtered[
            Math.max(
              0,
              Math.min(
                filtered.length - 1,
                index +
                  (['ArrowDown', 'ArrowRight'].includes(e.key) ? 1 : -1) *
                    (['ArrowDown', 'ArrowUp'].includes(e.key)
                      ? Number(
                          e.currentTarget.closest('.result-viewport')?.getAttribute('data-columns'),
                        ) || 1
                      : 1),
              ),
            )
          ];
      if (next) {
        select(next, { shiftKey: e.shiftKey, ctrlKey: false, metaKey: false });
        requestAnimationFrame(() =>
          requestAnimationFrame(() =>
            document.getElementById('product-' + next.id)?.focus({ preventScroll: true }),
          ),
        );
      }
    }
  };
  const resultKeys = (e: KeyboardEvent) => {
    const editing =
      e.target instanceof Element &&
      !!e.target.closest('input,textarea,select,[contenteditable="true"]');
    if (
      (e.ctrlKey || e.metaKey) &&
      e.key.toLowerCase() === 'a' &&
      !e.nativeEvent.isComposing &&
      !editing
    ) {
      e.preventDefault();
      setSelected(matchedIds);
    }
  };
  useEffect(() => {
    if (!activeSummary) {
      setProductDetails(null);
      return;
    }
    let current = true;
    void invokeAction<Product>('catalogue', 'product_details', {
      id: activeSummary.id,
      accountIds: JSON.parse(accountScopeKey),
    })
      .then((product) => {
        if (current) setProductDetails({ key: detailsKey, product });
      })
      .catch((error) => {
        if (current) notify(q(errorCode(error)));
      });
    return () => {
      current = false;
    };
  }, [activeSummary?.id, accountScopeKey, snapshotVersion]);
  useEffect(() => {
    if (!active || active.filesLoaded === false) {
      setAvailability(null);
      return;
    }
    let current = true;
    void invokeAction<{ folderAvailable: boolean; openableFileIds: string[] }>(
      'storage',
      'product_availability',
      { productId: active.id },
    )
      .then((result) => {
        if (current) setAvailability({ ...result, key: availabilityKey });
      })
      .catch(() => {
        if (current) setAvailability(null);
      });
    return () => {
      current = false;
    };
  }, [availabilityKey]);
  useEffect(() => {
    if (pendingSuggestionProduct.current) {
      if (queryResult?.key !== queryKey) return;
      const id = pendingSuggestionProduct.current;
      pendingSuggestionProduct.current = '';
      if (selectedScope.has(id)) {
        setSelected([id]);
        setFocusId(id);
        anchor.current = id;
        return;
      }
    }
    if (selected.some((id) => !selectedScope.has(id)))
      setSelected((old) => old.filter((id) => selectedScope.has(id)));
    if (focusId && !selectedScope.has(focusId)) setFocusId('');
  }, [selectedScope, queryResult, queryKey]);
  return {
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
  };
}
export type SelectedProducts = ReturnType<typeof useSelectedProducts>;
