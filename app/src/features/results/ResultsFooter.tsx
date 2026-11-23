import { CheckCircle2, Columns2, Download, Grid2X2, List, X } from 'lucide-react';
import type { CSSProperties } from 'react';
import { thumbnailSizes } from '../../model';
import Button from '../../shared/Button';
import BatchActions from '../products/BatchActions';
import type { SelectedProducts } from '../selection/useSelectedProducts';
import type { Library } from '../library/useLibrary';
import type { ProductEdits } from '../products/useProductEdits';
import type { Navigation } from '../shell/useNavigation';
import type { ProductActions } from '../products/useProductActions';
import type { Notices } from '../shell/useNotice';
import type { UiPreferences } from '../preferences/useUiPreferences';
import type { Selection } from '../selection/useSelection';
import type { Results } from './useResults';

/** Below the results: the selection dock, and the thumbnail size, density, columns and Download all. */
export default function ResultsFooter({
  selectedProducts,
  library,
  edits,
  nav,
  actions,
  notices,
  uiPrefs,
  selection,
  results,
  q,
}: {
  selectedProducts: SelectedProducts;
  library: Library;
  edits: ProductEdits;
  nav: Navigation;
  actions: ProductActions;
  notices: Notices;
  uiPrefs: UiPreferences;
  selection: Selection;
  results: Results;
  q: (key: string) => string;
}) {
  const { selectedSet } = selectedProducts;
  const { batchEnabled } = library;
  const { setProductConfirm } = actions;
  const { locale, thumb, setThumb, density, setDensity, columns, setColumns } = uiPrefs;
  const { selected, setSelected, addToSelection } = selection;
  const { matchedIds, queryBusy, queryResult, queryKey } = results;
  return (
    <footer className="result-footer">
      {selected.length > 0 && (
        <div
          className="selection-actions"
          onKeyUpCapture={(e) => {
            const row = e.currentTarget,
              b = (e.target as HTMLElement).getBoundingClientRect(),
              r = row.getBoundingClientRect();
            if (b.right > r.right) row.scrollLeft += b.right - r.right + 2;
            else if (b.left < r.left) row.scrollLeft -= r.left - b.left + 2;
          }}
          onFocusCapture={(e) => {
            const row = e.currentTarget,
              target = e.target as HTMLElement;
            requestAnimationFrame(() => {
              const b = target.getBoundingClientRect(),
                r = row.getBoundingClientRect();
              if (b.right > r.right) row.scrollLeft += b.right - r.right + 2;
              else if (b.left < r.left) row.scrollLeft -= r.left - b.left + 2;
            });
          }}
        >
          <div className="selection-group">
            <Button title={q('clearSelection')} onClick={() => setSelected([])}>
              <X size={13} />
            </Button>
            <strong className="selection-count">
              <b key={selected.length}>{selected.length.toLocaleString(locale)}</b>
              <span>{q('selected')}</span>
            </strong>
            {batchEnabled.selectAll && (
              <Button
                title={q('selectAll')}
                onClick={() => addToSelection(matchedIds)}
                disabled={
                  !matchedIds.length || queryBusy || matchedIds.every((id) => selectedSet.has(id))
                }
              >
                <CheckCircle2 size={14} />
                <span>{q('selectAll')}</span>
              </Button>
            )}
          </div>
          <BatchActions
            ids={selected}
            selectedProducts={selectedProducts}
            library={library}
            edits={edits}
            nav={nav}
            actions={actions}
            notices={notices}
            uiPrefs={uiPrefs}
            q={q}
          />
        </div>
      )}
      <div className="result-controls">
        <label className="thumbnail-control" title={q('thumbnail')}>
          <List size={14} />
          <input
            aria-label={q('thumbnail')}
            type="range"
            min={0}
            max={thumbnailSizes.length - 1}
            step={1}
            value={thumbnailSizes.indexOf(thumb as (typeof thumbnailSizes)[number])}
            style={
              {
                '--fill': `${(Math.max(0, thumbnailSizes.indexOf(thumb as (typeof thumbnailSizes)[number])) / (thumbnailSizes.length - 1)) * 100}%`,
              } as CSSProperties
            }
            aria-valuetext={thumb === 0 ? q('listView') : `${q('thumbnail')} ${thumb}px`}
            onChange={(e) => setThumb(thumbnailSizes[+e.target.value])}
          />
          <Grid2X2 size={14} />
        </label>
        <select
          aria-label={q('density')}
          title={q(density === 40 ? 'compact' : density === 74 ? 'standard' : 'comfortable')}
          value={density}
          onChange={(e) => setDensity(+e.target.value)}
        >
          <option value={40}>{q('compact')}</option>
          <option value={74}>{q('standard')}</option>
          <option value={90}>{q('comfortable')}</option>
        </select>
        <details
          className="column-menu"
          onKeyDown={(e) => {
            // Escape closes the open menu and keeps the selection.
            if (e.key === 'Escape' && e.currentTarget.open) {
              e.stopPropagation();
              e.currentTarget.open = false;
              e.currentTarget.querySelector('summary')?.focus();
            }
          }}
        >
          <summary aria-label={q('columns')} title={q('columns')}>
            <Columns2 size={14} />
            <span>{q('columns')}</span>
          </summary>
          <div className="popup-menu">
            {['shop', 'category', 'files', 'acquired', 'price'].map((column) => (
              <label className="check-row" key={column}>
                <input
                  type="checkbox"
                  checked={columns.includes(column)}
                  onChange={(e) =>
                    setColumns(
                      e.target.checked
                        ? [...columns, column]
                        : columns.filter((key) => key !== column),
                    )
                  }
                />
                {q(column)}
              </label>
            ))}
          </div>
        </details>
        {queryResult?.key === queryKey && queryResult.allDownloaded !== true && (
          <Button
            primary
            title={q('downloadAll')}
            disabled={!matchedIds.length || queryBusy}
            onClick={() => setProductConfirm({ kind: 'downloadAll', ids: matchedIds, names: [] })}
          >
            <Download size={14} />
            <span>{q('downloadAll')}</span>
          </Button>
        )}
      </div>
    </footer>
  );
}
