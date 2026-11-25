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
  const [current, setCurrent] = useState<{ message: string; jobId?: string } | null>(null);
  const notify = (message: string, jobId?: string) =>
    setCurrent(message ? { message, jobId } : null);
  const clearNotice = () => setCurrent(null);
  const run = (operation: Promise<unknown>) => {
    void operation.catch((error) => notify(q(errorCode(error))));
  };
  useEffect(() => {
    if (!current) return;
    const timer = setTimeout(() => setCurrent(null), 5000);
    return () => clearTimeout(timer);
  }, [current]);
  useEffect(() => {
    if (flow || dialog) setCurrent(null);
  }, [flow, dialog]);
  return { notice: current?.message ?? '', noticeJobId: current?.jobId, clearNotice, notify, run };
}
export type Notices = ReturnType<typeof useNotice>;
