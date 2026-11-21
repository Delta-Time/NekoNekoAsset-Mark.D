import { type Product, isFileDownloaded, type Locale, type Account } from '../../model';
import { wf } from '../../workflow-i18n';

function isZip(file: { name: string; type: string }): boolean {
  const extension = file.name
    .trim()
    .match(/\.([^./\\]+)$/)?.[1]
    .toLowerCase();
  return extension !== undefined
    ? extension === 'zip'
    : ['zip', 'application/zip'].includes(file.type.trim().toLowerCase());
}
/** Only checks retained original ZIP facts; extracted-file actions use native availability. */
export function canOpenZip(file: Product['files'][number]): boolean {
  return (
    isZip(file) &&
    isFileDownloaded(file) &&
    file.originalAvailable === true &&
    !!file.verifiedHash &&
    !!file.rootId &&
    !!file.relativePath
  );
}
export function canOpenProductPage(
  product: Pick<Product, 'sourceUrl' | 'sourceUrlAvailable'>,
): boolean {
  if (product.sourceUrlAvailable === false || !product.sourceUrl) return false;
  const value = product.sourceUrl.trim();
  if (!/^https?:\/\//i.test(value) || /[\u0000-\u0020\u007f]/.test(value)) return false;
  try {
    const url = new URL(value);
    return (
      !!url.hostname && !url.username && !url.password && ['http:', 'https:'].includes(url.protocol)
    );
  } catch {
    return false;
  }
}
export function sourceLabel(locale: Locale, value: string | undefined | null): string {
  const clean = value?.trim();
  if (!clean || /^(unknown|null|undefined)$/i.test(clean)) return '';
  const keys: Record<string, string> = {
    library: 'library',
    purchase: 'orders',
    purchases: 'orders',
    purchased: 'orders',
    order: 'orders',
    orders: 'orders',
    gift: 'gifts',
    gifts: 'gifts',
    free: 'free',
    local: 'localFiles',
    'local files': 'localFiles',
    'offline files': 'localFiles',
  };
  const key = keys[clean.toLowerCase()];
  return key ? wf(locale, key) : clean.toLowerCase() === 'booth' ? 'BOOTH' : clean;
}
export function provenanceLines(
  locale: Locale,
  rows: readonly {
    source?: string;
    accountId?: string;
    acquisitionKind?: string | null;
    acquiredDateText?: string | null;
    variantName?: string | null;
  }[],
  accounts: readonly Account[],
): string[] {
  return [
    ...new Set(
      rows
        .map((row) => {
          const account = accounts.find((account) => account.id === row.accountId);
          return [
            ...new Set(
              [
                account?.store,
                account?.name,
                sourceLabel(locale, row.source),
                sourceLabel(locale, row.acquisitionKind),
                row.acquiredDateText,
                row.variantName,
              ]
                .map((value) => value?.trim())
                .filter(
                  (value): value is string => !!value && !/^(unknown|null|undefined)$/i.test(value),
                ),
            ),
          ].join(' · ');
        })
        .filter(Boolean),
    ),
  ];
}
/** Bytes on disk, or a partly removed file that a retry can finish deleting. */
export const hasLocalCopy = (file: Product['files'][number]) =>
  isFileDownloaded(file) || file.status === 'missing';
/** A price row of an owned variant: current or paid. */
export type VariantPrice = {
  variantId?: string;
  name: string;
  price: number | null;
  currency: string | null;
  accountId?: string;
};
