import { useEffect, useRef, useState, type CSSProperties } from 'react';
import { isLightTheme } from '../../model';
import { dialog, errorCode, nativeFileAction } from '../../backend';
import {
  builtInTheme,
  exportThemeConfig,
  parseThemeConfig,
  prefixed,
  secondaryThemeDefaults,
  validColor,
} from '../appearance/themeConfig';
import type { EditSubject } from './useEditSubject';
import type { DialogFrame } from './useDialogFrame';
import type { DetailDrafts } from './useDetailDrafts';

/** The theme editor's colours, previewed in the App while they are valid, with import and export.
 * The dialog itself keeps the last valid colours. */
export function useThemeDraft({
  subject,
  frame,
  details,
}: {
  subject: EditSubject;
  frame: DialogFrame;
  details: DetailDrafts;
}) {
  const { p } = subject;
  const { inFlight, setBusy, setError, alive } = frame;
  const { setName, name } = details;
  const [theme, setTheme] = useState<Record<string, string>>(() => ({
    ...builtInTheme,
    ...secondaryThemeDefaults(p.themeTokens),
    ...Object.fromEntries(
      Object.entries(p.themeTokens).map(([key, value]) => [key.replace(/^--/, ''), value]),
    ),
  }));
  const initialTheme = useRef(theme);
  const lastValidTheme = useRef(theme);
  const themeValid = Object.keys(builtInTheme).every((key) => validColor(theme[key]));
  if (themeValid) lastValidTheme.current = theme;
  useEffect(() => {
    if (p.dialog === 'theme' && themeValid) p.onThemePreview(prefixed(theme));
  }, [theme]);
  const importTheme = async () => {
    if (inFlight.current) return;
    inFlight.current = true;
    setBusy(true);
    setError('');
    try {
      const chosen = await dialog('open_file', {
        filters: [{ name: 'JSON', extensions: ['json'] }],
      });
      if (!chosen) return;
      const result = await nativeFileAction('read_text', { path: chosen.path });
      const text = typeof result === 'string' ? result : (result.text ?? result.content);
      if (typeof text !== 'string' || new TextEncoder().encode(text).byteLength > 32_768)
        throw new Error('invalidTheme');
      const tokens = parseThemeConfig(text);
      const imported = JSON.parse(text);
      if (!alive.current) return;
      setTheme(tokens);
      if (typeof imported.name === 'string' && imported.name.trim()) setName(imported.name.trim());
      setError('');
    } catch (error) {
      if (alive.current) setError(error instanceof SyntaxError ? 'invalidTheme' : errorCode(error));
    } finally {
      inFlight.current = false;
      if (alive.current) setBusy(false);
    }
  };
  const exportTheme = async () => {
    if (inFlight.current) return;
    if (!themeValid) {
      setError('invalidTheme');
      return;
    }
    inFlight.current = true;
    setBusy(true);
    setError('');
    try {
      await exportThemeConfig(theme, name);
    } catch (error) {
      if (alive.current) setError(errorCode(error));
    } finally {
      inFlight.current = false;
      if (alive.current) setBusy(false);
    }
  };
  const themeDialogStyle =
    p.dialog === 'theme'
      ? ({
          ...prefixed(lastValidTheme.current),
          color: lastValidTheme.current.text,
          colorScheme: isLightTheme(prefixed(lastValidTheme.current)) ? 'light' : 'dark',
        } as CSSProperties)
      : undefined;
  return { theme, setTheme, initialTheme, themeValid, importTheme, exportTheme, themeDialogStyle };
}
export type ThemeDraft = ReturnType<typeof useThemeDraft>;
