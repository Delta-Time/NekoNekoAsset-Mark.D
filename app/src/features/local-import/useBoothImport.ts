import { useEffect, useRef, useState } from 'react';
import { errorCode } from '../../backend';
import type { LocalImportProps } from './import-parts';
import { needsStoreSignIn } from '../workflows/jobs';

/** The BOOTH item ID of a listing link: booth.pm/items/<id>, with or without a language, or a
 * shop's own <shop>.booth.pm/items/<id>. */
function boothItemId(text: string): string | null {
  let url: URL;
  try {
    url = new URL(text.trim());
  } catch {
    return null;
  }
  const host = url.hostname.toLowerCase();
  if (
    !['https:', 'http:'].includes(url.protocol) ||
    url.username ||
    url.password ||
    !(host === 'booth.pm' || host.endsWith('.booth.pm')) ||
    ['accounts.booth.pm', 'manage.booth.pm'].includes(host)
  )
    return null;
  return url.pathname.match(/^\/(?:[a-z]{2}(?:-[a-z]{2})?\/)?items\/(\d{1,12})\/?$/i)?.[1] ?? null;
}
export const boothListing = (id: string) => `https://booth.pm/ja/items/${id}`;

/** Free BOOTH listings to add by their links, the account that takes them, and the import. */
export function useBoothImport({
  c,
  L,
  active,
}: {
  c: LocalImportProps;
  L: (key: string) => string;
  active: boolean;
}) {
  const [ids, setIds] = useState<string[]>([]);
  const [text, setText] = useState('');
  const [inputError, setInputError] = useState('');
  const [accountId, setAccountId] = useState('');
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  const lock = useRef(false);
  const jobs = c.nativeState?.jobs ?? [];
  // Free downloads need an account BOOTH still knows as signed in on this device.
  const ready = (c.nativeState?.accounts ?? []).filter(
    (account) =>
      account.store === 'BOOTH' &&
      account.status === 'connected' &&
      !jobs.some((job) => job.accountId === account.id && needsStoreSignIn(job, account)),
  );
  const account = ready.find((item) => item.id === accountId) ?? ready[0];
  useEffect(() => {
    if (!active) setInputError('');
  }, [active]);
  /** Adds every link in the field, several pasted at once included, and answers the list;
   * anything that is not a BOOTH listing stays in the field. */
  function add(): string[] | null {
    const found: string[] = [];
    const rest = text
      .split(/\s+/)
      .filter(Boolean)
      .filter((token) => {
        const id = boothItemId(token);
        if (id) found.push(id);
        return !id;
      });
    const next = [...new Set([...ids, ...found])];
    setIds(next);
    setText(rest.join(' '));
    setInputError(rest.length ? 'boothListingRequired' : '');
    return rest.length ? null : next;
  }
  function remove(id: string) {
    setIds((previous) => previous.filter((item) => item !== id));
  }
  async function submit() {
    if (lock.current || !account) return;
    // A link left in the field joins the list, as if Add had been pressed.
    const listed = add();
    if (!listed?.length) return;
    lock.current = true;
    setBusy(true);
    setError('');
    try {
      if (!c.onNative) throw new Error('nativeUnavailable');
      const result = await c.onNative('booth', 'import_free', {
        accountId: account.id,
        productIds: listed,
      });
      if (typeof result?.jobId !== 'string') throw new Error('operationFailed');
      c.onDirtyChange?.(false);
      c.onNotice(L('importQueued'));
      c.onClose();
    } catch (failure) {
      setError(errorCode(failure));
    } finally {
      lock.current = false;
      setBusy(false);
    }
  }
  return {
    ids,
    text,
    setText,
    inputError,
    ready,
    account,
    setAccountId,
    busy,
    error,
    add,
    remove,
    submit,
    dirty: ids.length > 0 || !!text.trim(),
  };
}
export type BoothImport = ReturnType<typeof useBoothImport>;
