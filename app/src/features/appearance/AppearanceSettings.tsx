import type { Locale } from '../../model';
import { localeNames } from '../../i18n';
import { resetText } from '../../settings-reset-i18n';
import ScaleControl from '../../ScaleControl';
import Toggle from '../../Toggle';
import ThemePicker from '../../ThemePicker';
import { exportThemeConfig } from './themeConfig';
import Button from '../../shared/Button';
import type { UiPreferences } from '../preferences/useUiPreferences';
import type { Appearance } from './useAppearance';
import type { Navigation } from '../shell/useNavigation';
import type { Notices } from '../shell/useNotice';

/** Settings › Appearance: language, scale, appearance toggles and themes. */
export default function AppearanceSettings({
  uiPrefs,
  appearance,
  nav,
  notices,
  q,
}: {
  uiPrefs: UiPreferences;
  appearance: Appearance;
  nav: Navigation;
  notices: Notices;
  q: (key: string) => string;
}) {
  const {
    locale,
    setLocale,
    uiScale,
    roundedCorners,
    setRoundedCorners,
    reduceMotion,
    setReduceMotion,
    oneHandedDrag,
    setOneHandedDrag,
    bundledFonts,
    setBundledFonts,
    theme,
    setTheme,
    presets,
  } = uiPrefs;
  const {
    changeScale,
    themeDeleteBusy,
    setThemeDeleteTarget,
    selectedTheme,
    selectedThemeName,
    setThemePreview,
    selectedThemeTokens,
    themeOptions,
  } = appearance;
  const { setDialog } = nav;
  const { run } = notices;
  const localeOptions = Object.entries(localeNames).map(([id, name]) => (
    <option key={id} value={id}>
      {name}
    </option>
  ));
  return (
    <div className="appearance-content">
      <label className="field">
        <span>{q('language')}</span>
        <select value={locale} onChange={(e) => setLocale(e.target.value as Locale)}>
          {localeOptions}
        </select>
      </label>
      <label className="field">
        <span>{q('uiScale')}</span>
        <ScaleControl label={q('uiScale')} value={uiScale} onCommit={changeScale} />
      </label>
      <div className="appearance-toggles">
        <Toggle label={q('roundedCorners')} checked={roundedCorners} onChange={setRoundedCorners} />
        <Toggle label={q('oneHandedDrag')} checked={oneHandedDrag} onChange={setOneHandedDrag} />
        <Toggle label={q('bundledFonts')} checked={bundledFonts} onChange={setBundledFonts} />
        <Toggle label={q('reduceMotion')} checked={reduceMotion} onChange={setReduceMotion} />
      </div>
      <label className="field">
        <span>{q('themePreset')}</span>
      </label>
      <ThemePicker locale={locale} options={themeOptions} value={theme} onChange={setTheme} />
      <div className="theme-actions">
        <Button disabled={themeDeleteBusy} onClick={() => setDialog('theme')}>
          {q('editTheme')}
        </Button>
        <Button
          className="danger"
          disabled={themeDeleteBusy || !presets.some((p) => p.id === theme)}
          onClick={() => setThemeDeleteTarget({ id: selectedTheme.id, name: selectedThemeName })}
        >
          {resetText(locale, 'deleteTheme')}
        </Button>
        <div className="theme-transfer-actions">
          <Button
            disabled={themeDeleteBusy}
            onClick={() => {
              setThemePreview(null);
              setDialog('theme');
            }}
          >
            {q('importTheme')}
          </Button>
          <Button
            disabled={themeDeleteBusy}
            onClick={() => run(exportThemeConfig(selectedThemeTokens, selectedThemeName))}
          >
            {q('exportTheme')}
          </Button>
        </div>
      </div>
    </div>
  );
}
