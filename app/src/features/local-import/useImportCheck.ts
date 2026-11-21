import { useEffect, useMemo } from 'react';
import { emptyDraft, fileName } from './import-parts';
import type { Draft } from './import-parts';
import type { LocalImportProps } from './import-parts';
import type { ImportDrafts } from './useImportDrafts';

/** Tag suggestions, whether the drafts can be imported, and whether closing would lose anything. */
export function useImportCheck({ drafts, c }: { drafts: ImportDrafts; c: LocalImportProps }) {
  const {
    sourceUrl,
    mode,
    sources,
    batchDrafts,
    draft,
    name,
    tags,
    pendingTag,
    thumbnail,
    category,
  } = drafts;
  const suggestions = useMemo(
    () => [
      ...new Set([
        ...(c.nativeState?.tagOptions ?? []).map((tag) => tag.label),
        ...c.products.flatMap((product) => product.tags),
      ]),
    ],
    [c.nativeState?.tagOptions, c.products],
  );
  const aliases = useMemo(
    () =>
      Object.fromEntries((c.nativeState?.tagOptions ?? []).map((tag) => [tag.label, tag.aliases])),
    [c.nativeState?.tagOptions],
  );
  let validUrl = !sourceUrl.trim();
  try {
    const url = new URL(sourceUrl.trim());
    validUrl = ['https:', 'http:'].includes(url.protocol) && !url.username && !url.password;
  } catch {}
  const batchInvalid =
    mode === 'batch' &&
    sources.some((source) => source.kind === 'file' && !/\.zip$/i.test(source.path));
  const draftValid = (item: Draft) => {
    if (!item.name.trim()) return false;
    if (!item.sourceUrl.trim()) return true;
    try {
      const url = new URL(item.sourceUrl.trim());
      return ['https:', 'http:'].includes(url.protocol) && !url.username && !url.password;
    } catch {
      return false;
    }
  };
  const valid =
    sources.length > 0 &&
    (mode === 'batch'
      ? sources.every((source) =>
          draftValid(
            batchDrafts[source.path] ?? emptyDraft(fileName(source.path).replace(/\.zip$/i, '')),
          ),
        )
      : draftValid(draft)) &&
    !batchInvalid;
  const dirty = !!(
    sources.length ||
    name.trim() ||
    tags.length ||
    pendingTag.trim() ||
    sourceUrl.trim() ||
    thumbnail ||
    category !== 'NonVRChat' ||
    mode !== 'single'
  );
  useEffect(() => {
    c.onDirtyChange?.(dirty);
  }, [dirty, c.onDirtyChange]);
  useEffect(() => () => c.onDirtyChange?.(false), [c.onDirtyChange]);
  return { suggestions, aliases, validUrl, batchInvalid, valid };
}
export type ImportCheck = ReturnType<typeof useImportCheck>;
