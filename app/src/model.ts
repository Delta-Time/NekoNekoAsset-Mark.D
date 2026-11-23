import { t } from './i18n';
import type {
  AcquisitionProvenance,
  AssetFile,
  CustomCategory,
  Product as NativeProduct,
  SourceCategoryValue,
} from './bindings';
export type { Account, AssetFile, CustomCategory } from './bindings';
/** A product as the native catalogue sends it, with a session-only thumbnail. */
export type Product = NativeProduct & { thumbnailOverrideUrl?: string };
export type Locale = 'en' | 'ja' | 'ko' | 'zh-Hans' | 'zh-Hant';
/** Native item-line evidence supplies paid amount/currency; listing price never supplies paid facts. */
export function priceLabel(
  locale: Locale,
  value: number | null | undefined,
  currency: string | null | undefined,
): string | null {
  if (typeof value !== 'number' || !Number.isFinite(value) || value < 0 || !currency) return null;
  if (currency === 'JPY')
    return Number.isSafeInteger(value) ? `¥${value.toLocaleString(locale)}` : null;
  if (!Intl.supportedValuesOf('currency').includes(currency)) return null;
  return new Intl.NumberFormat(locale, { style: 'currency', currency }).format(value);
}
function startingPriceLabel(
  locale: Locale,
  product: Pick<Product, 'provider' | 'listingStartingPrice'>,
): string | null {
  if (product.provider !== 'booth') return null;
  const amount = priceLabel(locale, product.listingStartingPrice, 'JPY');
  return amount ? `${t(locale, 'startingPrice')} ${amount}` : null;
}
/** An owned quote takes priority; BOOTH's starting price is explicitly labelled when no owned quote is available. */
export function currentPriceLabel(
  locale: Locale,
  product: Pick<
    Product,
    | 'provider'
    | 'price'
    | 'priceCurrency'
    | 'currentPriceText'
    | 'ownedCurrentPrices'
    | 'listingStartingPrice'
  >,
): string | null {
  if (product.provider === 'booth') {
    const rows = product.ownedCurrentPrices ?? [];
    if (rows.some((row) => priceLabel(locale, row.price, row.currency)))
      return rows
        .map((row) =>
          [row.name, priceLabel(locale, row.price, row.currency) || '—']
            .filter(Boolean)
            .join(' · '),
        )
        .join(' / ');
    return startingPriceLabel(locale, product);
  }
  if (product.ownedCurrentPrices?.length)
    return product.ownedCurrentPrices
      .map((row) =>
        [row.name, priceLabel(locale, row.price, row.currency) || '—'].filter(Boolean).join(' · '),
      )
      .join(' / ');
  return (
    priceLabel(locale, product.price, product.priceCurrency) ||
    product.currentPriceText?.trim() ||
    null
  );
}
/** Acquisition rows that are all gifts prove no payment of the holder's own. */
export function onlyGifts(rows: readonly Pick<AcquisitionProvenance, 'source'>[]): boolean {
  return rows.length > 0 && rows.every((row) => row.source === 'gift');
}
/** An unpriced variant is a gift when a gift bound it, or when its account holds the product only through gifts. */
export function giftedVariant(
  rows: readonly Pick<AcquisitionProvenance, 'source' | 'accountId' | 'variantId' | 'variantIds'>[],
  accountId: string | undefined,
  variantId: string | undefined,
): boolean {
  const own = rows.filter((row) => row.accountId === accountId);
  return (
    onlyGifts(own) ||
    own.some(
      (row) =>
        row.source === 'gift' &&
        !!variantId &&
        (row.variantId === variantId || !!row.variantIds?.includes(variantId)),
    )
  );
}
/** Compact cards omit variant names while keeping starting prices distinct from owned quotes. */
export function cardPriceLabel(
  locale: Locale,
  product: Pick<
    Product,
    'provider' | 'price' | 'priceCurrency' | 'ownedCurrentPrices' | 'listingStartingPrice'
  >,
): string | null {
  const rows = product.ownedCurrentPrices?.length
    ? product.ownedCurrentPrices
    : [{ price: product.price, currency: product.priceCurrency }];
  const labels = rows.map((row) => {
    if (!priceLabel(locale, row.price, row.currency)) return null;
    return new Intl.NumberFormat(locale, {
      style: 'currency',
      currency: row.currency!,
      minimumFractionDigits: 0,
    }).format(row.price!);
  });
  return labels.some(Boolean)
    ? [...new Set(labels.map((label) => label || '—'))].join(' / ')
    : startingPriceLabel(locale, product);
}
/** Known descriptors remain visible while unverified membership has an explicit +. */
export function fileCountLabel(
  product: Pick<Product, 'manifestComplete' | 'downloaded' | 'total' | 'knownFileCount' | 'files'>,
  ratio = true,
): string {
  const count = product.knownFileCount ?? product.files.length;
  if (product.manifestComplete === true)
    return ratio ? `${product.downloaded}/${product.total}` : String(product.total);
  if (!count) return '—';
  return `${ratio ? product.downloaded + '/' : ''}${count}+`;
}
export function isFileDownloaded(file: AssetFile): boolean {
  return ['ready', 'downloaded', 'extracting', 'extractionFailed'].includes(file.status);
}
export type SortDirection = 'asc' | 'desc';
export function defaultSortDirection(key: string): SortDirection {
  return key === 'acquired' ? 'desc' : 'asc';
}
export const thumbnailSizes = [0, 96, 112, 128, 160, 192, 224, 272, 320] as const;
/** The fitted width drives both rendered cards and virtual row geometry. */
export function resultGeometry(
  width: number,
  size: number,
  density: number,
  metadataCount?: number,
) {
  const available = Math.max(1, width - 24),
    list = size === 0;
  const gap = density === 40 ? 8 : density === 74 ? 13 : 16;
  const columns = list ? 1 : Math.max(1, Math.floor((available + gap) / (size + gap)));
  const actual = list ? 0 : (available - gap * (columns - 1)) / columns;
  const info =
    metadataCount === undefined
      ? density === 40
        ? 88
        : density === 74
          ? 96
          : 110
      : 74 + Math.max(0, metadataCount) * 18;
  return { list, gap, columns, actual, info, row: list ? density : actual / 1.15 + info + gap };
}
/** Native controls follow the actual selected/previewed background, including imported themes. */
export function isLightTheme(tokens: Record<string, string>): boolean {
  const hex = tokens['--bg'];
  if (!/^#[a-f0-9]{6}$/i.test(hex ?? '')) return false;
  const channels = [1, 3, 5].map((offset) => {
    const value = parseInt(hex.slice(offset, offset + 2), 16) / 255;
    return value <= 0.04045 ? value / 12.92 : ((value + 0.055) / 1.055) ** 2.4;
  });
  return channels[0] * 0.2126 + channels[1] * 0.7152 + channels[2] * 0.0722 > 0.45;
}
export const categoryKeys = [
  'Base',
  'Clothes',
  'Hair',
  'Accessory',
  'Texture',
  'Motion',
  'Tool',
  'Shader',
  'Gimmick',
  'World',
  'Udon',
  'NonVRChat',
] as const;
export const isCategory = (value: string): value is (typeof categoryKeys)[number] =>
  (categoryKeys as readonly string[]).includes(value);
export function categoryName(
  category: string,
  custom: readonly CustomCategory[],
  label: (key: string) => string,
): string {
  return category.startsWith('custom:')
    ? (custom.find((entry) => entry.id === category)?.name ?? label('unknown'))
    : label(category);
}
/** A bare category name, as some records keep it, has no label. */
export function sourceCategoryLabel(category: SourceCategoryValue): string {
  if (typeof category === 'string') return '';
  return [category.parentName, category.name].filter((value) => value?.trim()).join(' / ');
}
export const statusKeys = ['notDownloaded', 'partial', 'processing', 'ready', 'attention'] as const;
export type ProductDisplayStatus = (typeof statusKeys)[number] | 'unknown';
export type SensitiveMode = 'show' | 'blur' | 'hide' | 'only';
/** Product badges are derived from file integrity, without replacing detailed file states. */
export function getProductDisplayStatus(product: Product): ProductDisplayStatus {
  if (product.filesLoaded === false)
    return statusKeys.includes(product.status as (typeof statusKeys)[number])
      ? (product.status as ProductDisplayStatus)
      : product.downloaded > 0
        ? 'partial'
        : 'notDownloaded';
  // Native projections decide actionable failures; a remaining byte ratio is not an error.
  if (typeof product.fullyDownloaded === 'boolean' && product.status !== 'ready')
    return statusKeys.includes(product.status as (typeof statusKeys)[number])
      ? (product.status as ProductDisplayStatus)
      : product.downloaded > 0
        ? 'partial'
        : 'notDownloaded';
  const summary = summarizeFiles(product.files, product.manifestComplete);
  const states = [product.status, ...product.files.map((file) => file.status)];
  if (
    states.some((state) =>
      ['queued', 'processing', 'downloading', 'transferring', 'extracting'].includes(state),
    )
  )
    return 'processing';
  if (states.some((state) => ['failed', 'extractionFailed', 'missing'].includes(state)))
    return 'attention';
  if (summary.usable) return 'ready';
  if (product.status === 'attention') return 'attention';
  if (summary.downloaded > 0) return 'partial';
  return 'notDownloaded';
}
export function getEffectiveAvatarTags(
  product: Product,
  accountIds: readonly string[] | null = null,
): string[] {
  if (product.provider === 'local') accountIds = null;
  if (accountIds?.length === 0) return [];
  return [...new Set(product.ownedAvatars)].filter((avatar) => {
    if (accountIds === null) return true;
    const evidence = (product.ownedAvatarEvidence ?? []).filter((item) => item.avatar === avatar);
    // An explicit compatibility correction has no account restriction. Raw account evidence does.
    return (
      !evidence.length ||
      evidence.some(
        (item) =>
          ['user', 'sourceTag', 'userTag'].includes(item.source) ||
          (item.accountId != null && accountIds.includes(item.accountId)),
      )
    );
  });
}
// Original, small alias table from human-provided examples and authored fixtures.
// This resolves exact tag spellings; it does not guess product identity or ownership.
const avatarAliasGroups: Record<string, readonly string[]> = {
  '真冬 / Mafuyu': ['真冬', 'まふゆ', 'Mafuyu', 'Mahuyu', '真冬用', '真冬對應', '真冬对应'],
  'マヌカ / Manuka': ['マヌカ', 'Manuka', 'マヌカ用', 'マヌカ対応'],
  'まめひなた / Mamehinata': ['まめひなた', 'Mamehinata', 'まめひなた用', 'まめひなた対応'],
  '紙羽 / Kamihane': ['紙羽', 'Kamihane'],
  'こいし / Koishi Study': ['こいし', 'Koishi Study'],
  'ルーム / Loom': ['ルーム', 'Loom'],
  'しずく / Shizuku Study': ['しずく', 'Shizuku Study'],
};
const avatarAliasIndex = new Map(
  Object.entries(avatarAliasGroups).flatMap(([canonical, aliases]) =>
    [canonical, ...aliases].map((alias) => [normalizeSearch(alias), canonical] as const),
  ),
);
export function getAvatarAliases(avatar: string): string[] {
  const canonical = avatarAliasIndex.get(normalizeSearch(avatar));
  return [
    ...new Set(
      [
        avatar,
        ...(canonical ? [canonical, ...avatarAliasGroups[canonical]] : avatar.split(/\s*\/\s*/)),
      ]
        .map((value) => value.trim())
        .filter(Boolean),
    ),
  ];
}
const avatarAliases = (avatar: string): string[] => getAvatarAliases(avatar).map(normalizeSearch);
/** Listing-only avatar tags stay raw evidence, and do not become an owned-content filter. */
export function getEffectiveTags(product: Product): string[] {
  const offered = new Set(product.listingAvatars.flatMap(avatarAliases));
  const owned = new Set(getEffectiveAvatarTags(product).flatMap(avatarAliases));
  const rejected = new Set((product.rejectedTags ?? []).flatMap(avatarAliases));
  return product.tags.filter(
    (tag) =>
      !rejected.has(normalizeSearch(tag)) &&
      (!offered.has(normalizeSearch(tag)) || owned.has(normalizeSearch(tag))),
  );
}
interface FileSummary {
  downloaded: number;
  total: number;
  status: string;
  usable: boolean;
}
/** This shortcut checks complete byte evidence, independently of extraction readiness. */
export function isProductDownloaded(product: Product): boolean {
  if (product.filesLoaded === false) return product.fullyDownloaded === true;
  return product.files.length > 0 && product.files.every(isFileDownloaded);
}
/** Bytes present is distinct from validated, usable output. Unknown is never ready. */
export function summarizeFiles(
  files: readonly AssetFile[],
  _manifestComplete = false,
): FileSummary {
  const total = files.length;
  const downloaded = files.filter((f) =>
    ['ready', 'downloaded', 'extracting', 'extractionFailed'].includes(f.status),
  ).length;
  let status = 'notDownloaded';
  if (!total || files.some((f) => f.status === 'unknown')) status = 'unknown';
  else if (files.every((f) => f.status === 'ready')) status = 'ready';
  else if (files.some((f) => f.status === 'extractionFailed')) status = 'attention';
  else if (files.some((f) => f.status === 'extracting')) status = 'extracting';
  else if (files.some((f) => f.status === 'failed' || f.status === 'missing')) status = 'attention';
  else if (downloaded > 0 && downloaded < total) status = 'partial';
  else if (files.some((f) => f.status === 'paused')) status = 'paused';
  else if (files.some((f) => f.status === 'queued')) status = 'queued';
  else if (downloaded === total) status = 'extracting';
  return { downloaded, total, status, usable: status === 'ready' };
}
export function normalizeSearch(value: string): string {
  return value.normalize('NFKC').toLocaleLowerCase().trim();
}
