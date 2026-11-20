import { knownLocale, message } from './messages';
import type { Locale } from './model';
import { nativeWorkflowText } from './native-workflow-i18n';

export function aiSettingsText(locale: Locale, key: string): string {
  return message(knownLocale(locale), 'aiSettings', key) ?? nativeWorkflowText(locale, key);
}
