import { useRef, useState } from 'react';
import { thumbnailSizes } from '../../model';
import type { Locale } from '../../model';
import { localeNames } from '../../i18n';
import type { AvatarSetting } from '../../backend';
import { initialPresets, safeTokens } from '../appearance/themes';
import type { Preset } from '../appearance/themes';

const params = new URLSearchParams(location.search);

/** The UI preferences the library stores: language, theme, result layout, panel widths, avatar
 * settings and appearance. */
export function useUiPreferences() {
  const [locale, setLocale] = useState<Locale>(() => {
    const value = params.get('locale') || 'en';
    return Object.hasOwn(localeNames, value) ? (value as Locale) : 'en';
  });
  const [theme, setTheme] = useState(params.get('theme') || 'dark');
  const [presets, setPresets] = useState<Preset[]>(initialPresets);
  const [thumb, setThumb] = useState(128),
    [density, setDensity] = useState(74);
  const [navWidth, setNavWidth] = useState(176),
    [inspectorWidth, setInspectorWidth] = useState(336),
    [cardColumns, setCardColumns] = useState<string[]>(['shop']),
    [listColumns, setListColumns] = useState<string[]>([
      'shop',
      'category',
      'files',
      'acquired',
      'price',
    ]);
  const columns = thumb === 0 ? listColumns : cardColumns,
    setColumns = thumb === 0 ? setListColumns : setCardColumns;
  const [avatarSettings, setAvatarSettings] = useState<Record<string, AvatarSetting>>({});
  const [uiScale, setUiScale] = useState(100),
    zoomTarget = useRef(100);
  const [roundedCorners, setRoundedCorners] = useState(true),
    [reduceMotion, setReduceMotion] = useState(false),
    [bundledFonts, setBundledFonts] = useState(true),
    [oneHandedDrag, setOneHandedDrag] = useState(false);
  /** Applies the preferences stored in the library; its first load calls this once. */
  const hydrate = (p: Record<string, unknown>) => {
    if (Object.hasOwn(localeNames, String(p.locale))) setLocale(p.locale as Locale);
    if (typeof p.theme === 'string') setTheme(p.theme);
    if (Array.isArray(p.presets))
      setPresets(
        p.presets.flatMap((value) => {
          if (!value || typeof value !== 'object') return [];
          const preset = value as Preset,
            tokens = safeTokens(preset.tokens);
          return typeof preset.name === 'string' && typeof preset.id === 'string' && tokens
            ? [{ ...preset, tokens }]
            : [];
        }),
      );
    if (typeof p.thumb === 'number' && thumbnailSizes.includes(p.thumb as any)) setThumb(p.thumb);
    if ([40, 74, 90].includes(Number(p.density))) setDensity(Number(p.density));
    if (typeof p.navWidth === 'number') setNavWidth(Math.max(132, Math.min(300, p.navWidth)));
    if (typeof p.inspectorWidth === 'number')
      setInspectorWidth(Math.max(240, Math.min(900, p.inspectorWidth)));
    if (Array.isArray(p.cardColumns)) setCardColumns(p.cardColumns as string[]);
    if (Array.isArray(p.listColumns)) setListColumns(p.listColumns as string[]);
    if (
      p.avatarSettings &&
      typeof p.avatarSettings === 'object' &&
      !Array.isArray(p.avatarSettings)
    )
      setAvatarSettings(p.avatarSettings as Record<string, AvatarSetting>);
    if (
      typeof p.uiScale === 'number' &&
      Number.isFinite(p.uiScale) &&
      p.uiScale >= 80 &&
      p.uiScale <= 300
    ) {
      zoomTarget.current = p.uiScale;
      setUiScale(p.uiScale);
    }
    if (typeof p.roundedCorners === 'boolean') setRoundedCorners(p.roundedCorners);
    if (typeof p.reduceMotion === 'boolean') setReduceMotion(p.reduceMotion);
    if (typeof p.bundledFonts === 'boolean') setBundledFonts(p.bundledFonts);
    if (typeof p.oneHandedDrag === 'boolean') setOneHandedDrag(p.oneHandedDrag);
  };
  return {
    locale,
    setLocale,
    theme,
    setTheme,
    presets,
    setPresets,
    thumb,
    setThumb,
    density,
    setDensity,
    navWidth,
    setNavWidth,
    inspectorWidth,
    setInspectorWidth,
    cardColumns,
    listColumns,
    columns,
    setColumns,
    avatarSettings,
    setAvatarSettings,
    uiScale,
    setUiScale,
    zoomTarget,
    roundedCorners,
    setRoundedCorners,
    reduceMotion,
    setReduceMotion,
    bundledFonts,
    setBundledFonts,
    oneHandedDrag,
    setOneHandedDrag,
    hydrate,
  };
}
export type UiPreferences = ReturnType<typeof useUiPreferences>;
