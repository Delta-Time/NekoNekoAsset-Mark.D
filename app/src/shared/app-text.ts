import type { Locale } from '../model';
import { t } from '../i18n';
import { shellText } from '../shell-i18n';
import { resetErrorText } from '../settings-reset-i18n';
import { uxText } from '../ux-i18n';
import { unityError } from '../unity-i18n';
import { licenseText } from '../license-i18n';
import { nativeErrorText } from '../error-i18n';
import { nativeWorkflowText } from '../native-workflow-i18n';

/** The App shell's text for a key: license, reset and Unity keys first, then UX wording, then
 * AI workflow text for `ai_` keys, otherwise native errors, shell text and the common table. */
export function appText(locale: Locale, key: string) {
  return (
    (key.startsWith('licenses_') || key === 'syncLicenses'
      ? licenseText(locale, key)
      : undefined) ||
    resetErrorText(locale, key) ||
    (key.startsWith('UNITY_') || key.startsWith('unity_') ? unityError(locale, key) : undefined) ||
    uxText(locale, key) ||
    (key.startsWith('ai_') || key === 'AI_INPUT_STALE'
      ? nativeWorkflowText(locale, key)
      : nativeErrorText(locale, key) || shellText(locale, key) || t(locale, key))
  );
}
