import { useMemo, useState } from 'react';
import { getAvatarAliases, getEffectiveTags } from '../../model';
import { sameTag } from './parts';
import type { EditSubject } from './useEditSubject';

/** Tags: the product's tags and rejected tags, or batch edit's tags to add or remove, with the
 * suggestions and aliases the picker offers. */
export function useTagDraft({ subject }: { subject: EditSubject }) {
  const { opening, allSuggestions, p, active, batch } = subject;
  const initialTags = useMemo(() => (opening ? getEffectiveTags(opening) : []), [opening]);
  const aliases = useMemo(
    () =>
      Object.fromEntries(
        allSuggestions.map((tag) => [
          tag,
          [
            ...getAvatarAliases(tag),
            ...((p.tagOptions ?? []).find((option) => option.label === tag)?.aliases ?? []),
          ],
        ]),
      ),
    [allSuggestions, p.tagOptions],
  );
  const [tags, setTags] = useState(initialTags);
  const [pendingTag, setPendingTag] = useState('');
  const [tagMode, setTagMode] = useState('unchanged');
  const [tagDelta, setTagDelta] = useState<string[]>([]);
  const [pendingDelta, setPendingDelta] = useState('');
  const [rejected, setRejected] = useState(active?.rejectedTags ?? []);
  const suggestions = useMemo(
    () =>
      batch
        ? allSuggestions
        : allSuggestions.filter((tag) => !rejected.some((value) => sameTag(value, tag))),
    [allSuggestions, rejected, batch],
  );
  const updateTags = (next: string[]) => {
    const added = next.filter((tag) => !tags.includes(tag));
    const removed = tags.filter((tag) => !next.includes(tag));
    setTags(next.filter((tag) => !removed.some((value) => sameTag(value, tag))));
    setRejected((values) => [
      ...new Set([
        ...values.filter((tag) => !added.some((value) => sameTag(value, tag))),
        ...removed,
      ]),
    ]);
  };
  return {
    initialTags,
    aliases,
    tags,
    setTags,
    pendingTag,
    setPendingTag,
    tagMode,
    setTagMode,
    tagDelta,
    setTagDelta,
    pendingDelta,
    setPendingDelta,
    rejected,
    setRejected,
    suggestions,
    updateTags,
  };
}
export type TagDraft = ReturnType<typeof useTagDraft>;
