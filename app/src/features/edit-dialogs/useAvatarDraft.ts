import { useLayoutEffect, useMemo, useRef, useState } from 'react';
import {
  productsAvatarOptions,
  productAvatarOptions,
  productAvatarSelection,
  productAvatarDelta,
} from '../../AvatarPicker';
import { errorCode } from '../../backend';
import { equal } from './parts';
import type { EditSubject } from './useEditSubject';
import type { DialogFrame } from './useDialogFrame';

/** Compatible avatars: the product's selection, or each batch product's own selection and what
 * changed in it, and the picker that refreshes the options before it opens. */
export function useAvatarDraft({ subject, frame }: { subject: EditSubject; frame: DialogFrame }) {
  const { batch, scoped, p, opening } = subject;
  const { inFlight, setError, alive, confirmationOpen } = frame;
  const [initialAvatarOptions] = useState(() =>
    batch
      ? productsAvatarOptions(scoped, p.avatarOptions ?? [])
      : opening
        ? productAvatarOptions(opening, p.avatarOptions ?? [])
        : (p.avatarOptions ?? []),
  );
  const avatarOptions = useMemo(
    () => [
      ...(p.avatarOptions ?? []),
      ...initialAvatarOptions.filter((option) => 'legacyValue' in option),
    ],
    [p.avatarOptions, initialAvatarOptions],
  );
  const [initialAvatarSelection] = useState(() =>
    !batch && opening ? productAvatarSelection(opening, initialAvatarOptions) : [],
  );
  const [selectedAvatars, setSelectedAvatars] = useState(initialAvatarSelection);
  const [initialBatchAvatarSelections] = useState(() =>
    Object.fromEntries(
      scoped.map((item) => [item.id, productAvatarSelection(item, initialAvatarOptions)]),
    ),
  );
  const [batchAvatarSelections, setBatchAvatarSelections] = useState(initialBatchAvatarSelections);
  const batchAvatarEnabled = (id: string) =>
    scoped.length > 0 && scoped.every((item) => batchAvatarSelections[item.id]?.includes(id));
  const batchAvatarMixed = (id: string) =>
    !batchAvatarEnabled(id) && scoped.some((item) => batchAvatarSelections[item.id]?.includes(id));
  const pickerSelection = batch
    ? avatarOptions.filter((option) => batchAvatarEnabled(option.id)).map((option) => option.id)
    : selectedAvatars;
  const batchAvatarCount = avatarOptions.filter((option) =>
    scoped.some((item) => batchAvatarSelections[item.id]?.includes(option.id)),
  ).length;
  const batchAvatarDeltas = scoped.map((item) => ({
    id: item.id,
    delta: productAvatarDelta(
      avatarOptions,
      initialBatchAvatarSelections[item.id] ?? [],
      batchAvatarSelections[item.id] ?? [],
    ),
  }));
  const batchAvatarChanged = batchAvatarDeltas.some(
    (item) => item.delta.add.length || item.delta.remove.length,
  );
  const [avatarsRefreshing, setAvatarsRefreshing] = useState(false);
  const [avatarPickerOpen, setAvatarPickerOpen] = useState(false);
  const avatarPickerShown = useRef(avatarPickerOpen);
  avatarPickerShown.current = avatarPickerOpen;
  const avatarTrigger = useRef<HTMLButtonElement>(null),
    avatarWasOpen = useRef(false);
  useLayoutEffect(() => {
    if (avatarWasOpen.current && !avatarPickerOpen) avatarTrigger.current?.focus();
    avatarWasOpen.current = avatarPickerOpen;
  }, [avatarPickerOpen]);
  const avatarChanged = !equal([...selectedAvatars].sort(), [...initialAvatarSelection].sort());
  const openAvatarPicker = async () => {
    if (inFlight.current || avatarsRefreshing) return;
    setAvatarsRefreshing(true);
    setError('');
    try {
      await p.onRefreshAvatars?.();
      if (alive.current && !inFlight.current && !confirmationOpen.current)
        setAvatarPickerOpen(true);
    } catch (error) {
      if (alive.current) setError(errorCode(error));
    } finally {
      if (alive.current) setAvatarsRefreshing(false);
    }
  };
  return {
    initialAvatarOptions,
    avatarOptions,
    selectedAvatars,
    setSelectedAvatars,
    setBatchAvatarSelections,
    batchAvatarMixed,
    pickerSelection,
    batchAvatarCount,
    batchAvatarDeltas,
    batchAvatarChanged,
    avatarsRefreshing,
    avatarPickerOpen,
    setAvatarPickerOpen,
    avatarPickerShown,
    avatarTrigger,
    avatarChanged,
    openAvatarPicker,
  };
}
export type AvatarDraft = ReturnType<typeof useAvatarDraft>;
