import { useEffect, useRef } from 'react';
import { invokeAction } from '../../backend';
import { coalescedRefresh } from '../../refresh';
import type { Library } from '../library/useLibrary';
import type { Results } from '../results/useResults';
import type { Filters } from '../filters/useFilters';
import type { Selection } from './useSelection';

/** While products selected in other views are out of sight, a list answer after a library change
 * is followed by one query over the whole library: it keeps their summaries current for the
 * actions and the batch editor, and products the library no longer holds, or that a setting hides
 * everywhere, leave the selection. Browsing an unchanged library asks nothing more. */
export function useSelectionCheck({
  library,
  results,
  filters,
  selection,
  inView,
}: {
  library: Library;
  results: Results;
  filters: Filters;
  selection: Selection;
  inView: ReadonlySet<string>;
}) {
  const { libraryEpoch, libraryStateRef, libraryRefreshPending, initialLoad, snapshotVersion } =
    library;
  const { summaryCache, absorb, queryResult, queryKey, hideUnavailable, hideTermsOnly } = results;
  const { query } = filters;
  const { selected, setSelected } = selection;
  // The list's account scope keeps the summaries' fingerprints shared with the list cache; Hidden
  // assets, R-18 Hide/Only and the hide preferences decide which products still count.
  const scope = {
    accountIds: query.accountIds,
    includeOffline: query.includeOffline,
    hiddenOnly: query.quick === 'hidden',
    sensitive: query.sensitive,
    hideUnavailable,
    hideTermsOnly,
  };
  const key = JSON.stringify(scope);
  const latest = useRef({ selected, scope, key, version: snapshotVersion });
  latest.current = { selected, scope, key, version: snapshotVersion };
  // The last completed check: library version, scope and every product it found.
  const checked = useRef({ version: -1, key: '', present: new Set<string>() });
  const task = useRef<(() => Promise<void>) | null>(null);
  if (!task.current)
    task.current = coalescedRefresh(async () => {
      const { selected: ids, scope, key, version } = latest.current,
        epoch = libraryEpoch.current,
        last = checked.current;
      if (
        !ids.length ||
        !initialLoad.current ||
        libraryStateRef.current?.state !== 'ready' ||
        libraryRefreshPending.current ||
        (last.version === version && last.key === key && ids.every((id) => last.present.has(id)))
      )
        return;
      const current = () =>
        epoch === libraryEpoch.current &&
        key === latest.current.key &&
        !libraryRefreshPending.current;
      const every = { hideUnclassified: false, includeFiles: false, presentation: 'list' };
      const listed = await invokeAction('catalogue', 'query', {
        ...scope,
        ...every,
        allSummaries: true,
        knownSummaries: Object.fromEntries(
          Array.from(summaryCache.current, ([id, entry]) => [id, entry.fingerprint]),
        ),
      });
      if (!current()) return;
      absorb(listed);
      const present = new Set(listed.matchedIds);
      // A product outside the account scope is only checked for existence.
      if (ids.some((id) => !present.has(id)) && (scope.accountIds || !scope.includeOffline)) {
        const all = await invokeAction('catalogue', 'query', {
          hiddenOnly: scope.hiddenOnly,
          sensitive: scope.sensitive,
          hideUnavailable: scope.hideUnavailable,
          hideTermsOnly: scope.hideTermsOnly,
          ...every,
          limit: 0,
        });
        if (!current()) return;
        for (const id of all.matchedIds) present.add(id);
      }
      checked.current = { version, key, present };
      const gone = new Set(ids.filter((id) => !present.has(id)));
      if (gone.size) setSelected((rows) => rows.filter((id) => !gone.has(id)));
    });
  useEffect(() => {
    if (queryResult?.key !== queryKey) return;
    const away = selected.filter((id) => !inView.has(id)),
      last = checked.current;
    // Unchanged library and scope, and every product out of view already found: nothing to ask.
    if (
      !away.length ||
      (last.version === snapshotVersion &&
        last.key === key &&
        away.every((id) => last.present.has(id)))
    )
      return;
    void task.current!().catch(() => {});
  }, [queryResult]);
}
