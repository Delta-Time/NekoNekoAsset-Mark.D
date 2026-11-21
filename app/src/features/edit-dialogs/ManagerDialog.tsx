import type { EditDialogsProps } from '../../EditDialogs';
import { editorText } from '../../editor-i18n';
import { useState, useRef, useEffect } from 'react';
import { focusable, operationMessage } from './parts';
import { normalizeSearch } from '../../model';
import { errorCode } from '../../backend';
import { Check, X, Pencil, Trash2 } from 'lucide-react';
import ConfirmDelete from '../../ConfirmDelete';

/** Manage collections, saved searches or custom categories: add, rename and delete them. */
export function ManagerDialog(p: EditDialogsProps) {
  const L = (key: string) => editorText(p.locale, key),
    collections = p.dialog === 'manageCollections',
    categories = p.dialog === 'manageCategories';
  const names = collections
    ? Object.keys(p.collections ?? {})
    : categories
      ? (p.customCategories ?? []).map((category) => category.name)
      : (p.savedSearches ?? []).map((search) => search.name);
  const entryId = (index: number, value: string) =>
    collections
      ? p.collectionIds?.[value]
      : categories
        ? p.customCategories?.[index]?.id
        : p.savedSearches?.[index]?.id;
  const [removeTarget, setRemoveTarget] = useState<{
    id?: string;
    name: string;
    index: number;
  } | null>(null);
  const confirming = useRef(false);
  confirming.current = !!removeTarget;
  const [name, setName] = useState(''),
    [editing, setEditing] = useState<number | null>(null),
    [renamed, setRenamed] = useState(''),
    [error, setError] = useState(''),
    [busy, setBusy] = useState(false),
    [pendingAction, setPendingAction] = useState<'add' | 'rename' | 'remove' | null>(null);
  const inFlight = useRef(false),
    alive = useRef(true);
  useEffect(() => {
    alive.current = true;
    return () => {
      alive.current = false;
    };
  }, []);
  const pending = !collections && busy;
  const requestClose = () => {
    if (!confirming.current && (collections || !inFlight.current)) p.onClose();
  };
  const box = useRef<HTMLDivElement>(null),
    close = useRef(requestClose);
  close.current = requestClose;
  useEffect(() => {
    if (!error) return;
    const frame = requestAnimationFrame(() =>
      box.current?.querySelector('[role="alert"]')?.scrollIntoView({ block: 'nearest' }),
    );
    return () => cancelAnimationFrame(frame);
  }, [error]);
  useEffect(() => {
    const prior = document.activeElement as HTMLElement | null;
    box.current?.querySelector<HTMLInputElement>('input')?.focus();
    const key = (e: KeyboardEvent) => {
      if (e.isComposing || e.keyCode === 229 || confirming.current) return;
      if (e.key === 'Escape') {
        e.preventDefault();
        e.stopPropagation();
        close.current();
      }
      if (e.key === 'Tab') {
        const elements = focusable(box.current),
          first = elements[0],
          last = elements[elements.length - 1];
        if (
          e.shiftKey &&
          (document.activeElement === first ||
            !elements.includes(document.activeElement as HTMLElement))
        ) {
          e.preventDefault();
          last?.focus();
        } else if (
          !e.shiftKey &&
          (document.activeElement === last ||
            !elements.includes(document.activeElement as HTMLElement))
        ) {
          e.preventDefault();
          first?.focus();
        }
      }
    };
    document.addEventListener('keydown', key, true);
    return () => {
      document.removeEventListener('keydown', key, true);
      if (prior?.isConnected) prior.focus();
    };
  }, []);
  // A finished or cancelled edit removes the focused control; return to the row still being renamed, else the name field.
  useEffect(() => {
    if (busy || confirming.current || !box.current || box.current.contains(document.activeElement))
      return;
    (
      box.current.querySelector<HTMLInputElement>('.editor-manager-row input') ??
      box.current.querySelector<HTMLInputElement>('.editor-manager-add input')
    )?.focus();
  }, [busy, editing]);
  const validate = (value: string, skip = -1) => {
    if (!value.trim()) {
      setError('nameRequired');
      return false;
    }
    if (
      names.some(
        (other, index) => index !== skip && normalizeSearch(other) === normalizeSearch(value),
      )
    ) {
      setError('nameExists');
      return false;
    }
    setError('');
    return true;
  };
  const run = async (
    action: () => void | Promise<void>,
    done: () => void,
    pending: 'add' | 'rename' | 'remove',
  ) => {
    if (collections) {
      setError('');
      try {
        const result = action();
        done();
        void Promise.resolve(result).catch((error) => {
          if (alive.current) setError(errorCode(error));
        });
      } catch (error) {
        setError(errorCode(error));
      }
      return;
    }
    if (inFlight.current) return;
    inFlight.current = true;
    setBusy(true);
    setPendingAction(pending);
    setError('');
    try {
      await action();
      if (alive.current) done();
    } catch (error) {
      if (alive.current) setError(errorCode(error));
    } finally {
      inFlight.current = false;
      if (alive.current) {
        setBusy(false);
        setPendingAction(null);
      }
    }
  };
  const add = async () => {
    if (!validate(name) || (collections && !p.onCreateCollection)) return;
    await run(
      () =>
        collections
          ? p.onCreateCollection?.(name.trim())
          : categories
            ? p.onCreateCategory?.(name.trim())
            : p.onSaveSearch(name.trim()),
      () => setName(''),
      'add',
    );
  };
  const rename = async (index: number) => {
    if (!validate(renamed, index)) return;
    const id = entryId(index, names[index]);
    await run(
      () =>
        collections
          ? p.onRenameCollection?.(names[index], renamed.trim())
          : categories
            ? id
              ? p.onRenameCategory?.(id, renamed.trim())
              : Promise.reject(new Error('entryUnavailable'))
            : p.onRenameSearch?.(index, renamed.trim()),
      () => setEditing(null),
      'rename',
    );
  };
  const remove = async () => {
    if (!removeTarget?.id) throw new Error('entryUnavailable');
    await (collections
      ? p.onDeleteCollection?.(removeTarget.name, removeTarget.id)
      : categories
        ? p.onDeleteCategory?.(removeTarget.id)
        : p.onDeleteSearch?.(removeTarget.index, removeTarget.id));
    if (alive.current) setEditing(null);
  };
  return (
    <>
      <div
        className="modal-backdrop"
        onMouseDown={(e) => {
          if (e.target === e.currentTarget) requestClose();
        }}
      >
        <div
          className="modal editor-modal editor-manager"
          role="dialog"
          aria-modal={!removeTarget}
          aria-hidden={!!removeTarget || undefined}
          inert={!!removeTarget}
          aria-busy={pending}
          aria-labelledby="editor-manager-title"
          ref={box}
        >
          <header className="modal-header">
            <h2 id="editor-manager-title">
              {L(
                collections
                  ? 'manageCollections'
                  : categories
                    ? 'customCategories'
                    : 'manageSearches',
              )}
            </h2>
            <button
              type="button"
              disabled={busy && !collections}
              aria-label={L('close')}
              onClick={requestClose}
            >
              ×
            </button>
          </header>
          <div className="modal-body" inert={pending}>
            <form
              className="editor-manager-add"
              onSubmit={(e) => {
                e.preventDefault();
                void add();
              }}
            >
              <input
                disabled={pending}
                aria-label={L('name')}
                placeholder={L('name')}
                value={name}
                onChange={(e) => {
                  setName(e.target.value);
                  setError('');
                }}
              />
              <button type="submit" disabled={pending || (collections && !p.onCreateCollection)}>
                {L(
                  busy && pendingAction === 'add'
                    ? collections
                      ? 'creatingEntry'
                      : 'addingEntry'
                    : 'addEntry',
                )}
              </button>
            </form>
            <div className="editor-manager-list">
              {names.map((value, index) => (
                <div
                  className="editor-manager-row"
                  key={entryId(index, value) ?? (collections ? value : index)}
                >
                  {editing === index ? (
                    <>
                      <input
                        aria-label={L('rename') + ' ' + value}
                        autoFocus
                        value={renamed}
                        onChange={(e) => setRenamed(e.target.value)}
                        onKeyDown={(e) => {
                          if (e.nativeEvent.isComposing || e.keyCode === 229) return;
                          if (e.key === 'Enter') {
                            e.preventDefault();
                            rename(index);
                          }
                        }}
                      />
                      <button
                        type="button"
                        aria-label={L('save')}
                        title={L('save')}
                        onClick={() => rename(index)}
                      >
                        <Check size={14} />
                      </button>
                      <button
                        type="button"
                        aria-label={L('cancel')}
                        title={L('cancel')}
                        onClick={() => {
                          setEditing(null);
                          setError('');
                        }}
                      >
                        <X size={14} />
                      </button>
                    </>
                  ) : (
                    <>
                      <span title={value}>{value}</span>
                      <button
                        type="button"
                        aria-label={L('rename') + ' ' + value}
                        title={L('rename')}
                        disabled={
                          collections
                            ? !p.onRenameCollection
                            : categories
                              ? !p.onRenameCategory
                              : !p.onRenameSearch
                        }
                        onClick={() => {
                          setEditing(index);
                          setRenamed(value);
                          setError('');
                        }}
                      >
                        <Pencil size={14} />
                      </button>
                      <button
                        type="button"
                        aria-label={L('delete') + ' ' + value}
                        title={L('delete')}
                        disabled={
                          collections
                            ? !p.onDeleteCollection
                            : categories
                              ? !p.onDeleteCategory
                              : !p.onDeleteSearch
                        }
                        onClick={() =>
                          setRemoveTarget({ id: entryId(index, value), name: value, index })
                        }
                      >
                        <Trash2 size={14} />
                      </button>
                    </>
                  )}
                </div>
              ))}
            </div>
            {error && (
              <div className="callout error" role="alert">
                {['nameRequired', 'nameExists'].includes(error)
                  ? L(error)
                  : operationMessage(p.locale, error)}
              </div>
            )}
          </div>
          <footer className="modal-footer">
            <button type="button" disabled={busy && !collections} onClick={requestClose}>
              {L('close')}
            </button>
          </footer>
        </div>
      </div>
      {removeTarget && (
        <ConfirmDelete
          locale={p.locale}
          kind={collections ? 'collection' : categories ? 'category' : 'search'}
          name={removeTarget.name}
          onDelete={remove}
          onClose={() => setRemoveTarget(null)}
        />
      )}
    </>
  );
}
