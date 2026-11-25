import { useState } from 'react';
import type { LocalImportProps } from './import-parts';

/** The library the import goes to, and the step that asks for a library location when none is
 * ready. */
export function useImportTarget({ c }: { c: LocalImportProps }) {
  const [rootId, setRootId] = useState('');
  const [libraryGate, setLibraryGate] = useState(false);
  const [libraryPath, setLibraryPath] = useState('');
  const roots = c.nativeState?.roots ?? [];
  const configuredRoot = c.nativeState?.preferences.libraryRootId;
  const defaultRootId =
    typeof configuredRoot === 'string'
      ? configuredRoot
      : roots.find((root) => Reflect.get(root, 'active') !== false)?.id;
  const destinationRootId = rootId || defaultRootId;
  const destination = roots.find((root) => root.id === destinationRootId);
  const ready =
    !!destination &&
    typeof destination.path === 'string' &&
    !!destination.path.trim() &&
    ['active', 'online', 'ready', 'configured'].includes(destination.status);
  return {
    rootId,
    setRootId,
    libraryGate,
    setLibraryGate,
    libraryPath,
    setLibraryPath,
    roots,
    defaultRootId,
    destinationRootId,
    ready,
  };
}
export type ImportTarget = ReturnType<typeof useImportTarget>;
