import { useMemo, useRef, useState } from 'react';
import { getEffectiveTags } from '../../model';
import { editorText } from '../../editor-i18n';
import type { EditDialogsProps } from '../../EditDialogs';

/** What the dialog edits, fixed when it opens: the props with the selection of that moment, the
 * product as it was then, batch mode and its products, and every tag the library knows. */
export function useEditSubject({ props }: { props: EditDialogsProps }) {
  const [target] = useState(() => ({ active: props.active, selected: [...props.selected] }));
  const p = { ...props, ...target };
  const L = (key: string) => editorText(p.locale, key);
  const active = p.active;
  // This editor is keyed by product. Background source/AI updates must not
  // turn untouched opening values into manual overrides when another field saves.
  const opening = useRef(active).current;
  const offline = opening?.provider === 'local';
  const batch = p.dialog === 'batch';
  const productMap = useMemo(
    () => new Map(p.products.map((item) => [item.id, item])),
    [p.products],
  );
  const [openingBatch] = useState(() =>
    batch
      ? p.selected.flatMap((id) => {
          const item = productMap.get(id);
          return item ? [item] : [];
        })
      : [],
  );
  const scoped = batch
    ? openingBatch.map((item) => productMap.get(item.id) ?? item)
    : p.selected.flatMap((id) => {
        const item = productMap.get(id);
        return item ? [item] : [];
      });
  const allSuggestions = useMemo(
    () => [
      ...new Set([
        ...(p.tagOptions ?? []).map((option) => option.label),
        ...p.products.flatMap((product) => [
          ...getEffectiveTags(product),
          ...(product.tagSuggestions ?? []).map((option) => option.label),
        ]),
      ]),
    ],
    [p.products, p.tagOptions],
  );
  return { p, L, active, opening, offline, batch, openingBatch, scoped, allSuggestions };
}
export type EditSubject = ReturnType<typeof useEditSubject>;
