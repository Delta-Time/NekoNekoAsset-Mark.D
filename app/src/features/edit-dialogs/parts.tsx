import { type Locale, normalizeSearch, getAvatarAliases } from '../../model';
import { table } from '../../messages';
import type { ReactNode } from 'react';
import { resolveTag } from '../../TagPicker';

/** The message for a failed native operation. */
export const operationMessage = (locale: Locale, code: string) =>
  table(locale, 'editDialog')[
    code === 'nativeUnavailable' ? 'nativeUnavailable' : 'operationFailed'
  ];
/** The trimmed URL when it is http(s) with a host and no credentials, '' when empty, else null. */
export function normalizeProductUrl(value: string): string | null {
  const trimmed = value.trim();
  if (!trimmed) return '';
  if (!/^https?:\/\//i.test(trimmed) || /[\u0000-\u0020\u007f]/.test(trimmed)) return null;
  try {
    const url = new URL(trimmed);
    return url.hostname &&
      !url.username &&
      !url.password &&
      ['http:', 'https:'].includes(url.protocol)
      ? trimmed
      : null;
  } catch {
    return null;
  }
}
export function Field({ label, children }: { label: string; children: ReactNode }) {
  return (
    <label className="field">
      <span>{label}</span>
      {children}
    </label>
  );
}
export function equal(a: unknown, b: unknown) {
  return JSON.stringify(a) === JSON.stringify(b);
}
/** The tags plus the one still typed in the picker, unless it is already there. */
export const withPendingTag = (
  tags: string[],
  pending: string,
  suggestions: string[],
  aliases: Record<string, readonly string[]> = {},
) => {
  const tag = resolveTag(pending, suggestions, aliases);
  return tag && !tags.some((value) => normalizeSearch(value) === normalizeSearch(tag))
    ? [...tags, tag]
    : tags;
};
/** Whether two tags name the same avatar under any alias. */
export const sameTag = (left: string, right: string) => {
  const aliases = new Set(getAvatarAliases(left).map(normalizeSearch));
  return getAvatarAliases(right).some((alias) => aliases.has(normalizeSearch(alias)));
};
/** The controls Tab moves between inside a dialog. */
export const focusable = (element: HTMLDivElement | null) =>
  Array.from(
    element?.querySelectorAll<HTMLElement>(
      'button:not(:disabled),input:not(:disabled):not([type=hidden]),select:not(:disabled),textarea:not(:disabled),[tabindex="0"]',
    ) ?? [],
  ).filter((el) => el.offsetParent !== null && !el.closest('[inert]'));
export const localFileName = (path: string) =>
  path
    .replace(/[\\/]+$/, '')
    .split(/[\\/]/)
    .pop() || path;
