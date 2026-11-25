import { useState } from 'react';
import type { EditSubject } from './useEditSubject';

/** The plain fields: the name of a saved search or theme, note, category and R-18, with batch
 * edit's note replacement and three-state R-18. */
export function useDetailDrafts({ subject }: { subject: EditSubject }) {
  const { batch, active, openingBatch, p } = subject;
  const [note, setNote] = useState(batch ? '' : (active?.note ?? ''));
  const [category, setCategory] = useState(batch ? '' : (active?.category ?? 'NonVRChat'));
  const [sensitive, setSensitive] = useState(active?.sensitive ?? false);
  const [rating, setRating] = useState('unchanged');
  const initialBatchRating =
    openingBatch.length > 0 && openingBatch.every((item) => item.sensitive)
      ? 'on'
      : openingBatch.some((item) => item.sensitive)
        ? 'mixed'
        : 'off';
  const batchRating = rating === 'unchanged' ? initialBatchRating : rating;
  const ratingChanged = rating !== 'unchanged' && rating !== initialBatchRating;
  const [noteChanged, setNoteChanged] = useState(false);
  const [name, setName] = useState(p.dialog === 'theme' ? (p.themeName ?? '') : '');
  return {
    note,
    setNote,
    category,
    setCategory,
    sensitive,
    setSensitive,
    rating,
    setRating,
    batchRating,
    ratingChanged,
    noteChanged,
    setNoteChanged,
    name,
    setName,
  };
}
export type DetailDrafts = ReturnType<typeof useDetailDrafts>;
