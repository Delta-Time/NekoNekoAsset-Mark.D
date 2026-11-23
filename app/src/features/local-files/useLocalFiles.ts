import { useEffect, useMemo, useRef, useState } from 'react';
import { isTauri } from '@tauri-apps/api/core';
import { getCurrentWebview } from '@tauri-apps/api/webview';
import { categoryKeys } from '../../model';
import { dialog as nativeDialog, errorCode, invokeAction, updateLocalFiles } from '../../backend';
import type { LocalFileChanges } from '../../backend';
import { fresh } from '../filters/query';
import type { Filters } from '../filters/useFilters';
import type { Navigation } from '../shell/useNavigation';
import type { Library } from '../library/useLibrary';
import type { Native } from '../library/useNative';
import type { Notices } from '../shell/useNotice';
import type { UiPreferences } from '../preferences/useUiPreferences';

/** Local files: adding and removing a local product's files, files dropped on the library opening
 * the import flow, and listing a just-imported product's category when it already existed. */
export function useLocalFiles({
  filters,
  nav,
  library,
  native,
  notices,
  uiPrefs,
  q,
}: {
  filters: Filters;
  nav: Navigation;
  library: Library;
  native: Native;
  notices: Notices;
  uiPrefs: UiPreferences;
  q: (key: string) => string;
}) {
  const { query, collection, updateQuery } = filters;
  const { view, setImportSeed, setFlow, flow } = nav;
  const { nativeState } = library;
  const { refreshState } = native;
  const { run, notify } = notices;
  const { locale } = uiPrefs;
  const localFileLocks = useRef(new Set<string>()),
    [localFilePending, setLocalFilePending] = useState(new Set<string>());
  const latestLocalImport = useRef<{
      jobId: string;
      plannedIds: string[];
      category?: string;
      handled?: boolean;
    } | null>(null),
    importNavigation = useRef({ query, collection, view });
  importNavigation.current = { query, collection, view };
  useEffect(() => {
    if (!isTauri()) return;
    let disposed = false;
    let unlisten: (() => void) | undefined;
    void getCurrentWebview()
      .onDragDropEvent((event) => {
        if (event.payload.type !== 'drop' || !event.payload.paths.length) return;
        const point = event.payload.position;
        const target = document.elementFromPoint(
          point.x / devicePixelRatio,
          point.y / devicePixelRatio,
        );
        if (!target?.closest('.result-workspace,.wf-import')) return;
        const paths = event.payload.paths;
        setImportSeed({
          name: paths[0]
            .replace(/[\\/]+$/, '')
            .split(/[\\/]/)
            .pop()
            ?.replace(/\.[^.]+$/, ''),
          sources: paths,
        });
        setFlow('import');
      })
      .then((stop) => {
        if (disposed) stop();
        else unlisten = stop;
      })
      .catch(() => {});
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, []);
  const jobs = useMemo(
    () =>
      new Map(
        nativeState.jobs
          .filter(
            (j) =>
              ['download', 'storage:import_local'].includes(j.type) &&
              ['queued', 'running'].includes(j.state),
          )
          .flatMap((j) =>
            [
              ...new Set([j.productId, ...(j.productIds ?? [])].filter((id): id is string => !!id)),
            ].map((id) => [id, j] as const),
          ),
      ),
    [nativeState.jobs],
  );
  async function changeLocalFiles(
    productId: string,
    changes: LocalFileChanges | (() => Promise<LocalFileChanges | undefined>),
  ) {
    if (localFileLocks.current.has(productId) || jobs.has(productId))
      throw new Error('storage_busy');
    localFileLocks.current.add(productId);
    setLocalFilePending(new Set(localFileLocks.current));
    try {
      const args = typeof changes === 'function' ? await changes() : changes;
      if (!args) return;
      const result = await updateLocalFiles(productId, args);
      await refreshState();
      return result;
    } finally {
      localFileLocks.current.delete(productId);
      setLocalFilePending(new Set(localFileLocks.current));
    }
  }
  const addLocalFiles = (productId: string, folder: boolean) =>
    run(
      changeLocalFiles(productId, async () => {
        const result = await nativeDialog(folder ? 'folder' : 'open_file', { multiple: true });
        const sources = result?.paths ?? (result?.path ? [result.path] : []);
        return sources.length
          ? { draftId: crypto.randomUUID(), addSources: sources, removeFileIds: [] }
          : undefined;
      }),
    );
  useEffect(() => {
    const accepted = latestLocalImport.current;
    if (!accepted || accepted.handled) return;
    const job = nativeState.jobs.find(
      (job) => job.id === accepted.jobId && job.type === 'storage:import_local',
    );
    if (job?.state !== 'completed') return;
    accepted.handled = true;
    const reusedId = job.productIds?.find((id) => !accepted.plannedIds.includes(id));
    if (!reusedId) return;
    notify(q('assetExists'));
    void invokeAction('catalogue', 'product_details', { id: reusedId })
      .then((product) => {
        const current = importNavigation.current;
        if (
          latestLocalImport.current !== accepted ||
          current.view !== 'library' ||
          current.collection ||
          JSON.stringify(current.query) !==
            JSON.stringify({ ...fresh, category: accepted.category ?? 'all' })
        )
          return;
        if (categoryKeys.some((key) => key === product.category))
          updateQuery({ category: product.category });
      })
      .catch((error) => notify(q(errorCode(error))));
  }, [nativeState.jobs, locale, flow]);
  return { localFilePending, latestLocalImport, jobs, changeLocalFiles, addLocalFiles };
}
export type LocalFiles = ReturnType<typeof useLocalFiles>;
