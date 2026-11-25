import { X } from 'lucide-react';
import { version as appVersion } from '../../../package.json';
import { updateText } from '../../update-i18n';
import type { Updates } from '../updates/useUpdates';
import type { Appearance } from '../appearance/useAppearance';
import type { UiPreferences } from '../preferences/useUiPreferences';
import type { Results } from '../results/useResults';
import type { CollectionStore } from '../collections/useCollections';
import type { Notices } from './useNotice';
import type { Library } from '../library/useLibrary';
import type { Navigation } from './useNavigation';

/** The status bar: version and update check, product total, and the current notice. */
export default function Statusbar({
  updates,
  appearance,
  uiPrefs,
  results,
  collectionStore,
  notices,
  library,
  nav,
  q,
}: {
  updates: Updates;
  appearance: Appearance;
  uiPrefs: UiPreferences;
  results: Results;
  collectionStore: CollectionStore;
  notices: Notices;
  library: Library;
  nav: Navigation;
  q: (key: string) => string;
}) {
  const { updateOpen, updateCanOpen, setUpdateOpen, setUpdateRequest, updateAvailable } = updates;
  const { themeDeleteTarget } = appearance;
  const { locale } = uiPrefs;
  const { catalogueTotal } = results;
  const { collectionBusy } = collectionStore;
  const { notice, noticeJobId, clearNotice } = notices;
  const { loadingError, setLoadingError } = library;
  return (
    <footer className="statusbar" inert={updateOpen || !!themeDeleteTarget}>
      <button
        type="button"
        className="app-version"
        title={updateText(locale, 'check')}
        aria-label={updateText(locale, 'check')}
        disabled={!updateCanOpen}
        onClick={() => {
          setUpdateOpen(true);
          setUpdateRequest((value) => value + 1);
        }}
      >
        v{appVersion}
        {updateAvailable && <span className="app-update-dot" />}
      </button>
      <span>{q('totalProducts').replace('{count}', catalogueTotal.toLocaleString(locale))}</span>
      <span className="spacer" />
      {(collectionBusy || notice || loadingError) && (
        <span role="status" className="inline-notice">
          <span title={notice || (loadingError ? q(loadingError) : q('processing'))}>
            {notice || (loadingError ? q(loadingError) : q('processing'))}
          </span>
          {noticeJobId && (
            <button
              type="button"
              className="notice-activity"
              onClick={() => {
                nav.openActivityJob(noticeJobId);
                clearNotice();
              }}
            >
              {q('activity')}
            </button>
          )}
          {!collectionBusy && (
            <button
              title={q('close')}
              onClick={() => {
                clearNotice();
                setLoadingError('');
              }}
            >
              <X size={12} />
            </button>
          )}
        </span>
      )}
    </footer>
  );
}
