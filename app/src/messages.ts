import en from './locales/en.json';
import ja from './locales/ja.json';
import ko from './locales/ko.json';
import zhHans from './locales/zh-Hans.json';
import zhHant from './locales/zh-Hant.json';
import type { Locale } from './model';

// UI text lives in one JSON file per language in ./locales, with en.json as the source.
// A key is `<table>.<key>`, and each table belongs to one lookup function:
//
//   common          t                    i18n.ts
//   ux              uxText               ux-i18n.ts
//   workflow        wf                   workflow-i18n.ts
//   nativeWorkflow  nativeWorkflowText   native-workflow-i18n.ts
//   shell           shellText            shell-i18n.ts
//   error           nativeErrorText      error-i18n.ts
//   editor          editorText           editor-i18n.ts
//   aiSettings      aiSettingsText       ai-settings-i18n.ts
//   aiSuggestions   aiSuggestionsText    ai-suggestions-i18n.ts
//   localImport     localImportText      local-import-i18n.ts
//   license         licenseText          license-i18n.ts
//   reset           resetText            settings-reset-i18n.ts
//   resetError      resetErrorText       settings-reset-i18n.ts
//   support         supportText          support-i18n.ts
//   unity           unityText            unity-i18n.ts
//   update          updateText           update-i18n.ts
//
// Most functions fall back to another table when their own lacks a key, so the same key
// can appear in several tables; the function a component calls decides which text wins.
// A few components read a whole table with `table()`: aiPersonality, aiPresetTitle,
// aiPresetDescription, editDialog, fileAction, spending and theme.

export type MessageKey = keyof typeof en;

/** The keys of one table without its prefix, e.g. TableKey<'support'>. */
export type TableKey<Table extends string, Key = MessageKey> = Key extends `${Table}.${infer Rest}`
  ? Rest
  : never;

// tsc rejects a language file that lacks any key of en.json.
const messages: Record<Locale, Record<MessageKey, string>> = {
  en,
  ja,
  ko,
  'zh-Hans': zhHans,
  'zh-Hant': zhHant,
};

/** The text of `table.key` in `locale`, or undefined when the table lacks it or the locale is unknown. */
export function message(locale: string, table: string, key: string): string | undefined {
  if (!Object.hasOwn(messages, locale)) return undefined;
  return (messages[locale as Locale] as Record<string, string>)[`${table}.${key}`];
}

const tables = new Map<string, Record<string, string>>();

/** Every text of one table in `locale`, keyed without the table prefix. */
export function table<Table extends string>(
  locale: Locale,
  name: Table,
): Record<TableKey<Table>, string> {
  const id = `${locale} ${name}`;
  let found = tables.get(id);
  if (!found) {
    const prefix = `${name}.`;
    found = Object.fromEntries(
      Object.entries(messages[locale])
        .filter(([key]) => key.startsWith(prefix))
        .map(([key, text]) => [key.slice(prefix.length), text]),
    );
    tables.set(id, found);
  }
  return found as Record<TableKey<Table>, string>;
}

/** Whether `table` has `key`. */
export const hasMessage = (table: string, key: string) => Object.hasOwn(en, `${table}.${key}`);

/** `locale` when it has a language file, otherwise English. */
export const knownLocale = (locale: string): Locale =>
  Object.hasOwn(messages, locale) ? (locale as Locale) : 'en';
