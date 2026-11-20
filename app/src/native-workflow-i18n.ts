import { nativeErrorText } from './error-i18n';
import { t } from './i18n';
import { knownLocale, message } from './messages';
import type { Locale } from './model';
import { wf } from './workflow-i18n';

export function nativeWorkflowText(locale: string, key: string): string {
  const known = knownLocale(locale);
  const own =
    message(known, 'nativeWorkflow', key) ??
    (key.startsWith('library_') ? message(known, 'nativeWorkflow', key.toUpperCase()) : undefined);
  if (own !== undefined) return own;
  const workflow = wf(locale, key);
  if (workflow !== key) return workflow;
  const common = t(locale as Locale, key);
  if (common !== key) return common;
  return nativeErrorText(locale as Locale, key) ?? key;
}
