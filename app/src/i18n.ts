import { message } from './messages';
import type { Locale } from './model';
import { uxText } from './ux-i18n';
export type { Locale } from './model';

export const locales: Locale[] = ['en', 'ja', 'ko', 'zh-Hans', 'zh-Hant'];
export const localeNames: Record<Locale, string> = {
  en: 'English',
  ja: '日本語',
  ko: '한국어',
  'zh-Hans': '简体中文',
  'zh-Hant': '繁體中文',
};

export function t(locale: Locale, key: string): string {
  return (
    uxText(locale, key) ?? message(locale, 'common', key) ?? message('en', 'common', key) ?? key
  );
}
