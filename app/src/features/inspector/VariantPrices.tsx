import { priceLabel } from '../../model';
import type { Locale } from '../../model';
import type { VariantPrice } from './inspector-facts';
import type { InspectorFacts } from './useInspectorFacts';

/** Prices per owned variant, or one fallback text when no variant has a row. */
export default function VariantPrices({
  rows,
  fallback,
  missing,
  facts,
  locale,
}: {
  rows: readonly VariantPrice[];
  fallback: string | null;
  missing?: (row: VariantPrice) => string;
  facts: InspectorFacts;
  locale: Locale;
}) {
  const { L } = facts;
  return rows.length ? (
    <span className="asset-inspector-prices">
      {rows.map((row, index) => (
        <span key={index}>
          {row.name && <strong>{row.name} · </strong>}
          {priceLabel(locale, row.price, row.currency) || (missing ? missing(row) : L('unknown'))}
        </span>
      ))}
    </span>
  ) : (
    <span>{fallback || L('unknown')}</span>
  );
}
