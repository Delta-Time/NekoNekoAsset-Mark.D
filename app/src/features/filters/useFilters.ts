import { useLayoutEffect, useRef, useState } from 'react';
import { defaultSortDirection } from '../../model';
import type { SortDirection } from '../../model';
import { invokeAction } from '../../backend';
import { fresh, leavingHidden } from './query';
import type { Query } from './query';
import type { Selection } from '../selection/useSelection';
import type { Notices } from '../shell/useNotice';
import type { Navigation } from '../shell/useNavigation';
import type { Library } from '../library/useLibrary';

/** What the library lists: search, sidebar scope, toolbar filters, sort, collection and saved
 * search. Changing them keeps the selection, except entering or leaving Hidden assets. */
export function useFilters({
  selection,
  notices,
  nav,
  library,
}: {
  selection: Selection;
  notices: Notices;
  nav: Navigation;
  library: Library;
}) {
  const { pendingSuggestionProduct, clearSelection, leaveView, setSelected } = selection;
  const { run } = notices;
  const { setView, setTab, setInspector } = nav;
  const { licenseEnabled } = library;
  const [query, setQuery] = useState<Query>({ ...fresh }),
    [sort, setSort] = useState('acquired'),
    [direction, setDirection] = useState<SortDirection>('desc');
  const [collection, setCollection] = useState(''),
    [savedScope, setSavedScope] = useState<number | null>(null);
  // Hidden assets are a place of their own, with Unhide as their action: a selection does not
  // cross into them or out of them, whichever control moved there.
  const hidden = query.quick === 'hidden',
    place = useRef(hidden);
  useLayoutEffect(() => {
    if (place.current === hidden) return;
    place.current = hidden;
    setSelected([]);
  }, [hidden]);
  const updateQuery = (patch: Partial<Query>) => {
    pendingSuggestionProduct.current = '';
    setQuery((old) => ({
      ...old,
      ...(patch.category || patch.avatar || patch.quick ? leavingHidden(old) : {}),
      ...patch,
    }));
    leaveView();
    setSavedScope(null);
  };
  const openSuggestedProduct = (id: string) =>
    run(
      invokeAction('catalogue', 'product_details', { id }).then((product) => {
        pendingSuggestionProduct.current = id;
        setQuery({
          ...fresh,
          sensitive: query.sensitive,
          search: product.title,
          category: product.category,
        });
        setCollection('');
        setSavedScope(null);
        // Opening one suggested product selects only it, as clicking it would.
        clearSelection();
        setView('library');
        setTab('details');
        setInspector(true);
      }),
    );
  const clearFilters = () => {
    setQuery({ ...fresh, sensitive: query.sensitive });
    setSort('acquired');
    setDirection('desc');
    setCollection('');
    setSavedScope(null);
    leaveView();
  };
  const changeSort = (value: string) => {
    setSort(value);
    setDirection(defaultSortDirection(value));
    setSavedScope(null);
    leaveView();
  };
  const defaultAccounts = query.includeOffline && query.accountIds === null;
  const filtersActive =
    !!query.search ||
    query.category !== 'all' ||
    query.avatar !== 'all' ||
    query.status !== 'all' ||
    !defaultAccounts ||
    query.quick !== 'all' ||
    !!query.acquisition ||
    (licenseEnabled && !!query.license) ||
    !!collection ||
    sort !== 'acquired' ||
    direction !== 'desc';
  return {
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
  };
}
export type Filters = ReturnType<typeof useFilters>;
