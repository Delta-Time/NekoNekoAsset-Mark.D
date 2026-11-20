import { Download, FileBox, MoreHorizontal, SlidersHorizontal, X } from 'lucide-react';
import { getProductDisplayStatus, isProductDownloaded } from '../../model';
import type { Product } from '../../model';
import Button from '../../shared/Button';
import type { ProductActions } from './useProductActions';
import type { Navigation } from '../shell/useNavigation';
import type { ProductEdits } from './useProductEdits';
import type { Library } from '../library/useLibrary';
import type { SelectedProducts } from '../selection/useSelectedProducts';
import type { Notices } from '../shell/useNotice';

/** The details pane's actions for one product: download, cancel or import, Edit, and the More menu. */
export default function SingleActions({
  p,
  actions,
  nav,
  edits,
  library,
  selectedProducts,
  notices,
  q,
}: {
  p: Product;
  actions: ProductActions;
  nav: Navigation;
  edits: ProductEdits;
  library: Library;
  selectedProducts: SelectedProducts;
  notices: Notices;
  q: (key: string) => string;
}) {
  const {
    activeDownloads,
    allUnavailable,
    cancelDownloads,
    download,
    needsAttention,
    hasDownloadedFiles,
    update,
    syncLicenses,
    requestDeleteDownloads,
    dismissAttention,
    exportZip,
  } = actions;
  const { openUnity, setFlow } = nav;
  const { edit, onlineSelection, requestLocalDelete, hideProducts } = edits;
  const { licenseEnabled, aiEnabled } = library;
  const { chosen } = selectedProducts;
  const { run } = notices;
  const ready = getProductDisplayStatus(p) === 'ready' && isProductDownloaded(p),
    downloads = activeDownloads(p.id);
  return (
    <div className="detail-actions">
      {!downloads.length && allUnavailable([p.id]) ? (
        <Button className="unavailable-action" title={q('downloadUnavailable')} disabled>
          <span>{q('downloadUnavailable')}</span>
        </Button>
      ) : downloads.length ? (
        <Button
          className="cancel-download"
          title={q('cancelDownload')}
          disabled={downloads.every((job) => job.cancelRequested)}
          onClick={() => cancelDownloads([p.id])}
        >
          <X size={14} />
          <span>{q('cancel')}</span>
        </Button>
      ) : (
        <Button
          primary={!ready}
          className={ready ? 'unity-action' : ''}
          title={q(ready ? 'importUnity' : 'download')}
          onClick={() => (ready ? openUnity([p.id]) : download([p.id]))}
        >
          {ready ? <FileBox size={14} /> : <Download size={14} />}
          <span>{q(ready ? 'importUnity' : 'download')}</span>
        </Button>
      )}
      <Button onClick={edit}>
        <SlidersHorizontal size={14} />
        {q('edit')}
      </Button>
      <details className="action-menu">
        <summary aria-label={q('more')}>
          <MoreHorizontal size={16} />
        </summary>
        <div className="popup-menu">
          {[
            'update',
            ...(licenseEnabled ? ['syncLicenses'] : []),
            'download',
            ...(chosen.some((id) => activeDownloads(id).length) ? ['cancelDownload'] : []),
            ...(aiEnabled ? ['ai'] : []),
            'importUnity',
            'exportZip',
            ...(needsAttention(chosen) ? ['dismissAttention'] : []),
            ...(p.provider === 'local'
              ? ['delete']
              : [
                  p.hidden ? 'unhide' : 'hide',
                  ...(onlineSelection(chosen) && hasDownloadedFiles(chosen)
                    ? ['deleteDownloads']
                    : []),
                ]),
          ].map((key) => (
            <button
              key={key}
              className={key === 'delete' || key === 'deleteDownloads' ? 'danger' : undefined}
              disabled={key === 'download' && allUnavailable(chosen)}
              onClick={(e) => {
                e.currentTarget.closest('details')?.removeAttribute('open');
                if (key === 'delete') requestLocalDelete([p.id]);
                else if (key === 'hide' || key === 'unhide')
                  run(hideProducts([p.id], key === 'hide'));
                else if (key === 'update') update(chosen);
                else if (key === 'syncLicenses') syncLicenses(chosen);
                else if (key === 'download') download(chosen);
                else if (key === 'cancelDownload') cancelDownloads(chosen);
                else if (key === 'deleteDownloads') requestDeleteDownloads(chosen);
                else if (key === 'dismissAttention') dismissAttention(chosen);
                else if (key === 'ai') setFlow('ai');
                else if (key === 'importUnity') openUnity(chosen);
                else exportZip(chosen);
              }}
            >
              {q(key)}
            </button>
          ))}
        </div>
      </details>
    </div>
  );
}
