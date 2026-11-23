import { useEffect, useLayoutEffect, useMemo, useState } from 'react';
import type { KeyboardEvent, MouseEvent } from 'react';
import type { Product } from '../../model';
import { errorCode, invokeAction } from '../../backend';
import type { Results } from '../results/useResults';
import type { Selection } from './useSelection';
import type { Library } from '../library/useLibrary';
import type { Navigation } from '../shell/useNavigation';
import type { Notices } from '../shell/useNotice';
import type { Filters } from '../filters/useFilters';
import { useSelectionCheck } from './useSelectionCheck';

/** The selection against the current results, the active product with its full details and
 * openable files, and selection by click and keyboard. Products selected in other views stay
 * selected, and the details pane shows the selection wherever it is. */
export function useSelectedProducts({
  results,
  selection,
  library,
  nav,
  notices,
  filters,
  q,
}: {
  results: Results;
  selection: Selection;
  library: Library;
  nav: Navigation;
  notices: Notices;
  filters: Filters;
  q: (key: string) => string;
}) {
  const { matchedIds, queryResult, queryKey, products, filtered, accountScopeKey } = results;
  const {
    selected,
    focusId,
    setFocusId,
    anchor,
    setSelected,
    addToSelection,
    pendingSuggestionProduct,
  } = selection;
  const { nativeState, snapshotVersion } = library;
  const { view, setView, setTab, setInspector } = nav;
  const { notify } = notices;
  const inView = useMemo(() => new Set(matchedIds), [queryResult, queryKey]);
  const visibleSelected = selected.filter((id) => inView.has(id));
  const selectedSet = useMemo(() => new Set(selected), [selected]);
  // Every summary seen this session, so products selected in other views are found too.
  const productMap = useMemo(
    () => new Map(products.map((product) => [product.id, product])),
    [products],
  );
  useSelectionCheck({ library, results, filters, selection, inView });
  // R-18 Hide and Only are a privacy setting, not browsing: products they hide leave the selection.
  const sensitive = filters.query.sensitive;
  useLayoutEffect(() => {
    if (sensitive !== 'hide' && sensitive !== 'only') return;
    setSelected((ids) => {
      const kept = ids.filter((id) => {
        const product = productMap.get(id);
        return !product || product.sensitive === (sensitive === 'only');
      });
      return kept.length === ids.length ? ids : kept;
    });
  }, [sensitive]);
  // One selected product is the details pane's, in view or not; without a selection it shows the
  // focused product, else the first in view. Several selected show their summary instead.
  const only = selected.length === 1 ? selected[0] : '';
  const activeSummary = only
    ? (productMap.get(only) ?? filtered.find((p) => p.id === only))
    : filtered.find((p) => p.id === (focusId || visibleSelected[0])) || filtered[0];
  // The account filter limits acquisition and owned prices to its accounts. A product selected
  // under another filter that none of its accounts meet is read with every account instead.
  const scoped: string[] | null = JSON.parse(accountScopeKey);
  const detailsAccounts =
    scoped && activeSummary && !activeSummary.accountIds.some((id) => scoped.includes(id))
      ? null
      : scoped;
  const detailsKey = JSON.stringify([activeSummary?.id, detailsAccounts]);
  const [productDetails, setProductDetails] = useState<{ key: string; product: Product } | null>(
    null,
  );
  const active = productDetails?.key === detailsKey ? productDetails.product : activeSummary;
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
    if (e?.shiftKey) {
      // The range from the anchor replaces this view's part of the selection; products
      // selected in other views stay. Without an anchor in view the range is the product.
      const a = filtered.findIndex((item) => item.id === anchor.current),
        b = filtered.findIndex((item) => item.id === p.id),
        range = filtered
          .slice(a < 0 ? b : Math.min(a, b), Math.max(a, b) + 1)
          .map((item) => item.id);
      setSelected((ids) => [...ids.filter((id) => !inView.has(id)), ...range]);
      if (a < 0) anchor.current = p.id;
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
      addToSelection(matchedIds);
    }
  };
  useEffect(() => {
    if (!activeSummary) {
      setProductDetails(null);
      return;
    }
    let current = true;
    void invokeAction('catalogue', 'product_details', {
      id: activeSummary.id,
      accountIds: detailsAccounts ?? undefined,
    })
      .then((product) => {
        if (current) setProductDetails({ key: detailsKey, product });
      })
      .catch((error) => {
        // A product removed elsewhere leaves with the next list answer and selection check.
        const code = errorCode(error);
        if (current && code !== 'catalogue.product_missing') notify(q(code));
      });
    return () => {
      current = false;
    };
  }, [detailsKey, snapshotVersion]);
  useEffect(() => {
    if (!active || active.filesLoaded === false) {
      setAvailability(null);
      return;
    }
    let current = true;
    void invokeAction('storage', 'product_availability', { productId: active.id })
      .then((result: { folderAvailable: boolean; openableFileIds: string[] }) => {
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
      if (inView.has(id)) {
        setSelected([id]);
        setFocusId(id);
        anchor.current = id;
        return;
      }
    }
    if (focusId && !inView.has(focusId)) setFocusId('');
  }, [inView, queryResult, queryKey]);
  return {
    selectedSet,
    productMap,
    setProductDetails,
    active,
    availability,
    availabilityKey,
    select,
    keySelect,
    resultKeys,
  };
}
export type SelectedProducts = ReturnType<typeof useSelectedProducts>;
