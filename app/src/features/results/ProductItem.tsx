import { Eye } from 'lucide-react';
import { cardPriceLabel, currentPriceLabel, fileCountLabel } from '../../model';
import type { Product } from '../../model';
import Preview from '../../Preview';
import { contentLang } from '../../content-lang';
import ProductStatus from './ProductStatus';
import type { SelectedProducts } from '../selection/useSelectedProducts';
import type { Selection } from '../selection/useSelection';
import type { Navigation } from '../shell/useNavigation';
import type { UiPreferences } from '../preferences/useUiPreferences';
import type { Categories } from '../categories/useCategories';
import type { Filters } from '../filters/useFilters';

/** A product card in the grid, or a row in the list. */
export default function ProductItem({
  p,
  list,
  selectedProducts,
  selection,
  nav,
  uiPrefs,
  categories,
  filters,
  q,
}: {
  p: Product;
  list: boolean;
  selectedProducts: SelectedProducts;
  selection: Selection;
  nav: Navigation;
  uiPrefs: UiPreferences;
  categories: Categories;
  filters: Filters;
  q: (key: string) => string;
}) {
  const { selectedSet, select, visibleSelected, keySelect } = selectedProducts;
  const { setSelected, setFocusId, revealed, setRevealed } = selection;
  const { setContext } = nav;
  const { density, columns, locale } = uiPrefs;
  const { categoryLabel } = categories;
  const { query } = filters;
  const cardArt = (p: Product, compact = false) => (
    <div
      className={`thumb ${p.sensitive && query.sensitive === 'blur' && !revealed.has(p.id) ? 'blurred' : ''}`}
    >
      {!compact && columns.includes('category') && (
        <span className="card-category" title={categoryLabel(p.category)}>
          {categoryLabel(p.category)}
        </span>
      )}
      <Preview
        src={p.thumbnailHidden ? undefined : p.thumbnailOverrideUrl || p.previewUrl}
        title={p.title}
        className="asset-art"
        lazy
      />{' '}
      {p.sensitive && query.sensitive === 'blur' && !revealed.has(p.id) && (
        <button
          type="button"
          className="reveal"
          title={q('revealImage')}
          aria-label={q('showR18')}
          onClick={(e) => {
            if (e.shiftKey || e.ctrlKey || e.metaKey) return;
            e.stopPropagation();
            setRevealed((old) => new Set([...old, p.id]));
          }}
          onKeyDown={(e) => {
            if (e.key === 'Enter' || e.key === ' ') e.stopPropagation();
          }}
        >
          <span className="reveal-label">{compact ? <Eye size={14} /> : q('showR18')}</span>
        </button>
      )}
    </div>
  );
  return (
    <div
      id={'product-' + p.id}
      data-product-id={p.id}
      key={p.id}
      role="option"
      aria-selected={selectedSet.has(p.id)}
      tabIndex={0}
      className={
        (list ? 'product-row' : 'product-card') + ' ' + (selectedSet.has(p.id) ? 'selected' : '')
      }
      onClick={(e) => select(p, e)}
      onContextMenu={(e) => {
        e.preventDefault();
        const ids = selectedSet.has(p.id) ? visibleSelected : [p.id];
        if (!selectedSet.has(p.id)) {
          setSelected(ids);
          setFocusId(p.id);
        }
        setContext({ x: e.clientX, y: e.clientY, ids });
      }}
      onDragStart={(e) => e.preventDefault()}
      onMouseDown={(e) => {
        if (e.shiftKey || e.ctrlKey || e.metaKey) e.preventDefault();
      }}
      onKeyDown={(e) => keySelect(e, p)}
    >
      {(!list || density !== 40) && (
        <div className={list ? 'row-thumb' : ''}>{cardArt(p, list)}</div>
      )}
      <div className="product-info">
        <strong className="product-title" lang={contentLang(p.title)} title={p.title}>
          {p.title}
        </strong>
        {columns.includes('shop') && (!list || density !== 40) && (
          <span
            className="product-shop"
            lang={p.provider === 'local' ? undefined : contentLang(p.shop)}
            title={p.provider === 'local' ? q('localProductShop') : p.shop || q('unknownShop')}
          >
            {p.provider === 'local' ? q('localProductShop') : p.shop || q('unknownShop')}
          </span>
        )}
        {list ? (
          <ProductStatus p={p} q={q} />
        ) : (
          <>
            {columns.includes('acquired') && (
              <span
                className="card-acquired"
                title={q('acquired') + ': ' + (p.acquired || q('unknown'))}
              >
                {p.acquired || q('unknown')}
              </span>
            )}
            <div
              className={
                'card-bottom ' +
                ((columns.includes('price') && cardPriceLabel(locale, p)) ||
                (columns.includes('files') && p.downloaded > 0)
                  ? 'has-metrics'
                  : '')
              }
            >
              <ProductStatus p={p} q={q} />
              <span className="card-metrics">
                {columns.includes('price') && cardPriceLabel(locale, p) && (
                  <span className="card-price" title={cardPriceLabel(locale, p)!}>
                    {cardPriceLabel(locale, p)}
                  </span>
                )}
                {columns.includes('files') && p.downloaded > 0 && (
                  <span className="card-file-count" title={q('downloaded') + ': ' + p.downloaded}>
                    <span aria-hidden="true">●</span> {p.downloaded}
                  </span>
                )}
              </span>
            </div>
          </>
        )}
      </div>
      {list && columns.includes('category') && (
        <span className="list-extra" title={categoryLabel(p.category)}>
          {categoryLabel(p.category)}
        </span>
      )}
      {list && columns.includes('files') && (
        <span className="list-extra file-count">{fileCountLabel(p)}</span>
      )}
      {list && columns.includes('acquired') && (
        <span className="list-extra">{p.acquired || q('unknown')}</span>
      )}
      {list && columns.includes('price') && (
        <span className="list-extra">{currentPriceLabel(locale, p) || '—'}</span>
      )}
      <input
        type="checkbox"
        aria-label={q('select') + ' ' + p.title}
        checked={selectedSet.has(p.id)}
        onClick={(e) => e.stopPropagation()}
        onChange={() => select(p, { ctrlKey: true, metaKey: false, shiftKey: false })}
      />
    </div>
  );
}
