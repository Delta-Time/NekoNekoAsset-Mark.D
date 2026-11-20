import { useRef } from 'react';
import { errorCode } from '../../backend';
import type { UiPreferences } from '../preferences/useUiPreferences';
import type { Library } from '../library/useLibrary';
import type { PreferenceSync } from '../preferences/usePreferenceSync';
import type { Native } from '../library/useNative';
import type { Notices } from '../shell/useNotice';
import type { Filters } from '../filters/useFilters';

/** Compatible avatars: options under the user's names, which ones the sidebar shows, and saving
 * renamed avatars with the other preferences. */
export function useAvatars({
  uiPrefs,
  library,
  preferenceSync,
  native,
  notices,
  filters,
  q,
}: {
  uiPrefs: UiPreferences;
  library: Library;
  preferenceSync: PreferenceSync;
  native: Native;
  notices: Notices;
  filters: Filters;
  q: (key: string) => string;
}) {
  const { avatarSettings, setAvatarSettings } = uiPrefs;
  const { libraryEpoch, nativeState } = library;
  const { drainPreferenceWrites, onPreferences, displayedPreferences } = preferenceSync;
  const { refreshAvatarOptions } = native;
  const { notify } = notices;
  const { query, updateQuery } = filters;
  const avatarSettingsRef = useRef(avatarSettings);
  avatarSettingsRef.current = avatarSettings;
  const changeAvatarDetails = async (
    id: string,
    patch: { displayName: string; shopName: string },
  ) => {
    const epoch = libraryEpoch.current;
    await drainPreferenceWrites();
    if (epoch !== libraryEpoch.current) throw new Error('library_not_open');
    const next = {
      ...avatarSettingsRef.current,
      [id]: { ...avatarSettingsRef.current[id], ...patch },
    };
    await onPreferences({ avatarSettings: next });
    if (epoch !== libraryEpoch.current) return;
    avatarSettingsRef.current = next;
    if (displayedPreferences.current) displayedPreferences.current.avatarSettings = next;
    setAvatarSettings(next);
    await refreshAvatarOptions().catch((error) => notify(q(errorCode(error))));
  };
  const avatarOptions = (nativeState.avatarOptions ?? []).map((option) => {
      const settings = avatarSettings[option.id];
      return {
        ...option,
        displayName:
          settings?.displayName === undefined
            ? option.displayName || option.sourceDisplayName
            : settings.displayName.trim() || option.sourceDisplayName || option.displayName,
        shopName:
          settings?.shopName === undefined
            ? option.shopName || option.sourceShopName
            : settings.shopName.trim() || option.sourceShopName || option.shopName,
      };
    }),
    avatarLabel = (id: string) => {
      const option =
        avatarOptions.find((option) => option.id === id) ??
        avatarOptions.find((option) => option.aliases.includes(id));
      return (
        option?.displayName ||
        [option?.officialName, option?.englishName].filter(Boolean).join(' / ') ||
        id
      );
    };
  const avatarEnabled = (id: string) => {
    const option = avatarOptions.find((option) => option.id === id),
      explicit = avatarSettings[id]?.enabled ?? option?.explicitEnabled;
    if (typeof explicit === 'boolean') return explicit;
    const automatic = nativeState.preferences.avatarAutomaticEnabled;
    if (automatic && typeof automatic === 'object' && !Array.isArray(automatic))
      return Reflect.get(automatic, id) === true;
    return option?.enabled ?? false;
  };
  const changeAvatars = (ids: string[], enabled: boolean) => {
    setAvatarSettings((old) => ({
      ...old,
      ...Object.fromEntries(ids.map((id) => [id, { ...old[id], enabled }])),
    }));
    if (!enabled && ids.includes(query.avatar)) updateQuery({ avatar: 'all' });
  };
  const avatarChoices = avatarOptions
    .filter((option) => avatarEnabled(option.id))
    .map((option) => option.id);
  return {
    changeAvatarDetails,
    avatarOptions,
    avatarLabel,
    avatarEnabled,
    changeAvatars,
    avatarChoices,
  };
}
export type Avatars = ReturnType<typeof useAvatars>;
