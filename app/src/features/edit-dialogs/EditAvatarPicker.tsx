import AvatarPicker from '../../AvatarPicker';
import type { EditSubject } from './useEditSubject';
import type { AvatarDraft } from './useAvatarDraft';

/** The compatible-avatar picker for one product, or for every batch product at once. */
export default function EditAvatarPicker({
  subject,
  avatars,
}: {
  subject: EditSubject;
  avatars: AvatarDraft;
}) {
  const { p, batch, scoped } = subject;
  const {
    avatarOptions,
    pickerSelection,
    batchAvatarMixed,
    setBatchAvatarSelections,
    setSelectedAvatars,
    setAvatarPickerOpen,
  } = avatars;
  return (
    <AvatarPicker
      locale={p.locale}
      options={avatarOptions}
      value={pickerSelection}
      mixed={batch ? batchAvatarMixed : undefined}
      label={(id) => {
        const option = avatarOptions.find((option) => option.id === id);
        return option && 'legacyValue' in option
          ? String(option.legacyValue)
          : p.avatarLabel?.(id) || option?.displayName || option?.officialName || id;
      }}
      onChange={(ids, selected) => {
        if (batch)
          setBatchAvatarSelections((current) =>
            Object.fromEntries(
              scoped.map((item) => {
                const values = current[item.id] ?? [];
                return [
                  item.id,
                  selected
                    ? [...new Set([...values, ...ids])]
                    : values.filter((id) => !ids.includes(id)),
                ];
              }),
            ),
          );
        else
          setSelectedAvatars((current) =>
            selected
              ? [...new Set([...current, ...ids])]
              : current.filter((id) => !ids.includes(id)),
          );
      }}
      onClose={() => setAvatarPickerOpen(false)}
    />
  );
}
