import { LoaderCircle, Search } from 'lucide-react';
import type { Product } from '../../model';
import Button from '../../shared/Button';
import ResultViewport from './ResultViewport';
import type { WorkflowProps } from '../workflows/useWorkflowProps';
import LibraryToolbar from '../filters/LibraryToolbar';
import ProductItem from './ProductItem';
import ResultsFooter from './ResultsFooter';
import InspectorPane from '../inspector/InspectorPane';
import type { Filters } from '../filters/useFilters';
import type { UiPreferences } from '../preferences/useUiPreferences';
import type { Library } from '../library/useLibrary';
import type { Navigation } from '../shell/useNavigation';
import type { Selection } from '../selection/useSelection';
import type { Results } from './useResults';
import type { Categories } from '../categories/useCategories';
import type { Avatars } from '../avatars/useAvatars';
import type { SelectedProducts } from '../selection/useSelectedProducts';
import type { ProductEdits } from '../products/useProductEdits';
import type { ProductActions } from '../products/useProductActions';
import type { Notices } from '../shell/useNotice';
import type { Appearance } from '../appearance/useAppearance';
import type { Layout } from '../shell/useLayout';
import type { PreferenceSync } from '../preferences/usePreferenceSync';
import type { Native } from '../library/useNative';
import type { LocalFiles } from '../local-files/useLocalFiles';

/** The library: toolbar, results or their empty and loading states, footer and details pane. */
export default function LibraryView({
  filters,
  uiPrefs,
  library,
  nav,
  selection,
  results,
  categories,
  avatars,
  selectedProducts,
  edits,
  actions,
  notices,
  appearance,
  layout,
  preferenceSync,
  native,
  localFiles,
  q,
  workflowProps,
}: {
  filters: Filters;
  uiPrefs: UiPreferences;
  library: Library;
  nav: Navigation;
  selection: Selection;
  results: Results;
  categories: Categories;
  avatars: Avatars;
  selectedProducts: SelectedProducts;
  edits: ProductEdits;
  actions: ProductActions;
  notices: Notices;
  appearance: Appearance;
  layout: Layout;
  preferenceSync: PreferenceSync;
  native: Native;
  localFiles: LocalFiles;
  q: (key: string) => string;
  workflowProps: WorkflowProps;
}) {
  const { query, clearFilters } = filters;
  const { thumb, density, cardColumns, oneHandedDrag, locale } = uiPrefs;
  const { loaded } = library;
  const { inspector } = nav;
  const { focusId, setSelected, anchor } = selection;
  const { filtered, scrollState, resultsPending, catalogueTotal, products } = results;
  const { categoryLabel } = categories;
  const { resultKeys, selectedSet } = selectedProducts;
  const { userPatch } = edits;
  const { run, notify } = notices;
  return (
    <div className="result-layout">
      <section
        className="result-workspace"
        onKeyDown={resultKeys}
        onDragOver={(e) => {
          if (e.dataTransfer.types.includes('Files')) {
            e.preventDefault();
            e.dataTransfer.dropEffect = 'copy';
          }
        }}
        onDrop={(e) => e.preventDefault()}
      >
        <LibraryToolbar
          filters={filters}
          uiPrefs={uiPrefs}
          library={library}
          nav={nav}
          selection={selection}
          results={results}
          categories={categories}
          avatars={avatars}
          q={q}
        />
        {filtered.length ? (
          <ResultViewport
            items={filtered}
            size={thumb}
            density={density}
            state={scrollState.current}
            activeId={focusId}
            metadataCount={
              Number(cardColumns.includes('shop')) + Number(cardColumns.includes('acquired'))
            }
            render={(p: Product, list: boolean) => (
              <ProductItem
                key={p.id}
                p={p}
                list={list}
                selectedProducts={selectedProducts}
                selection={selection}
                nav={nav}
                uiPrefs={uiPrefs}
                categories={categories}
                filters={filters}
                q={q}
              />
            )}
            gestures={
              oneHandedDrag
                ? {
                    selected: selectedSet,
                    select: (ids, first) => {
                      setSelected(ids);
                      if (first) anchor.current = first;
                    },
                    drop: (ids, category) =>
                      run(
                        userPatch(ids, { category, categoryOverride: true }).then(() =>
                          notify(
                            q('movedToCategory')
                              .replace('{count}', ids.length.toLocaleString(locale))
                              .replace('{category}', categoryLabel(category)),
                          ),
                        ),
                      ),
                  }
                : null
            }
          />
        ) : resultsPending ? (
          <div className="empty-state results-loading" role="status">
            <LoaderCircle size={25} className="progress-spin" aria-hidden="true" />
            <h2>{q(loaded && query.search.trim() ? 'searching' : 'loading')}</h2>
          </div>
        ) : (
          <div className="empty-state">
            <Search size={25} />
            <h2>{q(!catalogueTotal ? 'emptyLibrary' : 'noResults')}</h2>
            {products.length > 0 && <Button onClick={clearFilters}>{q('clearFilters')}</Button>}
          </div>
        )}
        <ResultsFooter
          selectedProducts={selectedProducts}
          library={library}
          edits={edits}
          nav={nav}
          actions={actions}
          notices={notices}
          uiPrefs={uiPrefs}
          selection={selection}
          results={results}
          q={q}
        />
      </section>
      {inspector && (
        <InspectorPane
          uiPrefs={uiPrefs}
          appearance={appearance}
          nav={nav}
          notices={notices}
          selectedProducts={selectedProducts}
          library={library}
          edits={edits}
          actions={actions}
          results={results}
          filters={filters}
          selection={selection}
          layout={layout}
          preferenceSync={preferenceSync}
          categories={categories}
          native={native}
          localFiles={localFiles}
          q={q}
          workflowProps={workflowProps}
        />
      )}
    </div>
  );
}
