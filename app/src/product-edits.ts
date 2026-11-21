import type { Product } from './model';
import { inferCategoryEvidence } from './category-inference';

// The App does not call these: product edits, undo and source refreshes go through the native
// catalogue.
export type EditableField =
  | 'title'
  | 'category'
  | 'tags'
  | 'note'
  | 'ownedAvatars'
  | 'sensitive'
  | 'rejectedTags';
export type ProductPatch = Partial<Pick<Product, EditableField>>;
export interface UndoEntry {
  id: string;
  before: ProductPatch;
  after: ProductPatch;
  wasCustom?: boolean;
  beforeTitleOverride?: boolean;
  afterTitleOverride?: boolean;
  beforeCategoryOverride?: boolean;
  afterCategoryOverride?: boolean;
}
export interface UndoChange {
  entries: UndoEntry[];
}
/** Only explicitly edited fields participate, so undo cannot erase unrelated later edits. */
export function applyProductPatch(
  products: readonly Product[],
  ids: readonly string[],
  patch: ProductPatch,
): { products: Product[]; undo: UndoChange } {
  const scope = new Set(ids);
  const entries: UndoEntry[] = [];
  const result = products.map((product) => {
    if (!scope.has(product.id)) return product;
    const before: ProductPatch = {};
    for (const field of Object.keys(patch) as EditableField[])
      Object.assign(before, { [field]: structuredClone(product[field]) });
    entries.push({
      id: product.id,
      before,
      after: structuredClone(patch),
      wasCustom: product.custom,
      beforeTitleOverride: product.titleOverride,
      afterTitleOverride: Object.hasOwn(patch, 'title') ? true : product.titleOverride,
      beforeCategoryOverride: product.categoryOverride,
      afterCategoryOverride: Object.hasOwn(patch, 'category') ? true : product.categoryOverride,
    });
    return {
      ...product,
      ...structuredClone(patch),
      custom: true,
      titleOverride: Object.hasOwn(patch, 'title') ? true : product.titleOverride,
      categoryOverride: Object.hasOwn(patch, 'category') ? true : product.categoryOverride,
      sourceTitle: product.sourceTitle ?? product.title,
      sourceTags: product.sourceTags ?? [...product.tags],
    };
  });
  return { products: result, undo: { entries } };
}
export function undoProductPatch(
  products: readonly Product[],
  change: UndoChange,
): { products: Product[]; conflicts: string[] } {
  const entries = new Map(change.entries.map((entry) => [entry.id, entry]));
  const conflicts: string[] = [];
  const result = products.map((product) => {
    const entry = entries.get(product.id);
    if (!entry) return product;
    const fields = Object.keys(entry.after) as EditableField[];
    if (
      fields.some(
        (field) => JSON.stringify(product[field]) !== JSON.stringify(entry.after[field]),
      ) ||
      (Object.hasOwn(entry.after, 'title') && product.titleOverride !== entry.afterTitleOverride) ||
      (Object.hasOwn(entry.after, 'category') &&
        product.categoryOverride !== entry.afterCategoryOverride)
    ) {
      conflicts.push(product.id);
      return product;
    }
    return {
      ...product,
      ...structuredClone(entry.before),
      custom: entry.wasCustom,
      titleOverride: Object.hasOwn(entry.before, 'title')
        ? entry.beforeTitleOverride
        : product.titleOverride,
      categoryOverride: Object.hasOwn(entry.before, 'category')
        ? entry.beforeCategoryOverride
        : product.categoryOverride,
    };
  });
  return { products: result, conflicts };
}
/** Mock source refresh retains the user layer and never infers identity from names. */
export function refreshSource(
  product: Product,
  source: Partial<Pick<Product, 'title' | 'shop' | 'listingAvatars' | 'sourceCategory' | 'tags'>>,
): Product {
  const rawCategory = source.sourceCategory ?? product.sourceCategory;
  const inference = inferCategoryEvidence(rawCategory, source.tags ?? product.tags);
  const categoryChanged = Object.hasOwn(source, 'sourceCategory') || Object.hasOwn(source, 'tags');
  return {
    ...product,
    shop: source.shop ?? product.shop,
    listingAvatars: source.listingAvatars ?? product.listingAvatars,
    sourceCategory: rawCategory,
    category:
      product.categoryOverride === true || !categoryChanged ? product.category : inference.category,
    categoryInference: categoryChanged ? inference : product.categoryInference,
    sourceTags: source.tags ?? product.sourceTags ?? [...product.tags],
    sourceTitle: source.title ?? product.sourceTitle ?? product.title,
    title: product.titleOverride === true ? product.title : (source.title ?? product.title),
  };
}
/** Clears only the title override. Notes, tags, classification and other corrections remain intact. */
export function resetSourceTitle(product: Product): Product {
  return { ...product, title: product.sourceTitle ?? product.title, titleOverride: false };
}
