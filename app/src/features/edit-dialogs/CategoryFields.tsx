import { Users } from 'lucide-react';
import { categoryKeys, categoryName, sourceCategoryLabel } from '../../model';
import { Field } from './parts';
import type { EditSubject } from './useEditSubject';
import type { DetailDrafts } from './useDetailDrafts';
import type { AvatarDraft } from './useAvatarDraft';

/** Category, the compatible-avatars button with its count, and the listing's own category. */
export default function CategoryFields({
  subject,
  details,
  avatars,
}: {
  subject: EditSubject;
  details: DetailDrafts;
  avatars: AvatarDraft;
}) {
  const { L, batch, scoped, p, offline, active } = subject;
  const { category, setCategory } = details;
  const {
    avatarTrigger,
    avatarPickerOpen,
    avatarsRefreshing,
    openAvatarPicker,
    batchAvatarCount,
    selectedAvatars,
  } = avatars;
  return (
    <>
      <div className="editor-category-row">
        <Field label={L('category')}>
          <select value={category} onChange={(e) => setCategory(e.target.value)}>
            {batch && (
              <option value="">
                {L('unchanged')} ·{' '}
                {new Set(scoped.map((p) => p.category)).size > 1
                  ? L('mixed')
                  : categoryName(scoped[0]?.category ?? 'unknown', p.customCategories ?? [], L)}
              </option>
            )}
            {categoryKeys.map((key) => (
              <option key={key} value={key}>
                {L(key)}
              </option>
            ))}
            {!!p.customCategories?.length && (
              <optgroup label={L('customCategories')}>
                {p.customCategories.map((entry) => (
                  <option key={entry.id} value={entry.id}>
                    {entry.name}
                  </option>
                ))}
              </optgroup>
            )}
          </select>
        </Field>
        <button
          type="button"
          className="editor-avatar-trigger"
          ref={avatarTrigger}
          title={L('compatibleAvatars')}
          aria-label={L('compatibleAvatars')}
          aria-haspopup="dialog"
          aria-expanded={avatarPickerOpen}
          aria-busy={avatarsRefreshing}
          disabled={avatarsRefreshing || (batch && !p.onBatchPatch)}
          onClick={() => void openAvatarPicker()}
        >
          <Users size={14} />
          <span>{L('editAvatars')}</span>
          {(batch ? batchAvatarCount : selectedAvatars.length) > 0 && (
            <small>{batch ? batchAvatarCount : selectedAvatars.length}</small>
          )}
        </button>
      </div>
      {!batch && !offline && active?.sourceCategory && (
        <small className="muted">
          {[...new Set((active.sources ?? []).map((source) => source.provider).filter(Boolean))]
            .map((provider) => provider + ' · ')
            .join('')}
          {L('sourceValue')}: {sourceCategoryLabel(active.sourceCategory) || L('unknown')}
        </small>
      )}
    </>
  );
}
