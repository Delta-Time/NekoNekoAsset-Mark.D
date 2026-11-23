import { hasMessage, message } from './messages';
import type { Locale } from './model';

export function nativeErrorText(locale: Locale, code: string): string | undefined {
  const lower = code.toLowerCase();
  const nativeCode = lower.split('.').at(-1) ?? lower;
  const upper = nativeCode.toUpperCase();
  const aliases: Record<string, string> = {
    STORAGE_SOURCE_LOCKED: 'move_file_in_use',
    FILE_IN_USE: 'move_file_in_use',
    SOURCE_UNSUPPORTED: 'COLLECTOR_UNSUPPORTED',
    RECEIPT_BODY_FAILED: 'RECEIPT_REQUEST_FAILED',
    RECEIPT_AUTH_FAILED: 'auth',
    RECEIPT_AUTHORIZATION_REQUIRED: 'auth',
    PROTECTED_SESSION_UNAVAILABLE: 'auth',
    DIGITAL_DETAIL_ROLE_UNKNOWN: 'RECEIPT_PARSE_FAILED',
    GUMROAD_PUBLIC_METADATA_UNAVAILABLE: 'PUBLIC_METADATA_UNAVAILABLE',
    FILE_INCOMPLETE: 'manifest_incomplete',
    DOWNLOAD_LINK_UNAVAILABLE: 'download_resolver_failed',
    DOWNLOAD_RESOLVER_FAILED: 'download_resolver_failed',
  };
  let key = hasMessage('error', code)
    ? code
    : hasMessage('error', upper)
      ? upper
      : hasMessage('error', nativeCode)
        ? nativeCode
        : (aliases[upper] ?? '');
  if (
    !key &&
    ['store_identity_changed', 'store_identity_mismatch', 'account_identity_mismatch'].includes(
      nativeCode,
    )
  )
    key = 'auth';
  if (
    !key &&
    /^(jinxxy|gumroad)_(source_unsupported|scope_changed|pagination_unproven)$/.test(nativeCode)
  )
    key = 'operationFailed';
  if (!key && /^(file_unavailable|downloads?_unavailable)$/.test(nativeCode)) key = 'missing';
  if (!key && ['collector_unsupported', 'unsupported_image'].includes(nativeCode))
    key = 'validation';
  if (!key && ['unsafe_download_url', 'unsafe_redirect', 'redirect_limit'].includes(nativeCode))
    key = 'operationFailed';
  if (!key && /(?:^|[._])(?:auth|login|session)(?:[._]|$)|(?:^|[._])expired(?:[._]|$)/.test(lower))
    key = 'auth';
  if (!key && /offline|root_unavailable|root_offline/.test(lower)) key = 'offline';
  if (!key && /low_space|disk_full|insufficient_space/.test(lower)) key = 'space';
  if (
    !key &&
    /file_missing|file_not_usable|original_unavailable|manifest_incomplete|integrity/.test(lower)
  )
    key = 'missing';
  if (!key && /archive|extract|password|volume/.test(lower)) key = 'archive';
  if (!key && /conflict|undo/.test(lower)) key = 'conflict';
  if (!key && /permission|access_denied/.test(lower)) key = 'permission';
  if (!key && /invalid_|unsafe|newer_schema|version_unsupported/.test(lower)) key = 'validation';
  if (!key && /rate_limit/.test(lower)) key = 'rate';
  if (
    !key &&
    (/^(catalogue|storage|booth|media|native|file)\./.test(lower) ||
      (code === code.toUpperCase() && /[A-Z_]+/.test(code)) ||
      /_/.test(code))
  )
    key = 'operationFailed';
  return message(locale, 'error', key);
}

/** A native error's own text, or Action failed when it names nothing more specific. */
export function nativeErrorOrFailed(locale: Locale, code: string): string {
  return nativeErrorText(locale, code) ?? message(locale, 'error', 'operationFailed') ?? code;
}
