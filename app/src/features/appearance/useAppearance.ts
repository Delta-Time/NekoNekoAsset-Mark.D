import { useEffect, useRef, useState } from 'react';
import { isLightTheme } from '../../model';
import { errorCode, nativeZoom } from '../../backend';
import { defaultThemeOptions, themeDisplayName } from '../../ThemePicker';
import { neutralTokens } from './themes';
import type { UiPreferences } from '../preferences/useUiPreferences';
import type { PreferenceSync } from '../preferences/usePreferenceSync';
import type { Notices } from '../shell/useNotice';
import type { Library } from '../library/useLibrary';

/** Appearance: the selected and previewed theme, saving and deleting custom themes, zoom, the
 * document language, font loading, and the theme and class of the App root. */
export function useAppearance({
  uiPrefs,
  preferenceSync,
  notices,
  library,
  q,
}: {
  uiPrefs: UiPreferences;
  preferenceSync: PreferenceSync;
  notices: Notices;
  library: Library;
  q: (key: string) => string;
}) {
  const {
    presets,
    theme,
    locale,
    setPresets,
    setTheme,
    zoomTarget,
    setUiScale,
    uiScale,
    bundledFonts,
    roundedCorners,
    reduceMotion,
  } = uiPrefs;
  const { onPreferences, pendingPreferences } = preferenceSync;
  const { notify } = notices;
  const { loaded, initialLoad } = library;
  const [themePreview, setThemePreview] = useState<Record<string, string> | null>(null);
  const zoomQueue = useRef(Promise.resolve());
  const selectedTheme =
      presets.find((p) => p.id === theme) ||
      defaultThemeOptions.find((p) => p.id === theme) ||
      defaultThemeOptions[0],
    selectedThemeTokens = { ...neutralTokens, ...selectedTheme.tokens },
    selectedThemeName = ['dark', 'light'].includes(selectedTheme.id)
      ? q(selectedTheme.id)
      : themeDisplayName(locale, selectedTheme),
    themeTokens = themePreview || selectedThemeTokens;
  const choosePreset = async (tokens: Record<string, string>, name = 'Custom') => {
    let unique = name.trim() || 'Custom',
      suffix = 2;
    while (presets.some((p) => p.name.toLocaleLowerCase() === unique.toLocaleLowerCase()))
      unique = `${name} (${suffix++})`;
    const preset = { id: 'theme-' + Date.now(), name: unique, tokens };
    const next = [...presets, preset];
    await onPreferences({ presets: next, theme: preset.id });
    setPresets(next);
    setTheme(preset.id);
    setThemePreview(null);
  };
  const themeDeleting = useRef(false),
    [themeDeleteBusy, setThemeDeleteBusy] = useState(false);
  const [themeDeleteTarget, setThemeDeleteTarget] = useState<{ id: string; name: string } | null>(
    null,
  );
  const deleteTheme = async (id: string) => {
    if (themeDeleting.current) return;
    if (!presets.some((p) => p.id === id)) throw new Error('entryUnavailable');
    themeDeleting.current = true;
    setThemeDeleteBusy(true);
    const next = presets.filter((p) => p.id !== id);
    try {
      await onPreferences({ presets: next, ...(theme === id ? { theme: 'dark' } : {}) });
      setPresets(next);
      setTheme((current) => (current === id ? 'dark' : current));
      setThemePreview(null);
    } finally {
      themeDeleting.current = false;
      setThemeDeleteBusy(false);
    }
  };
  const changeScale = (scale: number) => {
    const bounded = Math.max(80, Math.min(300, Math.round(scale)));
    zoomTarget.current = bounded;
    zoomQueue.current = zoomQueue.current.then(async () => {
      if (bounded !== zoomTarget.current) return;
      try {
        const result = await nativeZoom(bounded);
        if (bounded === zoomTarget.current) {
          pendingPreferences.current.uiScale = result.scale;
          setUiScale(result.scale);
        }
      } catch (error) {
        if (bounded === zoomTarget.current) zoomTarget.current = uiScale;
        notify(q(errorCode(error)));
      }
    });
  };
  useEffect(() => {
    if (loaded && initialLoad.current) changeScale(zoomTarget.current);
  }, [loaded]);
  useEffect(() => {
    document.documentElement.lang = locale;
  }, [locale]);
  // Decode the UI-language face and the Japanese face for product titles while the library loads, so CJK text is not blank on first paint (fonts.css uses font-display:block).
  useEffect(() => {
    if (!loaded || !bundledFonts) return;
    const ui =
      locale === 'zh-Hant'
        ? 'Noto Sans TC'
        : locale === 'zh-Hans'
          ? 'Noto Sans SC'
          : locale === 'ko'
            ? 'Noto Sans KR'
            : 'Noto Sans JP';
    for (const family of new Set([ui, 'Noto Sans JP']))
      void document.fonts.load(`13px "${family}"`, 'あ一').catch(() => {});
  }, [loaded, bundledFonts, locale]);
  const appTheme = themeTokens['--bg']
    ? isLightTheme(themeTokens)
      ? 'light'
      : 'dark'
    : theme === 'light'
      ? 'light'
      : 'dark';
  const appClass =
    'app corrected-shell variant-a' +
    (roundedCorners ? '' : ' square-corners') +
    (reduceMotion ? ' reduce-motion' : '') +
    (bundledFonts ? '' : ' system-fonts');
  return {
    setThemePreview,
    zoomQueue,
    selectedTheme,
    selectedThemeTokens,
    selectedThemeName,
    themeTokens,
    choosePreset,
    themeDeleteBusy,
    themeDeleteTarget,
    setThemeDeleteTarget,
    deleteTheme,
    changeScale,
    appTheme,
    appClass,
  };
}
export type Appearance = ReturnType<typeof useAppearance>;
