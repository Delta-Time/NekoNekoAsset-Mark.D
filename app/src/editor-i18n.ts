import { t } from './i18n';
import { message } from './messages';
import type { Locale } from './model';
import { wf } from './workflow-i18n';

export function editorText(locale: Locale, key: string): string {
  return (
    message(locale, 'editor', key) ??
    (['productURL', 'invalidURL'].includes(key) ? wf(locale, key) : t(locale, key))
  );
}
