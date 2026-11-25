import type { SensitiveMode, SortDirection } from '../../model';
import { licenseCells } from '../../Licenses';

/** What the library lists: search text, sidebar scopes and toolbar filters. */
export type Query = {
  search: string;
  category: string;
  avatar: string;
  status: string;
  sensitive: SensitiveMode;
  accountIds: string[] | null;
  includeOffline: boolean;
  quick: 'all' | 'downloaded' | 'attention' | 'hidden';
  acquisition: string[] | null;
  license: Record<string, string> | null;
};
export type Saved = {
  id?: string;
  name: string;
  query: Query;
  sort?: string;
  direction?: SortDirection;
};

export const fresh: Query = {
  search: '',
  category: 'all',
  avatar: 'all',
  status: 'all',
  sensitive: 'blur',
  accountIds: null,
  includeOffline: true,
  quick: 'all',
  acquisition: null,
  license: null,
};
export const acquisitionKinds = ['paid', 'free', 'gift'];
/** A saved search's query as stored, completed with today's defaults. */
export function restoreQuery(raw: unknown): Query {
  const saved =
    raw && typeof raw === 'object'
      ? (raw as Partial<Query>)
      : { search: typeof raw === 'string' ? raw : '' };
  const license = licenseCells(saved.license);
  return {
    ...fresh,
    ...saved,
    includeOffline:
      typeof saved.includeOffline === 'boolean' ? saved.includeOffline : saved.accountIds == null,
    license: Object.keys(license).length ? license : null,
  };
}
// Hidden assets is a place of its own: going anywhere else leaves it.
export const leavingHidden = (old: Query): Partial<Query> =>
  old.quick === 'hidden' ? { quick: 'all' } : {};
