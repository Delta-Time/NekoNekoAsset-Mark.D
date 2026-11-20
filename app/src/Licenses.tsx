import { useEffect, useLayoutEffect, useRef, useState } from 'react';
import type { ReactNode, RefObject } from 'react';
import { createPortal } from 'react-dom';
import type { Locale } from './model';
import { invokeAction, errorCode, openExternal } from './backend';
import { licenseText } from './license-i18n';
import { editorText } from './editor-i18n';
import { resetText } from './settings-reset-i18n';
import { nativeErrorText } from './error-i18n';
import { t } from './i18n';
import Toggle from './Toggle';
import './licenses.css';

export type LicenseCells = Record<string, string>;
const GROUPS: [string, string[]][] = [
  ['who', ['A', 'B']],
  ['upload', ['C', 'D', 'E']],
  ['sensitive', ['F', 'G', 'H']],
  ['modify', ['I', 'J', 'K', 'L']],
  ['redistribute', ['M', 'N']],
  ['media', ['O', 'P', 'Q', 'R']],
  ['derivative', ['S', 'T', 'U']],
  ['other', ['V', 'W', 'X']],
];
const LETTERS = GROUPS.flatMap(([, letters]) => letters);
const choices = (letter: string) =>
  letter === 'V' ? ['required', 'notRequired'] : letter === 'X' ? ['yes', 'no'] : ['allow', 'deny'];
const text = (locale: Locale) => (key: string) => licenseText(locale, key) ?? key;

/** Known cells and values only, as the native side keeps them. */
export function licenseCells(value: unknown): LicenseCells {
  if (!value || typeof value !== 'object' || Array.isArray(value)) return {};
  return Object.fromEntries(
    Object.entries(value).filter(
      ([letter, choice]) =>
        LETTERS.includes(letter) && typeof choice === 'string' && choices(letter).includes(choice),
    ),
  ) as LicenseCells;
}
const sameCells = (a: LicenseCells, b: LicenseCells) =>
  LETTERS.every((letter) => (a[letter] ?? '') === (b[letter] ?? ''));
export function licenseSummary(locale: Locale, cells: LicenseCells) {
  const L = text(locale);
  return LETTERS.filter((letter) => cells[letter])
    .map((letter) => `${L('cell' + letter)} ${L(cells[letter])}`)
    .join(', ');
}
export function licenseError(locale: Locale, code: string) {
  return licenseText(locale, code) ?? nativeErrorText(locale, code) ?? t(locale, 'operationFailed');
}

/** Focus stays inside the dialog; Escape closes it and focus returns to its trigger. */
function useDialog(box: RefObject<HTMLDivElement | null>, onEscape: () => void) {
  const escape = useRef(onEscape);
  escape.current = onEscape;
  useLayoutEffect(() => {
    const prior = document.activeElement as HTMLElement | null;
    const focusable = () =>
      Array.from(
        box.current?.querySelectorAll<HTMLElement>('button:not(:disabled),select:not(:disabled)') ??
          [],
      ).filter((element) => element.offsetParent !== null);
    (
      box.current?.querySelector<HTMLElement>('select:not(:disabled)') ??
      focusable()[0] ??
      box.current
    )?.focus();
    const key = (event: KeyboardEvent) => {
      if (event.isComposing || event.keyCode === 229) return;
      // Library shortcuts (undo, select all, search, zoom) stay behind the dialog.
      if (event.ctrlKey || event.metaKey) {
        event.stopPropagation();
        if (['f', 'z', 'a', '+', '=', '-', 'add', 'subtract'].includes(event.key.toLowerCase()))
          event.preventDefault();
      }
      if (event.key === 'Escape') {
        event.preventDefault();
        event.stopPropagation();
        escape.current();
      }
      if (event.key === 'Tab') {
        event.stopPropagation();
        const elements = focusable(),
          first = elements[0],
          last = elements.at(-1);
        if (
          event.shiftKey &&
          (document.activeElement === first ||
            !elements.includes(document.activeElement as HTMLElement))
        ) {
          event.preventDefault();
          last?.focus();
        } else if (
          !event.shiftKey &&
          (document.activeElement === last ||
            !elements.includes(document.activeElement as HTMLElement))
        ) {
          event.preventDefault();
          first?.focus();
        }
      }
    };
    document.addEventListener('keydown', key, true);
    return () => {
      document.removeEventListener('keydown', key, true);
      if (prior?.isConnected) prior.focus({ preventScroll: true });
    };
  }, []);
}
const portal = (node: ReactNode) =>
  createPortal(node, document.querySelector('.app') ?? document.body);

export function LicenseTable({
  locale,
  cells,
  disabled,
  onChange,
  id,
}: {
  locale: Locale;
  cells: LicenseCells;
  disabled?: boolean;
  onChange: (cells: LicenseCells) => void;
  id: string;
}) {
  const L = text(locale);
  return (
    <div className="license-grid">
      {GROUPS.map(([group, letters]) => (
        <fieldset key={group} className="license-group" disabled={disabled}>
          <legend>{L(group)}</legend>
          <div className="license-cells">
            {letters.map((letter) => {
              const value = cells[letter] ?? '',
                field = `${id}-${letter}`;
              return (
                <div className="license-cell" key={letter}>
                  <label htmlFor={field}>{L('cell' + letter)}</label>
                  <select
                    id={field}
                    value={value}
                    data-tone={
                      value === 'allow'
                        ? 'allow'
                        : value === 'deny'
                          ? 'deny'
                          : value
                            ? 'set'
                            : undefined
                    }
                    onChange={(event) => {
                      const next = { ...cells };
                      if (event.target.value) next[letter] = event.target.value;
                      else delete next[letter];
                      onChange(next);
                    }}
                  >
                    <option value="">-</option>
                    {choices(letter).map((choice) => (
                      <option key={choice} value={choice}>
                        {L(choice)}
                      </option>
                    ))}
                  </select>
                </div>
              );
            })}
          </div>
        </fieldset>
      ))}
    </div>
  );
}

/** Changes apply to the list at once; Clear empties every cell. */
export function LicenseFilterDialog({
  locale,
  value,
  onChange,
  onClose,
}: {
  locale: Locale;
  value: LicenseCells;
  onChange: (cells: LicenseCells) => void;
  onClose: () => void;
}) {
  const box = useRef<HTMLDivElement>(null),
    L = text(locale);
  useDialog(box, onClose);
  return portal(
    <div
      className="modal-backdrop"
      onMouseDown={(event) => {
        if (event.target === event.currentTarget) onClose();
      }}
    >
      <div
        className="modal license-modal"
        role="dialog"
        aria-modal="true"
        aria-labelledby="license-filter-title"
        ref={box}
        tabIndex={-1}
      >
        <header className="modal-header">
          <h2 id="license-filter-title">{L('filterLicenses')}</h2>
          <button type="button" aria-label={editorText(locale, 'close')} onClick={onClose}>
            ×
          </button>
        </header>
        <div className="modal-body">
          <LicenseTable locale={locale} cells={value} onChange={onChange} id="license-filter" />
        </div>
        <footer className="modal-footer">
          <button
            type="button"
            className="license-footer-start"
            disabled={!Object.keys(value).length}
            onClick={() => onChange({})}
          >
            {L('clear')}
          </button>
          <button type="button" className="primary" onClick={onClose}>
            {L('done')}
          </button>
        </footer>
      </div>
    </div>,
  );
}

interface LicenseDetail {
  synced: {
    kind: string;
    version?: string | null;
    source?: string | null;
    cells: LicenseCells;
  } | null;
  edited: LicenseCells | null;
}
/** Opens on the asset's current table: this Edit's draft, else the saved edit, else the synced one.
 *  Done reports `null` when the table matches the synced one, so saving removes any edit. */
export function LicenseEditor({
  locale,
  productId,
  draft,
  onDone,
  onClose,
}: {
  locale: Locale;
  productId: string;
  draft: LicenseCells | null | undefined;
  onDone: (cells: LicenseCells | null) => void;
  onClose: () => void;
}) {
  const box = useRef<HTMLDivElement>(null),
    L = text(locale);
  useDialog(box, onClose);
  const [detail, setDetail] = useState<LicenseDetail | null>(null),
    [cells, setCells] = useState<LicenseCells>({}),
    [initial, setInitial] = useState<LicenseCells>({}),
    [failed, setFailed] = useState(false),
    [error, setError] = useState('');
  useEffect(() => {
    let alive = true;
    void invokeAction<{ synced?: LicenseDetail['synced']; edited?: unknown }>('licenses', 'get', {
      productId,
    })
      .then((value) => {
        if (!alive) return;
        const synced = value?.synced
            ? { ...value.synced, cells: licenseCells(value.synced.cells) }
            : null,
          edited = value?.edited ? licenseCells(value.edited) : null;
        const opened =
          draft === undefined ? (edited ?? synced?.cells ?? {}) : (draft ?? synced?.cells ?? {});
        setDetail({ synced, edited });
        setCells(opened);
        setInitial(opened);
      })
      .catch((failure) => {
        if (alive) {
          setError(licenseError(locale, errorCode(failure)));
          setFailed(true);
        }
      });
    return () => {
      alive = false;
    };
  }, [productId]);
  useEffect(() => {
    if (
      detail &&
      (!box.current?.contains(document.activeElement) ||
        document.activeElement?.closest('.modal-header'))
    )
      box.current?.querySelector<HTMLSelectElement>('select')?.focus();
  }, [detail]);
  const synced = detail?.synced?.cells ?? {},
    source = detail?.synced,
    kind =
      source?.kind === 'vn3'
        ? ['VN3', source.version].filter(Boolean).join(' ')
        : source?.kind === 'uv'
          ? 'UV License'
          : source?.kind === 'cc0'
            ? 'CC0'
            : '';
  return portal(
    <div
      className="modal-backdrop"
      onMouseDown={(event) => {
        if (event.target === event.currentTarget) onClose();
      }}
    >
      <div
        className="modal license-modal"
        role="dialog"
        aria-modal="true"
        aria-labelledby="license-editor-title"
        aria-busy={!detail && !failed}
        ref={box}
        tabIndex={-1}
      >
        <header className="modal-header">
          <h2 id="license-editor-title">{L('editLicense')}</h2>
          <button type="button" aria-label={editorText(locale, 'close')} onClick={onClose}>
            ×
          </button>
        </header>
        <div className="modal-body">
          {(kind || (detail && !sameCells(cells, synced))) && (
            <p className="license-source">
              {kind && <span>{kind}</span>}
              {source?.source && (
                <button
                  type="button"
                  className="license-link"
                  onClick={() =>
                    void openExternal(source.source!).catch((failure) =>
                      setError(licenseError(locale, errorCode(failure))),
                    )
                  }
                >
                  {L('openSource')}
                </button>
              )}
              {detail && !sameCells(cells, synced) && (
                <span className="license-edited">{L('edited')}</span>
              )}
            </p>
          )}
          {error && (
            <div className="callout error" role="alert">
              {error}
            </div>
          )}
          <LicenseTable
            locale={locale}
            cells={cells}
            disabled={!detail}
            onChange={setCells}
            id="license-editor"
          />
        </div>
        <footer className="modal-footer">
          <button
            type="button"
            className="license-footer-start"
            disabled={!detail || sameCells(cells, synced)}
            onClick={() => setCells(synced)}
          >
            {L('restore')}
          </button>
          <button type="button" onClick={onClose}>
            {t(locale, 'cancel')}
          </button>
          <button
            type="button"
            className="primary"
            disabled={!detail}
            onClick={() => {
              if (sameCells(cells, initial)) onClose();
              else onDone(sameCells(cells, synced) ? null : cells);
            }}
          >
            {L('done')}
          </button>
        </footer>
      </div>
    </div>,
  );
}

/** Features section: the switch and, while it is on, what can be read, sync all and the red delete. */
export function LicenseSettings({
  locale,
  enabled,
  busy,
  job,
  onToggle,
  onNotice,
}: {
  locale: Locale;
  enabled: boolean;
  busy: boolean;
  job?: { state: string; progress?: unknown };
  onToggle: (enabled: boolean) => void;
  onNotice: (text: string) => void;
}) {
  const L = text(locale),
    [exists, setExists] = useState(false),
    [pending, setPending] = useState(false),
    [confirming, setConfirming] = useState(false),
    trigger = useRef<HTMLButtonElement>(null);
  const running = !!job && ['queued', 'running'].includes(job.state);
  useEffect(() => {
    if (!enabled) return;
    let alive = true;
    void invokeAction<{ exists?: boolean }>('licenses', 'status')
      .then((status) => {
        if (alive) setExists(status?.exists === true);
      })
      .catch(() => {});
    return () => {
      alive = false;
    };
  }, [enabled, job?.state, confirming]);
  const syncAll = async () => {
    setPending(true);
    try {
      const result = await invokeAction<{ jobId?: string | null }>('licenses', 'sync', {});
      if (!result?.jobId) onNotice(L('noLicenseWork'));
    } catch (failure) {
      onNotice(licenseError(locale, errorCode(failure)));
    } finally {
      setPending(false);
    }
  };
  return (
    <section className="wf-section license-settings">
      <Toggle label={L('licenseFilter')} checked={enabled} disabled={busy} onChange={onToggle} />
      {enabled && <small className="license-hint">{L('licenseHint')}</small>}
      {enabled && (
        <div className="license-actions">
          <button type="button" disabled={pending || running} onClick={() => void syncAll()}>
            {L('syncAllLicenses')}
            {running && typeof job?.progress === 'number' ? ` · ${job.progress}%` : ''}
          </button>
          <button
            type="button"
            ref={trigger}
            className="danger"
            disabled={pending || !exists}
            onClick={() => setConfirming(true)}
          >
            {L('deleteLicenses')}
          </button>
        </div>
      )}
      {confirming && (
        <LicenseDeleteConfirmation
          locale={locale}
          onClose={(deleted) => {
            if (deleted) setExists(false);
            setConfirming(false);
            requestAnimationFrame(() => {
              const section = trigger.current?.closest('.license-settings');
              (deleted
                ? (section?.querySelector<HTMLElement>('.license-actions button:not(:disabled)') ??
                  section?.querySelector<HTMLElement>('[role=switch]'))
                : trigger.current
              )?.focus({ preventScroll: true });
            });
          }}
        />
      )}
    </section>
  );
}

/** Like the reset confirmations: starts on Cancel, Delete waits five seconds. */
function LicenseDeleteConfirmation({
  locale,
  onClose,
}: {
  locale: Locale;
  onClose: (deleted: boolean) => void;
}) {
  const L = text(locale),
    [seconds, setSeconds] = useState(5),
    [busy, setBusy] = useState(false),
    [error, setError] = useState('');
  const deadline = useRef(performance.now() + 5000),
    box = useRef<HTMLDivElement>(null),
    cancel = useRef<HTMLButtonElement>(null),
    inFlight = useRef(false);
  useDialog(box, () => {
    if (!inFlight.current) onClose(false);
  });
  useEffect(() => {
    const timer = setInterval(
      () => setSeconds(Math.max(0, Math.ceil((deadline.current - performance.now()) / 1000))),
      100,
    );
    return () => clearInterval(timer);
  }, []);
  useLayoutEffect(() => {
    if (busy) box.current?.focus();
    else cancel.current?.focus();
  }, [busy]);
  const confirm = async () => {
    if (inFlight.current || performance.now() < deadline.current) return;
    inFlight.current = true;
    setBusy(true);
    setError('');
    try {
      await invokeAction('licenses', 'delete');
      onClose(true);
    } catch (failure) {
      setError(licenseError(locale, errorCode(failure)));
      inFlight.current = false;
      setBusy(false);
    }
  };
  return portal(
    <div
      className="modal-backdrop reset-app-backdrop"
      onMouseDown={(event) => {
        if (event.target === event.currentTarget && !inFlight.current) onClose(false);
      }}
    >
      <div
        className="modal reset-app-modal"
        role="alertdialog"
        aria-modal="true"
        aria-labelledby="license-delete-title"
        aria-describedby="license-delete-message"
        aria-busy={busy}
        tabIndex={-1}
        ref={box}
      >
        <header className="modal-header">
          <h2 id="license-delete-title">{L('deleteLicenses')}</h2>
        </header>
        <div className="modal-body">
          <p id="license-delete-message">{L('deleteLicensesMessage')}</p>
          {error && (
            <div className="callout error" role="alert">
              {error}
            </div>
          )}
        </div>
        <footer className="modal-footer">
          <button ref={cancel} disabled={busy} onClick={() => onClose(false)}>
            {t(locale, 'cancel')}
          </button>
          <button
            disabled={busy || seconds > 0}
            className={seconds === 0 ? 'danger reset-app-confirm' : undefined}
            onClick={() => void confirm()}
          >
            {seconds > 0
              ? (resetText(locale, 'wait') ?? '').replace('{seconds}', String(seconds))
              : L('deleteLicenses')}
          </button>
        </footer>
      </div>
    </div>,
  );
}
