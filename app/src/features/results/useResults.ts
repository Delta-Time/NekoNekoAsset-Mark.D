import { useEffect, useRef, useState } from 'react';
import type { Product } from '../../model';
import { errorCode, invokeAction } from '../../backend';
import { coalescedRefresh } from '../../refresh';
import type { Library } from '../library/useLibrary';
import type { Filters } from '../filters/useFilters';
import type { CollectionStore } from '../collections/useCollections';
import type { Notices } from '../shell/useNotice';

/** The listed products: one catalogue query per filter change, its rows and counts, and every
 * product summary seen so far (`products`). Native returns only summaries whose fingerprint
 * changed. */
export function useResults({
  library,
  filters,
  collectionStore,
  notices,
  q,
}: {
  library: Library;
  filters: Filters;
  collectionStore: CollectionStore;
  notices: Notices;
  q: (key: string) => string;
}) {
  const {
    nativeState,
    licenseEnabled,
    loaded,
    libraryState,
    loadingError,
    initialLoad,
    libraryStateRef,
    libraryRefreshPending,
    libraryEpoch,
    snapshotVersion,
  } = library;
  const { query, collection, sort, direction } = filters;
  const { selectedCollectionKey, committedCollections } = collectionStore;
  const { notify } = notices;
  const [products, setProducts] = useState<Product[]>([]),
    [catalogueTotal, setCatalogueTotal] = useState(0),
    [catalogueCounts, setCatalogueCounts] = useState<{
      all: number;
      downloaded: number;
      attention: number;
      hidden?: number;
    }>({ all: 0, downloaded: 0, attention: 0 });
  const scrollState = useRef({ index: 0, offset: 0 });
  const [queryResult, setQueryResult] = useState<{
      products: Product[];
      matchedIds: string[];
      total: number;
      allDownloaded?: boolean;
      key: string;
    } | null>(null),
    [queryBusy, setQueryBusy] = useState(false),
    [failedQueryKey, setFailedQueryKey] = useState('');
  const queryResultRef = useRef(queryResult);
  queryResultRef.current = queryResult;
  const queryTask = useRef<(() => Promise<void>) | null>(null),
    queryMounted = useRef(false),
    summaryCache = useRef(new Map<string, { fingerprint: string; product: Product }>());
  useEffect(() => {
    queryMounted.current = true;
    return () => {
      queryMounted.current = false;
    };
  }, []);
  const hideUnavailable = nativeState.preferences.hideUnavailable !== false,
    hideTermsOnly = nativeState.preferences.hideTermsOnly !== false,
    hideUnclassified = nativeState.preferences.hideUnclassified !== false,
    queryKey = JSON.stringify({
      query,
      collection,
      sort,
      direction,
      hideUnavailable,
      hideTermsOnly,
      hideUnclassified,
      licenseEnabled,
    }),
    accountScopeKey = JSON.stringify(
      query.accountIds === null ? null : [...new Set(query.accountIds)].sort(),
    );
  const filtered = queryResult?.key === queryKey ? queryResult.products : [];
  const matchedIds = queryResult?.key === queryKey ? queryResult.matchedIds : [];
  // A changed search, filter or sort has no rows until its query answers: show progress, never "no results".
  const resultsPending =
    !loaded ||
    (libraryState?.state === 'ready' &&
      !loadingError &&
      queryResult?.key !== queryKey &&
      failedQueryKey !== queryKey);
  // Product events can share a result; changed filters or collection membership cannot.
  const queryIdentity = JSON.stringify([queryKey, selectedCollectionKey]);
  const queryRequest = useRef({
    identity: queryIdentity,
    epoch: 0,
    key: queryKey,
    args: {
      ...query,
      license: licenseEnabled ? query.license : null,
      hiddenOnly: query.quick === 'hidden',
      hideUnavailable,
      hideTermsOnly,
      hideUnclassified,
      collectionId: committedCollections.find((c) => c.name === collection)?.id,
      sort,
      direction,
      limit: 10000,
      includeFiles: false,
      presentation: 'list',
      allSummaries: true,
    },
    onError: (error: unknown) => notify(q(errorCode(error))),
  });
  queryRequest.current = {
    identity: queryIdentity,
    epoch: queryRequest.current.epoch + (queryRequest.current.identity === queryIdentity ? 0 : 1),
    key: queryKey,
    args: {
      ...query,
      license: licenseEnabled ? query.license : null,
      hiddenOnly: query.quick === 'hidden',
      hideUnavailable,
      hideTermsOnly,
      hideUnclassified,
      collectionId: committedCollections.find((c) => c.name === collection)?.id,
      sort,
      direction,
      limit: 10000,
      includeFiles: false,
      presentation: 'list',
      allSummaries: true,
    },
    onError: (error: unknown) => notify(q(errorCode(error))),
  };
  if (!queryTask.current)
    queryTask.current = coalescedRefresh(async () => {
      if (
        !queryMounted.current ||
        !initialLoad.current ||
        libraryStateRef.current?.state !== 'ready' ||
        libraryRefreshPending.current
      )
        return;
      const request = queryRequest.current,
        epoch = libraryEpoch.current,
        hasCurrent = queryResultRef.current?.key === request.key;
      const current = () =>
        queryMounted.current &&
        request.epoch === queryRequest.current.epoch &&
        epoch === libraryEpoch.current &&
        !libraryRefreshPending.current;
      try {
        // Native returns only summaries whose fingerprint differs from this cache.
        const knownSummaries = Object.fromEntries(
          Array.from(summaryCache.current, ([id, entry]) => [id, entry.fingerprint]),
        );
        const result = await invokeAction('catalogue', 'query', {
          ...request.args,
          knownSummaries,
        });
        if (epoch !== libraryEpoch.current) return;
        for (const product of result.products)
          summaryCache.current.set(product.id, {
            fingerprint: result.fingerprints?.[product.id] ?? '',
            product,
          });
        if (!current()) return;
        const products = result.matchedIds.flatMap((id) => {
          const product = summaryCache.current.get(id)?.product;
          return product ? [product] : [];
        });
        setQueryResult({ ...result, products, key: request.key });
        setFailedQueryKey('');
        setCatalogueTotal(result.total);
        setCatalogueCounts(result.counts);
        if (result.products.length)
          setProducts((old) => {
            const cache = new Map(old.map((product) => [product.id, product]));
            for (const product of result.products) cache.set(product.id, product);
            return [...cache.values()];
          });
      } catch (error) {
        if (current()) {
          if (!hasCurrent) setQueryResult(null);
          setFailedQueryKey(request.key);
          request.onError(error);
        }
      } finally {
        if (current()) setQueryBusy(false);
      }
    });
  const quickCounts = catalogueCounts;
  useEffect(() => {
    if (!initialLoad.current) return;
    setQueryBusy(queryResultRef.current?.key !== queryKey);
    void queryTask.current!();
  }, [
    snapshotVersion,
    selectedCollectionKey,
    query,
    sort,
    direction,
    collection,
    hideUnavailable,
    hideTermsOnly,
    hideUnclassified,
    licenseEnabled,
  ]);
  return {
    products,
    setProducts,
    catalogueTotal,
    setCatalogueTotal,
    setCatalogueCounts,
    scrollState,
    queryResult,
    setQueryResult,
    queryBusy,
    setQueryBusy,
    setFailedQueryKey,
    summaryCache,
    queryKey,
    accountScopeKey,
    filtered,
    matchedIds,
    resultsPending,
    quickCounts,
  };
}
export type Results = ReturnType<typeof useResults>;
