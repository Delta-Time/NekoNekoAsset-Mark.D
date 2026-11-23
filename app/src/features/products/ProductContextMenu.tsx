import {
  BellOff,
  CheckCircle2,
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
  X,
} from 'lucide-react';
import { licenseText } from '../../license-i18n';
import { LibraryContextMenu } from '../../LibraryMenus';
import type { Navigation } from '../shell/useNavigation';
import type { ProductActions } from './useProductActions';
import type { Library } from '../library/useLibrary';
import type { Selection } from '../selection/useSelection';
import type { Results } from '../results/useResults';
import type { UiPreferences } from '../preferences/useUiPreferences';
import type { ProductEdits } from './useProductEdits';
import type { Notices } from '../shell/useNotice';

/** The right-click menu for the products it was opened on. */
export default function ProductContextMenu({
  nav,
  actions,
  library,
  selection,
  results,
  uiPrefs,
  edits,
  notices,
  q,
}: {
  nav: Navigation;
  actions: ProductActions;
  library: Library;
  selection: Selection;
  results: Results;
  uiPrefs: UiPreferences;
  edits: ProductEdits;
  notices: Notices;
  q: (key: string) => string;
}) {
  const { context, setDialog, openUnity, openAi, setContext } = nav;
  const {
    exportZip,
    update,
    syncLicenses,
    allUnavailable,
    download,
    activeDownloads,
    cancelDownloads,
    needsAttention,
    dismissAttention,
    hasDownloadedFiles,
    requestDeleteDownloads,
  } = actions;
  const { aiEnabled, licenseEnabled } = library;
  const { addToSelection } = selection;
  const { matchedIds } = results;
  const { locale } = uiPrefs;
  const {
    editSelection,
    localSelection,
    requestLocalDelete,
    onlineSelection,
    hiddenSelection,
    hideProducts,
  } = edits;
  const { run } = notices;
  return (
    context && (
      <LibraryContextMenu
        {...context}
        actions={[
          {
            id: 'edit',
            label: q('edit'),
            icon: <SlidersHorizontal size={14} />,
            onClick: () => editSelection(context.ids),
          },
          {
            id: 'collection',
            label: q('addToCollection'),
            icon: <Folder size={14} />,
            onClick: () => setDialog('collection'),
          },
          {
            id: 'unity',
            label: q('importUnity'),
            icon: <FileBox size={14} />,
            onClick: () => openUnity(context.ids),
          },
          {
            id: 'export',
            label: q('exportZip'),
            icon: <FileArchive size={14} />,
            onClick: () => void exportZip(context.ids),
          },
          {
            id: 'ai',
            label: q('aiTags'),
            icon: <Tags size={14} />,
            disabled: !aiEnabled,
            onClick: () => openAi(context.ids),
          },
          {
            id: 'spending',
            label: q('spending'),
            icon: <Package size={14} />,
            onClick: () => setDialog('spending'),
          },
          {
            id: 'selectAll',
            label: q('selectAll'),
            icon: <CheckCircle2 size={14} />,
            onClick: () => addToSelection(matchedIds),
          },
          {
            id: 'update',
            label: q('update'),
            icon: <RefreshCw size={14} />,
            onClick: () => update(context.ids),
          },
          ...(licenseEnabled
            ? [
                {
                  id: 'licenses',
                  label: licenseText(locale, 'syncLicenses')!,
                  icon: <Scale size={14} />,
                  onClick: () => syncLicenses(context.ids),
                },
              ]
            : []),
          {
            id: 'download',
            label: q('download'),
            icon: <Download size={14} />,
            disabled: allUnavailable(context.ids),
            onClick: () => download(context.ids),
          },
          ...(context.ids.some((id) => activeDownloads(id).length)
            ? [
                {
                  id: 'cancelDownload',
                  label: q('cancelDownload'),
                  icon: <X size={14} />,
                  onClick: () => cancelDownloads(context.ids),
                },
              ]
            : []),
          ...(needsAttention(context.ids)
            ? [
                {
                  id: 'dismissAttention',
                  label: q('dismissAttention'),
                  icon: <BellOff size={14} />,
                  onClick: () => dismissAttention(context.ids),
                },
              ]
            : []),
          ...(localSelection(context.ids)
            ? [
                {
                  id: 'delete',
                  label: q('delete'),
                  icon: <Trash2 size={14} />,
                  danger: true,
                  onClick: () => requestLocalDelete(context.ids),
                },
              ]
            : []),
          ...(onlineSelection(context.ids)
            ? [
                {
                  id: 'hide',
                  label: q(hiddenSelection(context.ids) ? 'unhide' : 'hide'),
                  icon: <EyeOff size={14} />,
                  onClick: () => run(hideProducts(context.ids, !hiddenSelection(context.ids))),
                },
              ]
            : []),
          ...(onlineSelection(context.ids) && hasDownloadedFiles(context.ids)
            ? [
                {
                  id: 'deleteDownloads',
                  label: q('deleteDownloads'),
                  icon: <Trash2 size={14} />,
                  danger: true,
                  onClick: () => requestDeleteDownloads(context.ids),
                },
              ]
            : []),
        ]}
        onClose={() => setContext(null)}
      />
    )
  );
}
