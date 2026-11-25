import { Field } from './parts';
import type { EditSubject } from './useEditSubject';
import type { DetailDrafts } from './useDetailDrafts';

/** Batch edit's note replacement and its three-state R-18 switch. */
export default function BatchFields({
  subject,
  details,
}: {
  subject: EditSubject;
  details: DetailDrafts;
}) {
  const { batch, L } = subject;
  const { noteChanged, setNoteChanged, note, setNote, batchRating, ratingChanged, setRating } =
    details;
  return (
    <>
      {batch && (
        <label className="form-row">
          <input
            type="checkbox"
            checked={noteChanged}
            onChange={(e) => setNoteChanged(e.target.checked)}
          />
          {L('replaceNote')}
        </label>
      )}
      {batch && (
        <Field label={L('note')}>
          <textarea
            rows={3}
            value={note}
            disabled={!noteChanged}
            placeholder={L('unchanged')}
            onChange={(e) => setNote(e.target.value)}
          />
        </Field>
      )}
      {batch && (
        <div className="editor-batch-rating">
          <span className="editor-sensitive-toggle">
            <span className="nna-toggle">
              <span id="editor-batch-sensitive-label">R-18</span>
              <button
                type="button"
                role="checkbox"
                aria-checked={batchRating === 'mixed' ? 'mixed' : batchRating === 'on'}
                aria-labelledby="editor-batch-sensitive-label"
                title={
                  ratingChanged
                    ? L(batchRating === 'on' ? 'ratingOn' : 'ratingOff')
                    : L('unchanged') + (batchRating === 'mixed' ? ' · ' + L('mixed') : '')
                }
                onClick={() => setRating(batchRating === 'on' ? 'off' : 'on')}
              >
                <span aria-hidden="true" />
              </button>
            </span>
          </span>
        </div>
      )}
    </>
  );
}
