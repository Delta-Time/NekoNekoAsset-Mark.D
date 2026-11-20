import type { NativeRoot, NativeState } from '../../backend';

/** The root the preferences select, or the first one not marked inactive. */
export const configuredLibraryRoot = (state: Pick<NativeState, 'roots' | 'preferences'>) =>
  typeof state.preferences.libraryRootId === 'string'
    ? state.roots.find((root) => root.id === state.preferences.libraryRootId)
    : state.roots.find((root) => Reflect.get(root, 'active') !== false);
export const mappedLibraryRoot = (root: NativeRoot | undefined) =>
  !!root &&
  typeof root.path === 'string' &&
  !!root.path.trim() &&
  ['active', 'online', 'ready', 'configured'].includes(root.status);
export const libraryAvailable = (state: Pick<NativeState, 'roots' | 'preferences'>) =>
  mappedLibraryRoot(configuredLibraryRoot(state));
