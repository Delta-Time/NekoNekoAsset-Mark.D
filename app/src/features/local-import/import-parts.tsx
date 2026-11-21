import type { Props } from '../../Workflows';
import type { ReactNode } from 'react';

export type LocalImportProps = Pick<
  Props,
  | 'locale'
  | 'products'
  | 'importSeed'
  | 'libraryReady'
  | 'nativeState'
  | 'onNative'
  | 'onLibraryReady'
  | 'onClose'
  | 'onNotice'
  | 'onNewAssets'
> & {
  onDirtyChange?: (dirty: boolean) => void;
};
export type Source = { path: string; kind: 'file' | 'folder' | 'unknown' };
export type Draft = {
  name: string;
  category: string;
  categoryManual: boolean;
  tags: string[];
  pendingTag: string;
  sourceUrl: string;
  thumbnail: { relativePath: string; path: string; name: string } | null;
  thumbnailUrl: string;
};
export const emptyDraft = (name = ''): Draft => ({
  name,
  category: 'NonVRChat',
  categoryManual: false,
  tags: [],
  pendingTag: '',
  sourceUrl: '',
  thumbnail: null,
  thumbnailUrl: '',
});
export const absolutePath = (path: string) =>
  /^(?:[A-Za-z]:[\\/]|\\\\[^\\]+\\[^\\]+|\/)/.test(path);
export const fileName = (path: string) =>
  path
    .replace(/[\\/]+$/, '')
    .split(/[\\/]/)
    .pop() ?? path;
// Dialogs can return one selection, many selections, or the native wrapper object.
export function selectedPaths(result: unknown): string[] {
  if (typeof result === 'string') return result ? [result] : [];
  if (Array.isArray(result)) return result.flatMap(selectedPaths);
  if (result && typeof result === 'object') {
    const selected = result as { paths?: unknown; path?: unknown };
    return selectedPaths(selected.paths ?? selected.path);
  }
  return [];
}
export function Field({ label, children }: { label: string; children: ReactNode }) {
  return (
    <label className="field">
      <span>{label}</span>
      {children}
    </label>
  );
}
