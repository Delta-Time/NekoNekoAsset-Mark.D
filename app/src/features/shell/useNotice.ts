import { useEffect, useState } from 'react';
import { errorCode } from '../../backend';

/** The status bar notice: shown for five seconds and cleared when a flow or dialog opens. `run`
 * reports a failed operation there. */
export function useNotice({
  q,
  flow,
  dialog,
}: {
  q: (key: string) => string;
  flow: string | null;
  dialog: string | null;
}) {
  const [notice, setNotice] = useState('');
  const notify = (message: string) => setNotice(message);
  const run = (operation: Promise<unknown>) => {
    void operation.catch((error) => notify(q(errorCode(error))));
  };
  useEffect(() => {
    if (!notice) return;
    const timer = setTimeout(() => setNotice(''), 5000);
    return () => clearTimeout(timer);
  }, [notice]);
  useEffect(() => {
    if (flow || dialog) setNotice('');
  }, [flow, dialog]);
  return { notice, setNotice, notify, run };
}
