import { isLightTheme } from '../../model';
import { dialog, nativeFileAction } from '../../backend';

/** The default dark theme; its keys are every token a theme sets. */
export const builtInTheme = {
  bg: '#171717',
  panel: '#202020',
  raised: '#292929',
  border: '#373737',
  text: '#ededed',
  muted: '#a1a1a1',
  accent: '#b6daca',
  'secondary-accent': '#8bb8f0',
  'secondary-accent-text': '#171717',
  control: '#303030',
  positive: '#b6daca',
  warning: '#dfc689',
  negative: '#e49c9c',
};
export const tokenLabels: Record<string, string> = {
  bg: 'background',
  panel: 'surface',
  raised: 'raisedSurface',
  border: 'border',
  text: 'text',
  muted: 'secondaryText',
  accent: 'accent',
  'secondary-accent': 'secondaryAccent',
  'secondary-accent-text': 'secondaryAccentText',
  control: 'controls',
  positive: 'success',
  warning: 'attention',
  negative: 'failed',
};
export const prefixed = (tokens: Record<string, string>) =>
  Object.fromEntries(
    Object.entries(tokens).map(([key, value]) => ['--' + key.replace(/^--/, ''), value]),
  );
export const secondaryThemeDefaults = (tokens: Record<string, string>) =>
  isLightTheme(prefixed(tokens))
    ? { 'secondary-accent': '#356cae', 'secondary-accent-text': '#ffffff' }
    : { 'secondary-accent': '#8bb8f0', 'secondary-accent-text': '#171717' };
export const validColor = (color: unknown): color is string =>
  typeof color === 'string' && /^#[0-9a-fA-F]{6}$/.test(color);
/** Reads a theme file: format NekoNekoAsset.Theme version 1, known tokens only, each a six-digit
 * hex colour. The secondary accent colours may be left out and then follow the theme's lightness. */
export function parseThemeConfig(text: string): Record<string, string> {
  const value = JSON.parse(text);
  if (
    value?.format !== 'NekoNekoAsset.Theme' ||
    value.version !== 1 ||
    !value.tokens ||
    typeof value.tokens !== 'object' ||
    Array.isArray(value.tokens)
  )
    throw new Error('invalidTheme');
  if (Object.keys(value.tokens).some((key) => !Object.hasOwn(builtInTheme, key)))
    throw new Error('invalidTheme');
  const optional = secondaryThemeDefaults(value.tokens);
  if (
    Object.keys(builtInTheme).some(
      (key) =>
        (!Object.hasOwn(optional, key) || Object.hasOwn(value.tokens, key)) &&
        !validColor(value.tokens[key]),
    )
  )
    throw new Error('invalidTheme');
  return { ...optional, ...value.tokens };
}
/** Saves the tokens as a theme file where the user chooses; false when they cancel. */
export async function exportThemeConfig(
  tokens: Record<string, string>,
  name = '',
): Promise<boolean> {
  const normalized = Object.fromEntries(
    Object.entries(tokens).map(([key, value]) => [key.replace(/^--/, ''), value]),
  );
  const config = {
    format: 'NekoNekoAsset.Theme',
    version: 1,
    name: name.trim(),
    tokens: normalized,
  };
  config.tokens = parseThemeConfig(JSON.stringify(config));
  const chosen = await dialog('save_file', {
    defaultName: 'NekoNekoAsset-theme.json',
    filters: [{ name: 'JSON', extensions: ['json'] }],
  });
  if (!chosen) return false;
  await nativeFileAction('write_text', {
    path: chosen.path,
    text: JSON.stringify(config, null, 2),
  });
  return true;
}
