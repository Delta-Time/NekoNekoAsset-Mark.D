import { categoryName } from '../../model';
import type { Library } from '../library/useLibrary';
import type { UiPreferences } from '../preferences/useUiPreferences';
import type { Native } from '../library/useNative';
import type { Filters } from '../filters/useFilters';

/** Category names for any key, the sorted custom categories, and creating, renaming and deleting
 * them. Deleting the listed category lists all categories again. */
export function useCategories({
  library,
  uiPrefs,
  native,
  filters,
  q,
}: {
  library: Library;
  uiPrefs: UiPreferences;
  native: Native;
  filters: Filters;
  q: (key: string) => string;
}) {
  const { customCategories, setCustomCategories } = library;
  const { locale } = uiPrefs;
  const { onNative } = native;
  const { setQuery } = filters;
  const categoryLabel = (key: string) => categoryName(key, customCategories, q),
    categoryList = [...customCategories].sort((a, b) =>
      a.name.localeCompare(b.name, locale, { numeric: true }),
    );
  const changeCategories = async (args: Record<string, unknown>) => {
    const result = await onNative('catalogue', 'custom_category', args);
    if (Array.isArray(result?.customCategories)) setCustomCategories(result.customCategories);
    if (args.op === 'delete')
      setQuery((old) => (old.category === args.id ? { ...old, category: 'all' } : old));
  };
  return { categoryLabel, categoryList, changeCategories };
}
export type Categories = ReturnType<typeof useCategories>;
