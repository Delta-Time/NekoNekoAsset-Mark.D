import { getEffectiveTags } from '../../model';
import TagPicker from '../../TagPicker';
import { Field } from './parts';
import type { EditSubject } from './useEditSubject';
import type { TagDraft } from './useTagDraft';

/** The product's tags, or batch edit's choice to add or remove tags and which ones. */
export default function TagFields({
  subject,
  tagDraft,
}: {
  subject: EditSubject;
  tagDraft: TagDraft;
}) {
  const { batch, L, p, scoped } = subject;
  const {
    tagMode,
    setTagMode,
    setTagDelta,
    setPendingDelta,
    tagDelta,
    suggestions,
    aliases,
    tags,
    updateTags,
    setPendingTag,
  } = tagDraft;
  return batch ? (
    <>
      <Field label={L('tags')}>
        <select
          value={tagMode}
          onChange={(e) => {
            setTagMode(e.target.value);
            setTagDelta([]);
            setPendingDelta('');
          }}
        >
          {['unchanged', 'addTag', 'removeTag'].map((key) => (
            <option key={key} value={key}>
              {L(key)}
            </option>
          ))}
        </select>
      </Field>
      {tagMode !== 'unchanged' && (
        <TagPicker
          key={tagMode}
          locale={p.locale}
          label={L(tagMode)}
          value={tagDelta}
          suggestions={
            tagMode === 'removeTag'
              ? [...new Set(scoped.flatMap((item) => getEffectiveTags(item)))]
              : suggestions
          }
          aliases={aliases}
          onChange={setTagDelta}
          onDraftChange={setPendingDelta}
        />
      )}
    </>
  ) : (
    <TagPicker
      locale={p.locale}
      value={tags}
      suggestions={suggestions}
      aliases={aliases}
      onChange={updateTags}
      onDraftChange={setPendingTag}
    />
  );
}
