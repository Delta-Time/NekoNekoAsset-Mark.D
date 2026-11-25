import { message } from './messages';
import type { Locale } from './model';

export function shellText(locale: Locale, key: string): string | undefined {
  return message(locale, 'shell', key);
}
