import { knownLocale, message } from './messages';
import type { Locale } from './model';
import { nativeWorkflowText } from './native-workflow-i18n';

export function localImportText(locale: Locale, key: string): string {
  return message(knownLocale(locale), 'localImport', key) ?? nativeWorkflowText(locale, key);
}
