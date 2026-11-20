import { message } from './messages';
import type { Locale } from './model';

export const resetText = (locale: Locale, key: string) =>
  message(locale, 'reset', key) ?? message('en', 'reset', key) ?? key;

export const resetErrorText = (locale: Locale, code: string) => message(locale, 'resetError', code);
