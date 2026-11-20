import { message } from './messages';
import type { Locale } from './model';

export function uxText(locale: Locale, key: string): string | undefined {
  return message(locale, 'ux', key);
}
