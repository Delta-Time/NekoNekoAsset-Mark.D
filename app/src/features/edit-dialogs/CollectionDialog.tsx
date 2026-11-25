import type { EditDialogsProps } from '../../EditDialogs';
import { editorText } from '../../editor-i18n';
import { useState, useRef, useEffect } from 'react';
import { focusable } from './parts';
import { normalizeSearch } from '../../model';
import { errorCode } from '../../backend';
import { Plus } from 'lucide-react';
import { collectionMembership } from '../../collections';
import { contentLang } from '../../content-lang';

/** Add to collection: create collections, and add the products selected when it opened to each one
 * or remove them. */
export function CollectionDialog(p: EditDialogsProps) {
  const L = (key: string) => editorText(p.locale, key);
  // Removing members from the active filter may change the library selection.
  // Keep this popup's targets fixed until it closes.
  const [ids] = useState(() => [...new Set(p.selected)]);
  const [products] = useState(() => {
    const selected = new Set(ids);
    return p.products.filter((product) => selected.has(product.id));
  });
  const [name, setName] = useState(''),
    [error, setError] = useState(''),
    [creating, setCreating] = useState(false);
  const inFlight = useRef(false),
    alive = useRef(true),
    box = useRef<HTMLDivElement>(null),
    close = useRef(p.onClose);
  close.current = p.onClose;
  useEffect(() => {
    alive.current = true;
    return () => {
      alive.current = false;
    };
  }, []);
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
      if (e.isComposing || e.keyCode === 229) return;
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
  const create = async () => {
    if (inFlight.current || !p.onCreateCollection) return;
    const trimmed = name.trim();
    if (!trimmed) {
      setError('nameRequired');
      return;
    }
    if (
      Object.keys(p.collections ?? {}).some(
        (value) => normalizeSearch(value) === normalizeSearch(trimmed),
      )
    ) {
      setError('collectionExists');
      return;
    }
    inFlight.current = true;
    setCreating(true);
    setError('');
    try {
      await p.onCreateCollection(trimmed);
      if (alive.current) {
        setName('');
        requestAnimationFrame(() => {
          if (
            document.activeElement === document.body ||
            box.current?.querySelector('form')?.contains(document.activeElement)
          )
            box.current?.querySelector<HTMLInputElement>('input')?.focus();
        });
      }
    } catch (error) {
      if (alive.current) setError(errorCode(error));
    } finally {
      inFlight.current = false;
      if (alive.current) setCreating(false);
    }
  };
  const change = async (name: string, included: boolean) => {
    setError('');
    try {
      await p.onCollection(name, ids, included);
    } catch (error) {
      if (alive.current) setError(errorCode(error));
    }
  };
  return (
    <div
      className="modal-backdrop"
      onMouseDown={(e) => {
        if (e.target === e.currentTarget) p.onClose();
      }}
    >
      <div
        className="modal editor-modal editor-collection-modal"
        role="dialog"
        aria-modal="true"
        aria-busy={p.collectionBusy || creating}
        aria-labelledby="editor-collection-title"
        ref={box}
      >
        <header className="modal-header">
          <h2 id="editor-collection-title">{L('manageCollections')}</h2>
          <button type="button" aria-label={L('close')} onClick={p.onClose}>
            ×
          </button>
        </header>
        <div className="modal-body">
          <form
            onSubmit={(e) => {
              e.preventDefault();
              void create();
            }}
          >
            <label className="field" htmlFor="editor-new-collection">
              <span>{L('newCollection')}</span>
            </label>
            <div className="editor-collection-create">
              <input
                id="editor-new-collection"
                disabled={creating}
                value={name}
                onChange={(e) => {
                  setName(e.target.value);
                  setError('');
                }}
                onKeyDown={(e) => {
                  if (e.key === 'Enter' && (e.nativeEvent.isComposing || e.keyCode === 229))
                    e.preventDefault();
                }}
              />
              <button
                type="submit"
                className="primary"
                title={L('createCollection')}
                aria-label={L('createCollection')}
                aria-busy={creating}
                disabled={creating || !name.trim() || !p.onCreateCollection}
              >
                <Plus size={18} />
              </button>
            </div>
          </form>
          {Object.keys(p.collections ?? {}).length > 0 && (
            <>
              <div className="editor-collection-heading" id="editor-collections-label">
                {L('existingCollections')}
              </div>
              <div
                className="editor-collections"
                role="group"
                aria-labelledby="editor-collections-label"
              >
                {Object.entries(p.collections ?? {}).map(([name, members]) => {
                  const state = collectionMembership(members, ids);
                  return (
                    <label key={name}>
                      <input
                        type="checkbox"
                        checked={state.checked}
                        aria-checked={state.mixed ? 'mixed' : state.checked}
                        ref={(input) => {
                          if (input) input.indeterminate = state.mixed;
                        }}
                        disabled={!ids.length}
                        title={state.mixed ? L('mixed') : undefined}
                        onChange={() => void change(name, !state.checked)}
                      />
                      <span>{name}</span>
                      <small className="muted">{members.length}</small>
                    </label>
                  );
                })}
              </div>
            </>
          )}
          <div className="editor-selected-products">
            <div className="data-row" id="editor-selected-products-label">
              <span>{L('selected')}</span>
              <span>{ids.length}</span>
            </div>
            <ul tabIndex={0} aria-labelledby="editor-selected-products-label">
              {products.map((product) => (
                <li key={product.id} lang={contentLang(product.title)}>
                  {product.title}
                </li>
              ))}
            </ul>
          </div>
          {error && (
            <div className="callout error" role="alert">
              {['nameRequired', 'collectionExists'].includes(error)
                ? L(error)
                : L('collectionActionFailed')}
            </div>
          )}
        </div>
        <footer className="modal-footer">
          <button type="button" onClick={p.onClose}>
            {L('close')}
          </button>
        </footer>
      </div>
    </div>
  );
}
