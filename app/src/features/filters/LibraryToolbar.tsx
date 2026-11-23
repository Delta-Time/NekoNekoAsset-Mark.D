import { Columns2, X } from 'lucide-react';
import { statusKeys } from '../../model';
import type { SortDirection } from '../../model';
import { nativeWorkflowText } from '../../native-workflow-i18n';
import { licenseText } from '../../license-i18n';
import { licenseSummary } from '../../Licenses';
import { ResponsiveLibraryToolbar } from '../../LibraryMenus';
import Button from '../../shared/Button';
import { acquisitionKinds } from './query';
import type { Filters } from './useFilters';
import type { UiPreferences } from '../preferences/useUiPreferences';
import type { Library } from '../library/useLibrary';
import type { Navigation } from '../shell/useNavigation';
import type { Selection } from '../selection/useSelection';
import type { Results } from '../results/useResults';
import type { Categories } from '../categories/useCategories';
import type { Avatars } from '../avatars/useAvatars';

/** Above the results: account, acquisition and license filters, sort controls and filter chips. */
export default function LibraryToolbar({
  filters,
  uiPrefs,
  library,
  nav,
  selection,
  results,
  categories,
  avatars,
  q,
}: {
  filters: Filters;
  uiPrefs: UiPreferences;
  library: Library;
  nav: Navigation;
  selection: Selection;
  results: Results;
  categories: Categories;
  avatars: Avatars;
  q: (key: string) => string;
}) {
  const {
    query,
    updateQuery,
    sort,
    changeSort,
    direction,
    setDirection,
    setSavedScope,
    collection,
    setCollection,
    defaultAccounts,
    setSort,
    filtersActive,
    clearFilters,
  } = filters;
  const { locale } = uiPrefs;
  const { accounts, licenseEnabled } = library;
  const { licenseButton, setLicenseOpen, setInspector } = nav;
  const { leaveView } = selection;
  const { resultsPending, matchedIds } = results;
  const { categoryLabel } = categories;
  const { avatarLabel } = avatars;
  const filterChip = (id: string, label: string, value: string, onClear: () => void) => (
    <span key={id} className="filter-chip">
      <span title={label + ': ' + value}>
        {label}: {value}
      </span>
      <button
        aria-label={q('clearFilters') + ' · ' + label}
        title={q('clearFilters') + ' · ' + label}
        onClick={onClear}
      >
        <X size={12} />
      </button>
    </span>
  );
  const accountControl = (
    <details className="account-scope">
      <summary aria-label={q('accountScope')}>
        {query.accountIds === null
          ? q('accountAll')
          : query.accountIds.length
            ? q('selectedAccounts') + ' · ' + query.accountIds.length
            : query.includeOffline
              ? nativeWorkflowText(locale, 'offlineFiles')
              : q('noAccounts')}
      </summary>
      <div className="popup-menu">
        <label className="check-row">
          <input
            type="checkbox"
            checked={query.accountIds === null}
            onChange={(e) => updateQuery({ accountIds: e.target.checked ? null : [] })}
          />
          {q('accountAll')}
        </label>
        {accounts.map((account) => (
          <label className="check-row" key={account.id}>
            <input
              type="checkbox"
              checked={query.accountIds === null || query.accountIds.includes(account.id)}
              onChange={(e) => {
                const ids = query.accountIds ?? accounts.map((account) => account.id);
                updateQuery({
                  accountIds: e.target.checked
                    ? [...new Set([...ids, account.id])]
                    : ids.filter((id) => id !== account.id),
                });
              }}
            />
            <span>
              {account.store} · {account.name}
            </span>
          </label>
        ))}
        <label className="check-row">
          <input
            type="checkbox"
            checked={query.includeOffline}
            onChange={(e) => updateQuery({ includeOffline: e.target.checked })}
          />
          <span>{nativeWorkflowText(locale, 'offlineFiles')}</span>
        </label>
      </div>
    </details>
  );
  const acquisitionControl = (
    <details className="account-scope acquisition-scope">
      <summary aria-label={q('acquisitionFilter')}>
        {(query.acquisition ?? acquisitionKinds)
          .map((kind) => q(kind + (query.acquisition?.length === 1 ? 'Assets' : 'Short')))
          .join(' · ') || q('noAssets')}
      </summary>
      <div className="popup-menu">
        {acquisitionKinds.map((kind) => (
          <label className="check-row" key={kind}>
            <input
              type="checkbox"
              checked={!query.acquisition || query.acquisition.includes(kind)}
              onChange={(e) => {
                const kinds = acquisitionKinds.filter((item) =>
                  item === kind
                    ? e.target.checked
                    : !query.acquisition || query.acquisition.includes(item),
                );
                updateQuery({
                  acquisition: kinds.length === acquisitionKinds.length ? null : kinds,
                });
              }}
            />
            <span>{q(kind + 'Assets')}</span>
          </label>
        ))}
      </div>
    </details>
  );
  const licenseControl = licenseEnabled && (
    <button
      type="button"
      ref={licenseButton}
      className="license-scope"
      aria-pressed={!!query.license}
      onClick={() => setLicenseOpen(true)}
    >
      {licenseText(locale, 'license')}
      {query.license ? ` · ${Object.keys(query.license).length.toLocaleString(locale)}` : ''}
    </button>
  );
  const sortControls = (
    <>
      <select aria-label={q('sort')} value={sort} onChange={(e) => changeSort(e.target.value)}>
        {['acquired', 'title', 'price', 'status'].map((key) => (
          <option key={key} value={key}>
            {q(key)}
          </option>
        ))}
      </select>
      <select
        className="sort-direction"
        title={q(direction === 'asc' ? 'ascending' : 'descending')}
        aria-label={q('sortDirection')}
        value={direction}
        onChange={(e) => {
          setDirection(e.target.value as SortDirection);
          setSavedScope(null);
          leaveView();
        }}
      >
        <option value="asc">{q('ascending')}</option>
        <option value="desc">{q('descending')}</option>
      </select>
      {sort === 'status' && (
        <select
          aria-label={q('status')}
          value={query.status}
          onChange={(e) => updateQuery({ status: e.target.value })}
        >
          <option value="all">{q('allStates')}</option>
          {statusKeys.map((status) => (
            <option key={status} value={status}>
              {q(status)}
            </option>
          ))}
        </select>
      )}
      <span className="toolbar-spacer" />
      <span className="result-total">
        <b>{resultsPending ? '…' : matchedIds.length.toLocaleString(locale)}</b> {q('products')}
      </span>
      <Button title={q('details')} onClick={() => setInspector((value) => !value)}>
        <Columns2 size={14} />
      </Button>
    </>
  );
  const chips = (
    <>
      {query.search &&
        filterChip('search', q('searchFilter'), query.search, () => updateQuery({ search: '' }))}
      {query.category !== 'all' &&
        filterChip('category', q('categoryFilter'), categoryLabel(query.category), () =>
          updateQuery({ category: 'all' }),
        )}
      {query.avatar !== 'all' &&
        filterChip('avatar', q('avatarFilter'), avatarLabel(query.avatar), () =>
          updateQuery({ avatar: 'all' }),
        )}
      {collection &&
        filterChip('collection', q('collectionFilter'), collection, () => {
          setCollection('');
          leaveView();
        })}
      {!defaultAccounts &&
        filterChip(
          'accounts',
          q('accountFilter'),
          [
            ...(query.accountIds === null
              ? [q('accountAll')]
              : accounts
                  .filter((account) => query.accountIds?.includes(account.id))
                  .map((account) => account.name)),
            ...(query.includeOffline ? [nativeWorkflowText(locale, 'offlineFiles')] : []),
          ].join(', ') || q('noAccounts'),
          () => updateQuery({ accountIds: null, includeOffline: true }),
        )}
      {query.acquisition &&
        filterChip(
          'acquisition',
          q('acquisitionFilter'),
          query.acquisition.map((kind) => q(kind + 'Short')).join(', ') || q('noAssets'),
          () => updateQuery({ acquisition: null }),
        )}
      {licenseEnabled &&
        query.license &&
        filterChip(
          'license',
          licenseText(locale, 'license')!,
          licenseSummary(locale, query.license),
          () => updateQuery({ license: null }),
        )}
      {query.quick !== 'all' &&
        filterChip('quick', q('stateFilter'), q(query.quick), () => updateQuery({ quick: 'all' }))}
      {query.status !== 'all' &&
        filterChip('status', q('stateFilter'), q(query.status), () =>
          updateQuery({ status: 'all' }),
        )}
      {(sort !== 'acquired' || direction !== 'desc') &&
        filterChip(
          'sort',
          q('sortFilter'),
          q(sort) + ' · ' + q(direction === 'asc' ? 'ascending' : 'descending'),
          () => {
            setSort('acquired');
            setDirection('desc');
            leaveView();
          },
        )}
      {filtersActive && (
        <Button className="filter-clear-all" title={q('clearFilters')} onClick={clearFilters}>
          <X size={14} />
        </Button>
      )}
    </>
  );
  return (
    <ResponsiveLibraryToolbar
      accounts={
        <>
          {accountControl}
          {acquisitionControl}
          {licenseControl}
        </>
      }
      controls={sortControls}
      chips={chips}
      label={q('sortingMenu')}
    />
  );
}
