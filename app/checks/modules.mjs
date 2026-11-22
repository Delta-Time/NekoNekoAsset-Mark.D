// Loads the App's TypeScript modules for the checks, without a bundler or framework.
import { readFile } from 'node:fs/promises';
import { stripTypeScriptTypes } from 'node:module';
export const moduleUrl = (source) =>
  `data:text/javascript;base64,${Buffer.from(source).toString('base64')}`;
export const readTs = async (name, base = import.meta.url) =>
  stripTypeScriptTypes(await readFile(new URL(name, base), 'utf8'));
export const localeFiles = Object.fromEntries(
  await Promise.all(
    ['en', 'ja', 'ko', 'zh-Hans', 'zh-Hant'].map(async (locale) => [
      locale,
      await readFile(new URL(`../src/locales/${locale}.json`, import.meta.url), 'utf8'),
    ]),
  ),
);
export const messagesUrl = moduleUrl(
  (await readTs('../src/messages.ts')).replace(
    /import (\w+) from '\.\/locales\/([\w-]+)\.json';/g,
    (_, name, locale) => `const ${name} = ${localeFiles[locale]};`,
  ),
);
export const withMessages = (source) =>
  source.replace("from './messages'", `from '${messagesUrl}'`);
export const uxI18nUrl = moduleUrl(withMessages(await readTs('../src/ux-i18n.ts')));
export const i18nUrl = moduleUrl(
  withMessages((await readTs('../src/i18n.ts')).replace("from './ux-i18n'", `from '${uxI18nUrl}'`)),
);
export const modelUrl = moduleUrl(
  (await readTs('../src/model.ts')).replace("from './i18n'", `from '${i18nUrl}'`),
);
export const inferenceUrl = moduleUrl(
  (await readTs('../src/category-inference.ts')).replaceAll("from './model'", `from '${modelUrl}'`),
);
export const editsUrl = moduleUrl(
  (await readTs('../src/product-edits.ts'))
    .replaceAll("from './model'", `from '${modelUrl}'`)
    .replace("from './category-inference'", `from '${inferenceUrl}'`),
);
export const model = {
  ...(await import(modelUrl)),
  ...(await import(inferenceUrl)),
  ...(await import(editsUrl)),
};
