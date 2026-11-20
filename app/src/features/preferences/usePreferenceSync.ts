import { useEffect, useRef } from 'react';
import { errorCode, invokeAction } from '../../backend';
import { changedPreferences } from '../../refresh';
import type { Library } from '../library/useLibrary';
import type { Notices } from '../shell/useNotice';
import type { UiPreferences } from './useUiPreferences';

/** Saves preferences to the library: `onPreferences` writes a patch, the UI preferences save
 * themselves 250 ms after a change, and `drainPreferenceWrites` waits for every pending write. */
export function usePreferenceSync({
  library,
  notices,
  uiPrefs,
  q,
}: {
  library: Library;
  notices: Notices;
  uiPrefs: UiPreferences;
  q: (key: string) => string;
}) {
  const {
    libraryStateRef,
    libraryRefreshPending,
    setLibraryOpening,
    preferencesRef,
    setNativeState,
    loaded,
    initialLoad,
  } = library;
  const { notify, run } = notices;
  const {
    locale,
    theme,
    presets,
    thumb,
    density,
    navWidth,
    inspectorWidth,
    cardColumns,
    listColumns,
    avatarSettings,
    uiScale,
    roundedCorners,
    reduceMotion,
    bundledFonts,
    oneHandedDrag,
  } = uiPrefs;
  const displayedPreferences = useRef<Record<string, unknown> | null>(null),
    pendingPreferences = useRef<Record<string, unknown>>({}),
    preferenceSaves = useRef(new Set<Promise<void>>());
  const onPreferences = (patch: Record<string, unknown>) => {
    const save = (async () => {
      try {
        if (
          !['ready', 'setup'].includes(libraryStateRef.current?.state ?? '') ||
          libraryRefreshPending.current
        )
          throw new Error('library_not_open');
        const result = await invokeAction('catalogue', 'preferences', { patch });
        if (result?.restartRequired === true) {
          setLibraryOpening(true);
          return;
        }
        preferencesRef.current = { ...preferencesRef.current, ...patch };
        setNativeState((old) => ({ ...old, preferences: { ...old.preferences, ...patch } }));
      } catch (error) {
        notify(q(errorCode(error)));
        throw error;
      }
    })();
    preferenceSaves.current.add(save);
    void save.then(
      () => preferenceSaves.current.delete(save),
      () => preferenceSaves.current.delete(save),
    );
    return save;
  };
  const drainPreferenceWrites = async () => {
    while (preferenceSaves.current.size) await Promise.all([...preferenceSaves.current]);
    const patch = pendingPreferences.current;
    pendingPreferences.current = {};
    if (Object.keys(patch).length) {
      try {
        await onPreferences(patch);
      } catch (error) {
        pendingPreferences.current = { ...patch, ...pendingPreferences.current };
        throw error;
      }
    }
  };
  useEffect(() => {
    if (
      !loaded ||
      !initialLoad.current ||
      !['ready', 'setup'].includes(libraryStateRef.current?.state ?? '') ||
      libraryRefreshPending.current
    )
      return;
    const current = {
      locale,
      theme,
      presets,
      thumb,
      density,
      navWidth,
      inspectorWidth,
      cardColumns,
      listColumns,
      avatarSettings,
      uiScale,
      roundedCorners,
      reduceMotion,
      bundledFonts,
      oneHandedDrag,
    };
    if (!displayedPreferences.current) {
      displayedPreferences.current = current;
      return;
    }
    Object.assign(
      pendingPreferences.current,
      changedPreferences(displayedPreferences.current, current),
    );
    displayedPreferences.current = current;
    if (!Object.keys(pendingPreferences.current).length) return;
    const timer = setTimeout(() => {
      const patch = pendingPreferences.current;
      if (!Object.keys(patch).length) return;
      pendingPreferences.current = {};
      run(
        onPreferences(patch).catch((error) => {
          pendingPreferences.current = { ...patch, ...pendingPreferences.current };
          throw error;
        }),
      );
    }, 250);
    return () => clearTimeout(timer);
  }, [
    loaded,
    locale,
    theme,
    presets,
    thumb,
    density,
    navWidth,
    inspectorWidth,
    cardColumns,
    listColumns,
    avatarSettings,
    uiScale,
    roundedCorners,
    reduceMotion,
    bundledFonts,
    oneHandedDrag,
  ]);
  return { displayedPreferences, pendingPreferences, onPreferences, drainPreferenceWrites };
}
export type PreferenceSync = ReturnType<typeof usePreferenceSync>;
