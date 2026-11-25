import { useId, useLayoutEffect, useState } from 'react';
import { getEffectiveTags } from '../../model';
import type { Product } from '../../model';

/** The product's tags and how many rows fit before "Show all". */
export function useTagFold({ p }: { p: Product }) {
  // Tags show three rows and fold the rest behind "Show all". Rows are read from layout,
  // so a long tag that wraps still counts as one row and the clamp never cuts a tag.
  const tags = getEffectiveTags(p),
    tagKey = JSON.stringify([p.id, tags]),
    tagsId = useId();
  const [tagList, setTagList] = useState<HTMLDivElement | null>(null);
  const [tagRows, setTagRows] = useState<{ key: string; clamp: number; full: number } | null>(null),
    [expandedTags, setExpandedTags] = useState('');
  useLayoutEffect(() => {
    if (!tagList) return;
    const measure = () => {
      let rows = 0,
        left = Infinity,
        clamp = 0;
      for (const item of Array.from(tagList.children) as HTMLElement[]) {
        if (item.offsetLeft <= left) rows++;
        left = item.offsetLeft;
        if (rows <= 3) clamp = Math.max(clamp, item.offsetTop + item.offsetHeight);
      }
      const full = tagList.offsetHeight;
      setTagRows((old) =>
        rows <= 3
          ? null
          : old?.key === tagKey && old.clamp === clamp && old.full === full
            ? old
            : { key: tagKey, clamp, full },
      );
    };
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(tagList);
    return () => observer.disconnect();
  }, [tagList, tagKey]);
  const tagClamp = tagRows?.key === tagKey ? tagRows : null,
    tagsExpanded = !!tagClamp && expandedTags === p.id;
  return { tags, tagsId, setTagList, setExpandedTags, tagClamp, tagsExpanded };
}
export type TagFold = ReturnType<typeof useTagFold>;
