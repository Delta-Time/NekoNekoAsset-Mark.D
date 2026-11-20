import { message } from './messages';
import type { Locale } from './model';

// Cell names in the `license` table follow VN3's items A–X.
export function licenseText(locale: Locale, key: string): string | undefined {
  return message(locale, 'license', key);
}
