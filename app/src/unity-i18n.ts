import type { UnityPreparation } from './backend';
import { nativeErrorText } from './error-i18n';
import { message } from './messages';
import type { Locale } from './model';

export function unityText(locale: Locale, key: string): string {
  return message(locale, 'unity', key === 'compiling' ? 'refreshing' : key) ?? key;
}
export function unityProgressLabel(
  locale: Locale,
  value: UnityPreparation & { stage?: string },
): string {
  if (value.stage === 'preparing' && value.preparationWaitingForEditor)
    return unityText(locale, 'waitingForEditor');
  const phase = value.stage === 'preparing' ? value.preparationPhase : null;
  const label = unityText(locale, phase || value.stage || 'preparing');
  if (!phase) return label;
  if (phase === 'checkTargets')
    return typeof value.preparationTotalEntries === 'number' && value.preparationTotalEntries > 0
      ? `${label} · ${(value.preparationEntries ?? 0).toLocaleString(locale)}/${value.preparationTotalEntries.toLocaleString(locale)}`
      : label;
  const bytes = (size: number) =>
    size >= 1_073_741_824
      ? `${(size / 1_073_741_824).toFixed(2)} GiB`
      : `${(size / 1_048_576).toFixed(1)} MiB`;
  return typeof value.preparationTotalBytes === 'number' && value.preparationTotalBytes > 0
    ? `${label} · ${bytes(value.preparationBytes ?? 0)}/${bytes(value.preparationTotalBytes)}`
    : label;
}
export function unityError(locale: Locale, key: string): string {
  const aliases: Record<string, string> = {
    unity_leaf_stale: 'UNITY_SOURCE_CHANGED',
    unity_leaf_invalid: 'UNITY_SOURCE_CHANGED',
    unity_selection_empty: 'UNITY_SELECTION_EMPTY',
    unity_payload_too_large: 'UNITY_SELECTION_TOO_LARGE',
  };
  return (
    message(locale, 'unity', aliases[key] ?? key) ??
    nativeErrorText(locale, key) ??
    nativeErrorText(locale, 'operationFailed') ??
    'Operation failed'
  );
}
