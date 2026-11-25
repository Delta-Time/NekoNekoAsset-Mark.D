import {
  getEffectiveAvatarTags,
  getProductDisplayStatus,
  giftedVariant,
  onlyGifts,
  priceLabel,
  sourceCategoryLabel,
} from '../../model';
import type { Account, Locale, Product } from '../../model';
import { t } from '../../i18n';
import { shellText } from '../../shell-i18n';
import { canOpenProductPage, provenanceLines, sourceLabel } from './inspector-facts';
import type { VariantPrice } from './inspector-facts';

/** What the inspector shows about the product: status, owned avatars, the image unless R-18
 * hides it, where it came from, and the current and paid prices in the account scope. */
export function useInspectorFacts({
  p,
  locale,
  accountIds,
  accounts,
  sensitiveMode,
  revealed,
  onOpenProductPage,
}: {
  p: Product;
  locale: Locale;
  accountIds: string[] | null;
  accounts: Account[];
  sensitiveMode: string;
  revealed: boolean;
  onOpenProductPage: ((url: string) => void) | undefined;
}) {
  const L = (key: string) => shellText(locale, key) ?? t(locale, key);
  const status = getProductDisplayStatus(p),
    owned = getEffectiveAvatarTags(p, accountIds),
    image = p.thumbnailHidden ? undefined : (p.thumbnailOverrideUrl ?? p.previewUrl);
  const blocked = p.sensitive && sensitiveMode === 'blur' && !revealed;
  const pageAvailable = canOpenProductPage(p) && !!onOpenProductPage;
  const sourceAccounts = [
    ...accounts,
    ...(p.sources ?? [])
      .filter(
        (source) =>
          source.accountId &&
          source.accountName &&
          !accounts.some((account) => account.id === source.accountId),
      )
      .map((source) => ({
        id: source.accountId!,
        name: source.accountName!,
        store: source.provider,
        status: '',
      })),
  ];
  const sourceSummaries = (p.sources ?? [])
    .map((source) => {
      const account = sourceAccounts.find((account) => account.id === source.accountId);
      return (
        sourceLabel(locale, source.label) ||
        [...new Set([source.provider, source.accountName ?? account?.name].filter(Boolean))].join(
          ' · ',
        )
      );
    })
    .filter(Boolean);
  const productSources = [
    ...new Set([
      ...sourceSummaries,
      ...provenanceLines(locale, p.provenance ?? [], sourceAccounts),
    ]),
  ];
  const sourceCategory = p.sourceCategory ? sourceCategoryLabel(p.sourceCategory) : '';
  const accountSources = productSources.length
    ? []
    : accounts
        .filter((account) => p.accountIds?.includes(account.id))
        .map((account) => [account.store, account.name].filter(Boolean).join(' · '));
  const openPage = () => {
    if (canOpenProductPage(p)) onOpenProductPage?.(p.sourceUrl!.trim());
  };
  const currentPrices = (p.ownedCurrentPrices ?? []).filter(
    (row) => accountIds === null || row.accountIds.some((id) => accountIds.includes(id)),
  );
  const paidPrices = (p.ownedPaidPrices ?? []).filter(
    (row) => accountIds === null || (!!row.accountId && accountIds.includes(row.accountId)),
  );
  const provenanceInScope = (p.provenance ?? []).filter(
    (row) => accountIds === null || (!!row.accountId && accountIds.includes(row.accountId)),
  );
  const paidFallback =
    p.provider === 'local'
      ? L('localProductShop')
      : priceLabel(locale, p.paidPrice, p.paidCurrency) ||
        (onlyGifts(provenanceInScope) ? L('paidGift') : null);
  const unpaidRow = (row: VariantPrice) =>
    giftedVariant(provenanceInScope, row.accountId, row.variantId) ? L('paidGift') : L('unknown');
  return {
    L,
    status,
    owned,
    image,
    blocked,
    pageAvailable,
    sourceAccounts,
    productSources,
    sourceCategory,
    accountSources,
    openPage,
    currentPrices,
    paidPrices,
    paidFallback,
    unpaidRow,
  };
}
export type InspectorFacts = ReturnType<typeof useInspectorFacts>;
