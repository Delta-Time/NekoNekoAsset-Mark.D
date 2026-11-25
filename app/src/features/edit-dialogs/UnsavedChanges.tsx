import type { DialogFrame } from './useDialogFrame';
import type { ThemeDraft } from './useThemeDraft';
import type { EditSubject } from './useEditSubject';
import type { EditSave } from './useEditSave';
import type { CloseGuard } from './useCloseGuard';

/** Save, Discard or Stay when closing with unsaved changes. */
export default function UnsavedChanges({
  frame,
  themeDraft,
  subject,
  editSave,
  closeGuard,
}: {
  frame: DialogFrame;
  themeDraft: ThemeDraft;
  subject: EditSubject;
  editSave: EditSave;
  closeGuard: CloseGuard;
}) {
  const { inFlight, setConfirmClose, confirmation, busy, stayButton } = frame;
  const { themeDialogStyle } = themeDraft;
  const { L } = subject;
  const { save } = editSave;
  const { close } = closeGuard;
  return (
    <div
      className="modal-backdrop editor-confirm-backdrop"
      onMouseDown={(e) => {
        if (e.target === e.currentTarget && !inFlight.current) setConfirmClose(false);
      }}
    >
      <div
        className="modal editor-confirm"
        role="alertdialog"
        aria-modal="true"
        aria-labelledby="editor-unsaved-title"
        ref={confirmation}
        style={themeDialogStyle}
      >
        <div className="modal-header">
          <h2 id="editor-unsaved-title">{L('unsavedChanges')}</h2>
        </div>
        <div className="modal-footer">
          <button type="button" disabled={busy} onClick={() => void save()}>
            {L('save')}
          </button>
          <button type="button" disabled={busy} onClick={close}>
            {L('discard')}
          </button>
          <button
            type="button"
            disabled={busy}
            className="primary"
            ref={stayButton}
            onClick={() => setConfirmClose(false)}
          >
            {L('stay')}
          </button>
        </div>
      </div>
    </div>
  );
}
