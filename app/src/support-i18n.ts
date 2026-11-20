import { message, type TableKey } from './messages';
import type { Locale } from './model';

export function supportText(locale: Locale, key: TableKey<'support'>): string {
  return message(locale, 'support', key) ?? key;
}
