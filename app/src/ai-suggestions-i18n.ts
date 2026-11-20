import { aiSettingsText } from './ai-settings-i18n';
import { message } from './messages';
import type { Locale } from './model';

export function aiSuggestionsText(locale: Locale, key: string): string {
  return message(locale, 'aiSuggestions', key) ?? aiSettingsText(locale, key);
}
