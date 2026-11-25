import { Bookmark, Package, Plus, RefreshCw, Search, Settings, X } from 'lucide-react';
import { aiSuggestionsText } from '../../ai-suggestions-i18n';
import Button from '../../shared/Button';
import type { Updates } from '../updates/useUpdates';
import type { Appearance } from '../appearance/useAppearance';
import type { Library } from '../library/useLibrary';
import type { Navigation } from './useNavigation';
import type { UiPreferences } from '../preferences/useUiPreferences';
import type { Shortcuts } from './useShortcuts';
import type { Filters } from '../filters/useFilters';
import type { SettingsPane } from '../settings/useSettingsPane';

/** The top bar: views, search, save search, sync, import and settings. */
export default function Topbar({
  updates,
  appearance,
  library,
  nav,
  uiPrefs,
  shortcuts,
  filters,
  settingsPane,
  q,
}: {
  updates: Updates;
  appearance: Appearance;
  library: Library;
  nav: Navigation;
  uiPrefs: UiPreferences;
  shortcuts: Shortcuts;
  filters: Filters;
  settingsPane: SettingsPane;
  q: (key: string) => string;
}) {
  const { updateOpen } = updates;
  const { themeDeleteTarget } = appearance;
  const { aiEnabled, syncing } = library;
  const { view, setView, setDialog, setFlow, setImportSeed } = nav;
  const { locale } = uiPrefs;
  const { searchRef } = shortcuts;
  const { query, updateQuery } = filters;
  const { toggleSettings } = settingsPane;
  return (
    <header className="topbar" inert={updateOpen || !!themeDeleteTarget}>
      <div className="brand">
        <span className="brand-mark">
          <Package size={21} />
        </span>
        <span>
          NekoNeko<span className="brand-light">Asset</span>
        </span>
      </div>
      <nav className="main-nav">
        {['library', 'activity', ...(aiEnabled ? ['ai-suggestions'] : [])].map((name) => (
          <button
            key={name}
            className={view === name ? 'active' : ''}
            onClick={() => setView(name)}
          >
            {name === 'ai-suggestions' ? aiSuggestionsText(locale, 'aiSuggestions') : q(name)}
          </button>
        ))}
      </nav>
      <div className="searchbox">
        <Search size={16} />
        <input
          ref={searchRef}
          aria-label={q('search')}
          placeholder={q('searchPlaceholder')}
          value={query.search}
          onChange={(e) => {
            updateQuery({ search: e.target.value });
            if (view === 'activity' || view === 'ai-suggestions') setView('library');
          }}
        />
        <Button
          title={q('clearSearch')}
          disabled={!query.search}
          onMouseDown={(e) => e.preventDefault()}
          onClick={() => {
            const keyboard = !!document.activeElement?.closest('.searchbox button');
            updateQuery({ search: '' });
            if (keyboard) searchRef.current?.focus();
          }}
        >
          <X size={13} />
        </Button>
        <kbd>Ctrl F</kbd>
      </div>
      <Button title={q('saveSearch')} onClick={() => setDialog('saveSearch')}>
        <Bookmark size={15} />
      </Button>
      <div className="top-actions">
        <Button title={q(syncing ? 'syncing' : 'sync')} onClick={() => setFlow('sync')}>
          <RefreshCw size={15} className={syncing ? 'sync-spinning' : undefined} />
          <span>{q(syncing ? 'syncing' : 'sync')}</span>
        </Button>
        <Button
          title={q('import')}
          onClick={() => {
            setImportSeed(undefined);
            setFlow('import');
          }}
        >
          <Plus size={17} />
          <span>{q('importAssets')}</span>
        </Button>
        <Button title={q('settings')} onClick={toggleSettings}>
          <Settings size={17} />
        </Button>
      </div>
    </header>
  );
}
