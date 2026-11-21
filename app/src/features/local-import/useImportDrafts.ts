import { useState } from 'react';
import type { Props } from '../../Workflows';
import { localImportText } from '../../local-import-i18n';
import { absolutePath, emptyDraft, fileName } from './import-parts';
import type { Draft, Source } from './import-parts';
import type { LocalImportProps } from './import-parts';

/** The sources to import and their drafts: one product, or one draft per folder or ZIP in batch
 * mode, with the draft of the selected source being edited. */
export function useImportDrafts({ c }: { c: LocalImportProps }) {
  const L = (key: string) => localImportText(c.locale, key);
  const seed = c.importSeed as Props['importSeed'] & { sources?: string[] };
  // Legacy seed.files are display names, not permission to read filesystem paths.
  const [sources, setSources] = useState<Source[]>(() =>
    [...new Set(seed?.sources ?? [])]
      .filter(absolutePath)
      .map((path) => ({ path, kind: 'unknown' })),
  );
  const [mode, setMode] = useState<'single' | 'batch'>('single');
  const [singleDraft, setSingleDraft] = useState<Draft>(() => emptyDraft(seed?.name ?? ''));
  const [batchDrafts, setBatchDrafts] = useState<Record<string, Draft>>({});
  const [selectedPath, setSelectedPath] = useState('');
  const editorPath = sources.some((source) => source.path === selectedPath)
    ? selectedPath
    : (sources[0]?.path ?? '');
  const draft =
    mode === 'single'
      ? singleDraft
      : (batchDrafts[editorPath] ?? emptyDraft(fileName(editorPath).replace(/\.zip$/i, '')));
  const { name, category, categoryManual, tags, pendingTag, sourceUrl, thumbnail, thumbnailUrl } =
    draft;
  const updateDraft = (patch: Partial<Draft>) => {
    if (mode === 'single') setSingleDraft((current) => ({ ...current, ...patch }));
    else
      setBatchDrafts((current) => ({
        ...current,
        [editorPath]: { ...(current[editorPath] ?? draft), ...patch },
      }));
  };
  const setName = (name: string) => updateDraft({ name });
  const setCategory = (category: string) => updateDraft({ category, categoryManual: true });
  const setTags = (tags: string[]) => updateDraft({ tags });
  const setPendingTag = (pendingTag: string) => updateDraft({ pendingTag });
  const setSourceUrl = (sourceUrl: string) => updateDraft({ sourceUrl });
  const setThumbnail = (thumbnail: Draft['thumbnail']) =>
    updateDraft({ thumbnail, thumbnailUrl: '' });
  return {
    L,
    seed,
    sources,
    setSources,
    mode,
    setMode,
    setSingleDraft,
    batchDrafts,
    setBatchDrafts,
    setSelectedPath,
    editorPath,
    draft,
    name,
    category,
    categoryManual,
    tags,
    pendingTag,
    sourceUrl,
    thumbnail,
    thumbnailUrl,
    updateDraft,
    setName,
    setCategory,
    setTags,
    setPendingTag,
    setSourceUrl,
    setThumbnail,
  };
}
export type ImportDrafts = ReturnType<typeof useImportDrafts>;
