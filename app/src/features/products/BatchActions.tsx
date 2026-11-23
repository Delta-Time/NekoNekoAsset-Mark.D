import {
  Download,
  EyeOff,
  FileArchive,
  FileBox,
  Folder,
  Package,
  RefreshCw,
  Scale,
  SlidersHorizontal,
  Tags,
  Trash2,
} from 'lucide-react';
import { getProductDisplayStatus, isProductDownloaded } from '../../model';
import { licenseText } from '../../license-i18n';
import Button from '../../shared/Button';
import type { SelectedProducts } from '../selection/useSelectedProducts';
import type { Library } from '../library/useLibrary';
import type { ProductEdits } from './useProductEdits';
import type { Navigation } from '../shell/useNavigation';
import type { ProductActions } from './useProductActions';
import type { Notices } from '../shell/useNotice';
import type { UiPreferences } from '../preferences/useUiPreferences';

/** Actions for several selected products. Import joins Download once a selected file is on disk,
 * and takes its place when every product is ready. */
export default function BatchActions({
  ids,
  selectedProducts,
  library,
  edits,
  nav,
  actions,
  notices,
  uiPrefs,
  q,
}: {
  ids: string[];
  selectedProducts: SelectedProducts;
  library: Library;
  edits: ProductEdits;
  nav: Navigation;
  actions: ProductActions;
  notices: Notices;
  uiPrefs: UiPreferences;
  q: (key: string) => string;
}) {
  const { productMap } = selectedProducts;
  const { batchEnabled, aiEnabled, licenseEnabled } = library;
  const {
    editSelection,
    localSelection,
    requestLocalDelete,
    onlineSelection,
    hiddenSelection,
    hideProducts,
  } = edits;
  const { setDialog, openAi, openUnity } = nav;
  const { exportZip, update, syncLicenses, download, allUnavailable } = actions;
  const { run } = notices;
  const { locale } = uiPrefs;
  const ready =
      ids.length > 0 &&
      ids.every((id) => {
        const p = productMap.get(id);
        return !!p && getProductDisplayStatus(p) === 'ready' && isProductDownloaded(p);
      }),
    importable = ids.some((id) => (productMap.get(id)?.downloaded ?? 0) > 0);
  return (
    <div className="scope-actions" role="group" aria-label={q('selected')}>
      <div className="scope-actions-left">
        {batchEnabled.edit && (
          <Button title={q('edit')} onClick={() => editSelection(ids)} disabled={!ids.length}>
            <SlidersHorizontal size={14} />
            <span>{q('edit')}</span>
          </Button>
        )}
        {batchEnabled.collection && (
          <Button
            title={q('addToCollection')}
            onClick={() => setDialog('collection')}
            disabled={!ids.length}
          >
            <Folder size={14} />
            <span>{q('addToCollection')}</span>
          </Button>
        )}
        {batchEnabled.export && (
          <Button title={q('exportZip')} onClick={() => exportZip(ids)} disabled={!ids.length}>
            <FileArchive size={14} />
            <span>{q('exportZip')}</span>
          </Button>
        )}
        {aiEnabled && batchEnabled.ai && (
          <Button title={q('aiTags')} onClick={() => openAi(ids)} disabled={!ids.length}>
            <Tags size={14} />
            <span>{q('aiTags')}</span>
          </Button>
        )}
        {batchEnabled.spending && (
          <Button
            title={q('spending')}
            onClick={() => setDialog('spending')}
            disabled={!ids.length}
          >
            <Package size={14} />
            <span>{q('spending')}</span>
          </Button>
        )}
        {batchEnabled.delete && localSelection(ids) && (
          <Button title={q('delete')} className="danger" onClick={() => requestLocalDelete(ids)}>
            <Trash2 size={14} />
            <span>{q('delete')}</span>
          </Button>
        )}
        {batchEnabled.hide && onlineSelection(ids) && (
          <Button
            title={q(hiddenSelection(ids) ? 'unhide' : 'hide')}
            onClick={() => run(hideProducts(ids, !hiddenSelection(ids)))}
          >
            <EyeOff size={14} />
            <span>{q(hiddenSelection(ids) ? 'unhide' : 'hide')}</span>
          </Button>
        )}
      </div>
      <div className="scope-actions-right">
        {batchEnabled.update && (
          <Button title={q('update')} onClick={() => update(ids)} disabled={!ids.length}>
            <RefreshCw size={14} />
            <span>{q('update')}</span>
          </Button>
        )}
        {licenseEnabled && batchEnabled.licenses && (
          <Button
            title={licenseText(locale, 'syncLicenses')}
            onClick={() => syncLicenses(ids)}
            disabled={!ids.length}
          >
            <Scale size={14} />
            <span>{licenseText(locale, 'syncLicenses')}</span>
          </Button>
        )}
        {batchEnabled.download && !ready && (
          <Button
            title={q('download')}
            primary
            onClick={() => download(ids)}
            disabled={!ids.length || allUnavailable(ids)}
          >
            <Download size={14} />
            <span>{q('download')}</span>
          </Button>
        )}
        {batchEnabled.unity && importable && (
          <Button
            title={q('importUnity')}
            className="unity-action"
            primary={ready}
            onClick={() => openUnity(ids)}
          >
            <FileBox size={14} />
            <span>{q('importUnity')}</span>
          </Button>
        )}
      </div>
    </div>
  );
}
