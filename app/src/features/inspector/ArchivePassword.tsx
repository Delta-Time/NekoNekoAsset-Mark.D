import { useState } from 'react';
import { Check, X } from 'lucide-react';
import { errorCode } from '../../backend';
/** A password for a file's archive: Enter extracts with it, Escape closes. */
export default function ArchivePassword({
  fileName,
  manual,
  L,
  onExtract,
  onClose,
}: {
  fileName: string;
  manual: boolean;
  L: (key: string) => string;
  onExtract: (password: string, save: boolean) => Promise<void>;
  onClose: () => void;
}) {
  const [password, setPassword] = useState('');
  const [save, setSave] = useState(true);
  const [error, setError] = useState('');
  const [pending, setPending] = useState(false);
  const extract = async () => {
    if (!password || pending) return;
    setPending(true);
    setError('');
    try {
      await onExtract(password, manual || save);
    } catch (cause) {
      setError(errorCode(cause));
    } finally {
      setPending(false);
    }
  };
  // Focus goes back to the row's key, which stays when the field closes unsent.
  const close = (from: HTMLElement) => {
    from
      .closest('.asset-inspector-file')
      ?.querySelector<HTMLButtonElement>('.asset-inspector-file-unlock')
      ?.focus();
    onClose();
  };
  return (
    <div className="asset-inspector-file-password">
      {error && <small role="alert">{L(error)}</small>}
      <input
        type="password"
        autoFocus
        autoComplete="off"
        spellCheck={false}
        maxLength={256}
        placeholder={L('enterArchivePassword')}
        aria-label={L('enterArchivePassword') + ' ' + fileName}
        value={password}
        onChange={(event) => setPassword(event.target.value)}
        onKeyDown={(event) => {
          if (event.nativeEvent.isComposing) return;
          if (event.key === 'Escape') {
            event.preventDefault();
            event.stopPropagation();
            close(event.currentTarget);
          } else if (event.key === 'Enter') {
            event.preventDefault();
            extract();
          }
        }}
      />
      {!manual && (
        <label>
          <input
            type="checkbox"
            checked={save}
            disabled={pending}
            onChange={(event) => setSave(event.target.checked)}
          />
          {L('saveArchivePassword')}
        </label>
      )}
      <button
        type="button"
        title={L(manual ? 'saveArchivePassword' : 'extractArchive')}
        aria-label={L(manual ? 'saveArchivePassword' : 'extractArchive') + ' ' + fileName}
        disabled={!password || pending}
        onClick={extract}
      >
        <Check size={13} />
      </button>
      <button
        type="button"
        title={L('cancel')}
        aria-label={L('cancel')}
        onClick={(event) => close(event.currentTarget)}
      >
        <X size={13} />
      </button>
    </div>
  );
}
