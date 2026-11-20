import type { CSSProperties } from 'react';
import Preview from '../../Preview';
import { contentLang } from '../../content-lang';
import ProductStatus from '../results/ProductStatus';
import BatchActions from '../products/BatchActions';
import type { SelectedProducts } from '../selection/useSelectedProducts';
import type { Library } from '../library/useLibrary';
import type { ProductEdits } from '../products/useProductEdits';
import type { Navigation } from '../shell/useNavigation';
import type { ProductActions } from '../products/useProductActions';
import type { Notices } from '../shell/useNotice';
import type { UiPreferences } from '../preferences/useUiPreferences';
import type { Results } from '../results/useResults';
import type { Filters } from '../filters/useFilters';
import type { Selection } from '../selection/useSelection';

/** The details pane for several selected products: their thumbnails, count, actions and names. */
export default function BatchSummary({
  selectedProducts,
  library,
  edits,
  nav,
  actions,
  notices,
  uiPrefs,
  results,
  filters,
  selection,
  q,
}: {
  selectedProducts: SelectedProducts;
  library: Library;
  edits: ProductEdits;
  nav: Navigation;
  actions: ProductActions;
  notices: Notices;
  uiPrefs: UiPreferences;
  results: Results;
  filters: Filters;
  selection: Selection;
  q: (key: string) => string;
}) {
  const { selectedSet, visibleSelected } = selectedProducts;
  const { locale } = uiPrefs;
  const { products } = results;
  const { query } = filters;
  const { revealed } = selection;
  return (
    <div className="revised-batch">
      <div className="batch-stack" aria-hidden="true">
        {products
          .filter((p) => selectedSet.has(p.id))
          .slice(0, 5)
          .map((p, k) => (
            <span
              key={p.id}
              className={
                p.sensitive && query.sensitive === 'blur' && !revealed.has(p.id)
                  ? 'blurred'
                  : undefined
              }
              style={{ '--k': k } as CSSProperties}
            >
              <Preview
                src={p.thumbnailHidden ? undefined : p.thumbnailOverrideUrl || p.previewUrl}
                title=""
                className="asset-art"
              />
            </span>
          ))}
      </div>
      <h2 className="batch-count">
        <b>{visibleSelected.length.toLocaleString(locale)}</b>
        <span>{q('selected')}</span>
      </h2>
      <BatchActions
        ids={visibleSelected}
        selectedProducts={selectedProducts}
        library={library}
        edits={edits}
        nav={nav}
        actions={actions}
        notices={notices}
        uiPrefs={uiPrefs}
        q={q}
      />
      <div className="selected-products">
        {products
          .filter((p) => selectedSet.has(p.id))
          .slice(0, 12)
          .map((p) => (
            <div key={p.id} title={p.title}>
              <span lang={contentLang(p.title)}>{p.title}</span>
              <ProductStatus p={p} q={q} />
            </div>
          ))}
      </div>
    </div>
  );
}
