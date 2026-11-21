import { useEffect, useRef, useState } from 'react';
import { getCurrentWebview } from '@tauri-apps/api/webview';
import { isTauri } from '@tauri-apps/api/core';
import { categoryKeys, normalizeSearch } from '../../model';
import { errorCode, nativeFileAction } from '../../backend';
import { resolveTag } from '../../TagPicker';
import { absolutePath, emptyDraft, fileName, selectedPaths } from './import-parts';
import type { Source } from './import-parts';
import type { LocalImportProps } from './import-parts';
import type { ImportDrafts } from './useImportDrafts';
import type { ImportTarget } from './useImportTarget';
import type { ImportCheck } from './useImportCheck';

/** Adding sources by choosing or dropping them, reading a listing page, choosing a thumbnail or a
 * library location, and queueing the import. One action runs at a time. */
export function useImportActions({
  drafts,
  target,
  check,
  c,
}: {
  drafts: ImportDrafts;
  target: ImportTarget;
  check: ImportCheck;
  c: LocalImportProps;
}) {
  const {
    setSources,
    setBatchDrafts,
    mode,
    setSingleDraft,
    sourceUrl,
    updateDraft,
    seed,
    L,
    setThumbnail,
    tags,
    pendingTag,
    sources,
    batchDrafts,
    name,
    category,
    categoryManual,
    thumbnail,
    thumbnailUrl,
  } = drafts;
  const { setLibraryPath, destinationRootId, ready, libraryGate, setLibraryGate, libraryPath } =
    target;
  const { valid, suggestions, aliases } = check;
  const [dropping, setDropping] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  const lock = useRef(false);
  const draftId = useRef(crypto.randomUUID());
  const accepted = useRef(false);
  const mounted = useRef(true);
  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
    };
  }, []);
  async function native(
    service: 'storage' | 'dialog' | 'booth',
    action: string,
    args: Record<string, unknown> = {},
  ) {
    if (!c.onNative) throw new Error('nativeUnavailable');
    return c.onNative(service, action, args);
  }
  async function run(action: () => Promise<void>) {
    if (lock.current) return;
    lock.current = true;
    setBusy(true);
    setError('');
    try {
      await action();
    } catch (e) {
      setError(errorCode(e));
    } finally {
      lock.current = false;
      setBusy(false);
    }
  }
  function addSources(paths: string[], kind: Source['kind']) {
    if (paths.some((path) => !absolutePath(path))) throw new Error('root_not_absolute');
    setSources((previous) => [
      ...previous,
      ...paths
        .filter((path) => !previous.some((source) => source.path === path))
        .map((path) => ({ path, kind })),
    ]);
    setBatchDrafts((current) => ({
      ...Object.fromEntries(
        paths.map((path) => [path, emptyDraft(fileName(path).replace(/\.zip$/i, ''))]),
      ),
      ...current,
    }));
    if (mode === 'single' && paths[0])
      setSingleDraft((current) =>
        current.name.trim()
          ? current
          : {
              ...current,
              name:
                kind === 'folder' ? fileName(paths[0]) : fileName(paths[0]).replace(/\.[^.]+$/, ''),
            },
      );
  }
  useEffect(() => {
    if (!isTauri()) return;
    let stop: (() => void) | undefined;
    let alive = true;
    void getCurrentWebview()
      .onDragDropEvent((event) => {
        if (!alive || lock.current) return;
        const payload = event.payload;
        setDropping(payload.type === 'enter' || payload.type === 'over');
        if (payload.type === 'drop') {
          setDropping(false);
          try {
            addSources(payload.paths, 'unknown');
          } catch (error) {
            setError(errorCode(error));
          }
        }
      })
      .then((unlisten) => {
        if (alive) stop = unlisten;
        else unlisten();
      })
      .catch(() => {});
    return () => {
      alive = false;
      stop?.();
    };
  }, [mode]);
  const fetchMetadata = () =>
    run(async () => {
      if (!sourceUrl.trim()) throw new Error('invalidURL');
      const data = await native('booth', 'fetch_product_metadata', { url: sourceUrl.trim() });
      if (
        !data ||
        typeof data.title !== 'string' ||
        !categoryKeys.includes(data.category) ||
        !Array.isArray(data.tags)
      )
        throw new Error('invalidMetadata');
      updateDraft({
        name: data.title,
        category: data.category,
        categoryManual: true,
        tags: data.tags,
        sourceUrl: data.sourceUrl,
        thumbnail: null,
        thumbnailUrl: data.thumbnailUrl ?? '',
      });
    });
  useEffect(() => {
    if (seed?.sources?.length) addSources(seed.sources, 'unknown');
  }, [seed?.sources]);
  const chooseSources = (folder: boolean) =>
    run(async () => {
      const result = await native(
        'dialog',
        folder ? 'folder' : 'open_file',
        folder
          ? { multiple: true }
          : {
              multiple: true,
              ...(mode === 'batch' ? { filters: [{ name: 'ZIP', extensions: ['zip'] }] } : {}),
            },
      );
      addSources(selectedPaths(result), folder ? 'folder' : 'file');
    });
  const chooseThumbnail = () =>
    run(async () => {
      const [path] = selectedPaths(
        await native('dialog', 'open_file', {
          filters: [{ name: L('thumbnail'), extensions: ['png', 'jpg', 'jpeg', 'gif', 'webp'] }],
        }),
      );
      if (!path) return;
      const copied = await nativeFileAction<{ relativePath: string; path: string }>('copy_image', {
        path,
      });
      setThumbnail({ ...copied, name: fileName(path) });
    });
  const chooseLibrary = () =>
    run(async () => {
      const [path] = selectedPaths(await native('dialog', 'folder'));
      if (path) setLibraryPath(path);
    });
  const submit = () =>
    run(async () => {
      if (!valid || accepted.current) return;
      const destinationRoot = destinationRootId;
      if (!ready) {
        if (!libraryGate) {
          setLibraryGate(true);
          return;
        }
        if (!absolutePath(libraryPath)) throw new Error('root_not_absolute');
        if (c.onLibraryReady) await c.onLibraryReady(libraryPath, destinationRoot, true);
        else await native('storage', 'configure_root', { path: libraryPath });
      }
      if (!mounted.current) return;
      const allTags = [
        ...tags,
        ...(pendingTag.trim() ? [resolveTag(pendingTag, suggestions, aliases)] : []),
      ];
      const seen = new Set<string>();
      const uniqueTags = allTags.filter((tag) => {
        const key = normalizeSearch(tag);
        if (!key || seen.has(key)) return false;
        seen.add(key);
        return true;
      });
      const products =
        mode === 'batch'
          ? sources.map((source) => {
              const item =
                batchDrafts[source.path] ??
                emptyDraft(fileName(source.path).replace(/\.zip$/i, ''));
              return {
                source: source.path,
                name: item.name.trim(),
                category: item.category,
                categoryManual: item.categoryManual,
                tags: [
                  ...new Set([
                    ...item.tags,
                    ...(item.pendingTag.trim()
                      ? [resolveTag(item.pendingTag, suggestions, aliases)]
                      : []),
                  ]),
                ],
                sourceUrl: item.sourceUrl.trim(),
                ...(item.thumbnail ? { thumbnailPath: item.thumbnail.relativePath } : {}),
                thumbnailUrl: item.thumbnailUrl,
              };
            })
          : undefined;
      const result = await native('storage', 'import_local', {
        draftId: draftId.current,
        mode,
        sources: sources.map((source) => source.path),
        ...(destinationRoot ? { rootId: destinationRoot } : {}),
        name: name.trim(),
        category,
        categoryManual,
        tags: uniqueTags,
        ...(sourceUrl.trim() ? { sourceUrl: sourceUrl.trim() } : {}),
        ...(thumbnail ? { thumbnailPath: thumbnail.relativePath } : {}),
        thumbnailUrl,
        ...(products ? { products } : {}),
      });
      if (typeof result?.jobId !== 'string') throw new Error('operationFailed');
      accepted.current = true;
      c.onDirtyChange?.(false);
      const productIds = Array.isArray(result.productIds)
        ? result.productIds.filter((id: unknown): id is string => typeof id === 'string')
        : [];
      c.onNewAssets?.(productIds, mode === 'single' ? category : undefined, result.jobId);
      c.onNotice(L('importQueued'));
      c.onClose();
    });
  return {
    dropping,
    setDropping,
    busy,
    error,
    lock,
    fetchMetadata,
    chooseSources,
    chooseThumbnail,
    chooseLibrary,
    submit,
  };
}
export type ImportActions = ReturnType<typeof useImportActions>;
