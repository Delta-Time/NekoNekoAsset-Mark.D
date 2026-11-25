import { knownLocale, message } from './messages';

export function wf(locale: string, key: string): string {
  return message(knownLocale(locale), 'workflow', key) ?? key;
}
