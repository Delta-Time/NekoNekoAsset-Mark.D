import { AlertCircle, Bookmark, CheckCircle2, EyeOff, Folder, Grid2X2 } from 'lucide-react';
import { categoryKeys, defaultSortDirection } from '../../model';
import { CategoryIcon } from '../../LibraryIcons';
import { contentLang } from '../../content-lang';
import { fresh, leavingHidden } from '../filters/query';
import type { UiPreferences } from '../preferences/useUiPreferences';
import type { Filters } from '../filters/useFilters';
import type { Results } from '../results/useResults';
import type { Categories } from '../categories/useCategories';
import type { Navigation } from '../shell/useNavigation';
import type { Avatars } from '../avatars/useAvatars';
import type { Notices } from '../shell/useNotice';
import type { Native } from '../library/useNative';
import type { CollectionStore } from '../collections/useCollections';
import type { Library } from '../library/useLibrary';
import type { Selection } from '../selection/useSelection';

/** The navigation sidebar: quick scopes, categories, custom categories, avatars, collections and saved searches. */
export default function Sidebar({
  uiPrefs,
  filters,
  results,
  categories,
  nav,
  avatars,
  notices,
  native,
  collectionStore,
  library,
  selection,
  q,
}: {
  uiPrefs: UiPreferences;
  filters: Filters;
  results: Results;
  categories: Categories;
  nav: Navigation;
  avatars: Avatars;
  notices: Notices;
  native: Native;
  collectionStore: CollectionStore;
  library: Library;
  selection: Selection;
  q: (key: string) => string;
}) {
  const { navWidth, locale } = uiPrefs;
  const {
    query,
    collection,
    clearFilters,
    updateQuery,
    setCollection,
    savedScope,
    setQuery,
    setSort,
    setDirection,
    setSavedScope,
  } = filters;
  const { quickCounts } = results;
  const { categoryList } = categories;
  const { setDialog } = nav;
  const { avatarChoices, avatarLabel, avatarOptions } = avatars;
  const { run } = notices;
  const { refreshAvatarOptions } = native;
  const { collections } = collectionStore;
  const { saved } = library;
  const { leaveView } = selection;
  return (
    <aside className="sidebar" style={{ width: navWidth, flexBasis: navWidth }}>
      <div className="quick-scopes">
        <button
          className={`nav-item quick-scope ${query.category === 'all' && !collection && query.avatar === 'all' && query.quick === 'all' ? 'active' : ''}`}
          onClick={clearFilters}
        >
          <Grid2X2 size={15} />
          <span>{q('allAssets')}</span>
          <span className="count">{quickCounts.all.toLocaleString(locale)}</span>
        </button>
        <button
          className={`nav-item quick-scope ${query.quick === 'downloaded' ? 'active' : ''}`}
          onClick={() =>
            updateQuery({ quick: 'downloaded', category: 'all', avatar: 'all', status: 'all' })
          }
        >
          <CheckCircle2 size={15} />
          <span>{q('downloaded')}</span>
          <span className="count">{quickCounts.downloaded.toLocaleString(locale)}</span>
        </button>
        <button
          className={`nav-item quick-scope ${query.quick === 'attention' ? 'active' : ''}`}
          onClick={() =>
            updateQuery({ quick: 'attention', category: 'all', avatar: 'all', status: 'all' })
          }
        >
          <AlertCircle size={15} />
          <span>{q('attention')}</span>
          <span className="count">{quickCounts.attention.toLocaleString(locale)}</span>
        </button>
        <button
          className={`nav-item quick-scope ${query.quick === 'hidden' ? 'active' : ''}`}
          onClick={() => {
            updateQuery({ ...fresh, sensitive: query.sensitive, quick: 'hidden' });
            setCollection('');
          }}
        >
          <EyeOff size={15} />
          <span>{q('hidden')}</span>
          <span className="count">{(quickCounts.hidden ?? 0).toLocaleString(locale)}</span>
        </button>
      </div>
      <details className="nav-foldout" open>
        <summary>{q('categories')}</summary>
        {categoryKeys.map((key) => (
          <button
            className={`nav-item category-item ${query.category === key ? 'active' : ''}`}
            key={key}
            data-drop-category={key}
            onClick={() => updateQuery({ category: key })}
          >
            <CategoryIcon category={key} />
            <span>{q(key)}</span>
          </button>
        ))}
      </details>
      <section className="nav-avatar-section">
        <div className="nav-section-heading">
          <details className="nav-foldout" open>
            <summary>{q('customCategories')}</summary>
            {categoryList.map((category) => (
              <button
                className={`nav-item category-item custom-category ${query.category === category.id ? 'active' : ''}`}
                key={category.id}
                title={category.name}
                data-drop-category={category.id}
                onClick={() => updateQuery({ category: category.id })}
              >
                <CategoryIcon category={category.id} />
                <span>{category.name}</span>
              </button>
            ))}
          </details>
          <button
            className="avatar-manager"
            title={q('customCategories')}
            aria-label={q('customCategories')}
            onClick={() => setDialog('manageCategories')}
          >
            +
          </button>
        </div>
      </section>
      <section className="nav-avatar-section">
        <div className="nav-section-heading">
          <details className="nav-foldout" open>
            <summary>{q('compatibleAvatars')}</summary>
            {avatarChoices.map((avatar) => (
              <button
                key={avatar}
                title={avatarLabel(avatar)}
                className={`nav-item avatar-row ${query.avatar === avatar ? 'active' : ''}`}
                onClick={() => updateQuery({ avatar })}
              >
                <span lang={contentLang(avatarLabel(avatar))}>{avatarLabel(avatar)}</span>
                <small>
                  {avatarOptions.find((option) => option.id === avatar)?.matchingProductCount ?? 0}
                </small>
              </button>
            ))}
          </details>
          <button
            className="avatar-manager"
            title={q('manageAvatars')}
            aria-label={q('manageAvatars')}
            onClick={() => {
              setDialog('avatars');
              run(refreshAvatarOptions());
            }}
          >
            +
          </button>
        </div>
      </section>
      <section className="nav-avatar-section">
        <div className="nav-section-heading">
          <details className="nav-foldout" open>
            <summary>{q('collections')}</summary>
            {Object.keys(collections).map((name) => (
              <button
                className={`nav-item ${collection === name ? 'active' : ''}`}
                key={name}
                title={name}
                onClick={() => {
                  setCollection(name);
                  updateQuery(leavingHidden(query));
                }}
              >
                <Folder size={13} />
                <span>{name}</span>
              </button>
            ))}
          </details>
          <button
            className="avatar-manager"
            title={q('collections')}
            aria-label={q('collections')}
            onClick={() => setDialog('manageCollections')}
          >
            +
          </button>
        </div>
      </section>
      <section className="nav-avatar-section">
        <div className="nav-section-heading">
          <details className="nav-foldout" open>
            <summary>{q('savedSearches')}</summary>
            {saved.map((search, i) => (
              <button
                className={`nav-item ${savedScope === i ? 'active' : ''}`}
                key={i}
                title={search.name}
                onClick={() => {
                  setQuery({
                    ...fresh,
                    ...search.query,
                    sensitive: query.sensitive,
                    accountIds: search.query.accountIds ?? null,
                  });
                  setSort(search.sort || 'acquired');
                  setDirection(search.direction || defaultSortDirection(search.sort || 'acquired'));
                  setCollection('');
                  setSavedScope(i);
                  leaveView();
                }}
              >
                <Bookmark size={13} />
                <span>{search.name}</span>
              </button>
            ))}
          </details>
          <button
            className="avatar-manager"
            title={q('savedSearches')}
            aria-label={q('savedSearches')}
            onClick={() => setDialog('manageSearches')}
          >
            +
          </button>
        </div>
      </section>
    </aside>
  );
}
