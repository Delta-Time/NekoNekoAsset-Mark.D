import { type Ctx, useAction, native, Panel, Status } from './parts';
import { accounts } from './Accounts';
import { isStoreJob, isActive, needsStoreSignIn, isStoreSyncJob, syncProgressLabel } from './jobs';
import type { WorkflowAccount } from '../../Workflows';
import { useState, useRef, useEffect } from 'react';
import { errorCode } from '../../backend';
import type { Job } from '../../backend';
import { coalescedRefresh } from '../../refresh';

export function Sync(c: Ctx) {
  const { L } = c;
  const task = useAction();
  const rows = accounts(c);
  const historyJobs = c.nativeState?.jobs ?? [];
  const jobs = historyJobs.filter((j) => isStoreJob(j.type) && isActive(j));
  const needsLogin = (account: WorkflowAccount) =>
    account.status !== 'connected' ||
    historyJobs.some((job) => job.accountId === account.id && needsStoreSignIn(job, account));
  const available = rows.filter(
    (a) => !needsLogin(a) && !historyJobs.some((job) => job.accountId === a.id && isActive(job)),
  );
  const [startErrors, setStartErrors] = useState<Record<string, string>>({});
  const begin = (ids: string[], mode: string) =>
    task.run(async () => {
      setStartErrors((previous) =>
        Object.fromEntries(Object.entries(previous).filter(([id]) => !ids.includes(id))),
      );
      for (const accountId of ids) {
        try {
          await native(c, 'booth', 'sync', { accountId, source: 'all', mode });
        } catch (error) {
          setStartErrors((previous) => ({ ...previous, [accountId]: errorCode(error) }));
        }
      }
    });
  const stamp = (value: unknown) => {
    if (typeof value !== 'string' || !Number.isFinite(Date.parse(value))) return L('never');
    return new Date(value).toLocaleString(c.locale, {
      year: 'numeric',
      month: '2-digit',
      day: '2-digit',
      hour: '2-digit',
      minute: '2-digit',
      second: '2-digit',
    });
  };
  const [accountCounts, setAccountCounts] = useState<{
      accounts: Record<string, number>;
      unassigned: number;
    } | null>(null),
    [countError, setCountError] = useState('');
  const countMounted = useRef(false),
    countContext = useRef(c),
    countRequest = useRef<(() => Promise<void>) | null>(null);
  countContext.current = c;
  if (!countRequest.current)
    countRequest.current = coalescedRefresh(async () => {
      try {
        const result = await native(countContext.current, 'catalogue', 'account_counts');
        if (countMounted.current) {
          setAccountCounts(result);
          setCountError('');
        }
      } catch (error) {
        if (countMounted.current) setCountError(errorCode(error));
      }
    });
  const accountKey = JSON.stringify(rows.map((account) => account.id).sort());
  useEffect(() => {
    countMounted.current = true;
    void countRequest.current!();
    return () => {
      countMounted.current = false;
    };
  }, [accountKey, c.catalogueVersion]);
  const unassigned = accountCounts?.unassigned ?? 0;
  const count = (value: number) =>
    value.toLocaleString(c.locale) + ' ' + L(value === 1 ? 'assetSingular' : 'assetCount');
  return (
    <Panel
      actions={
        <>
          <button
            className="primary"
            disabled={task.busy || !available.length}
            onClick={() =>
              begin(
                available.map((a) => a.id),
                'incremental',
              )
            }
          >
            {L('syncAll')}
          </button>
          <button
            disabled={task.busy || !available.length}
            onClick={() =>
              begin(
                available.map((a) => a.id),
                'forced',
              )
            }
          >
            {L('fullResyncAll')}
          </button>
        </>
      }
    >
      <div className="wf-sync-accounts">
        {rows.map((a) => {
          const coverage = (c.nativeState?.sourceCoverage ?? []).filter(
            (row) => row.accountId === a.id,
          );
          const history = (c.nativeState?.jobs ?? [])
            .filter((job) => job.accountId === a.id && isStoreSyncJob(job.type))
            .sort((a, b) => String(a.updatedAt ?? '').localeCompare(String(b.updatedAt ?? '')));
          const completed = history
            .filter((job) => job.state === 'completed' && /sync$/i.test(job.type))
            .at(-1);
          const success =
            a.lastSyncAt ??
            a.lastSync ??
            completed?.completedAt ??
            completed?.updatedAt ??
            (coverage.length && coverage.every((row) => row.status === 'complete')
              ? coverage
                  .map((row) => row.lastSuccessfulAt ?? row.lastSyncAt ?? row.completedAt)
                  .filter((v): v is string => typeof v === 'string')
                  .sort()
                  .at(-1)
              : undefined);
          const job = jobs.find((j) => j.accountId === a.id);
          const busy = historyJobs.some((j) => j.accountId === a.id && isActive(j));
          const signInRequired = needsLogin(a);
          const latest = history.at(-1);
          const settled = !job && latest?.state === 'completed';
          const left =
            settled && latest.progress && typeof latest.progress === 'object'
              ? (latest.progress.incompleteProducts ?? 0)
              : 0;
          const error = job
            ? undefined
            : latest && ['failed', 'needsAction'].includes(latest.state)
              ? latest
              : coverage.find((row) =>
                  [
                    'failed',
                    'auth-required',
                    'unsupported',
                    ...(settled ? [] : ['incomplete']),
                  ].includes(String(row.status)),
                );
          return (
            <div className="wf-sync-account" key={a.id}>
              <div className="wf-sync-identity">
                <strong title={a.name ?? undefined}>
                  {a.store} · {a.name}
                </strong>
                <small>
                  {L('lastSync')} · {stamp(success)}
                </small>
                {job && (
                  <small role="status">
                    {isStoreSyncJob(job.type)
                      ? syncProgressLabel(c.locale, job)
                      : L('signIn') + ' · ' + L(job.state)}
                  </small>
                )}
                {a.status !== 'connected' && <small>{L(a.status)}</small>}
                {error && (
                  <small>
                    {L(
                      String(
                        error.errorCode ??
                          ('status' in error ? error.status : (error as Job).state),
                      ),
                    )}
                  </small>
                )}
                {left > 0 && (
                  <small>
                    {L('syncIncompleteAssets')} {left.toLocaleString(c.locale)}
                  </small>
                )}
                {startErrors[a.id] && (
                  <small className="wf-error" role="alert">
                    {L(startErrors[a.id])}
                  </small>
                )}
              </div>
              <div className="wf-sync-action">
                <span>{accountCounts ? count(accountCounts.accounts[a.id] ?? 0) : '—'}</span>
                <button
                  disabled={task.busy || busy}
                  onClick={() =>
                    signInRequired
                      ? task.run(async () => {
                          await native(c, 'booth', 'account_login', { accountId: a.id });
                        })
                      : begin([a.id], 'incremental')
                  }
                >
                  {L(job ? 'running' : signInRequired ? 'signIn' : 'syncAccount')}
                </button>
                {job && (
                  <button
                    disabled={task.busy}
                    onClick={() =>
                      task.run(async () => {
                        await native(c, 'booth', 'cancel_sync', { jobId: job.id, accountId: a.id });
                      })
                    }
                  >
                    {L('cancel')}
                  </button>
                )}
              </div>
            </div>
          );
        })}
        {unassigned > 0 && (
          <div className="wf-sync-account">
            <div className="wf-sync-identity">
              <strong>{L('unassignedAccount')}</strong>
              <small>
                {L('lastSync')} · {L('unknown')}
              </small>
            </div>
            <div className="wf-sync-action">
              <span>{count(unassigned)}</span>
              <button disabled>{L('syncAccount')}</button>
            </div>
          </div>
        )}
      </div>
      <Status value={task.error || countError} L={L} error />
    </Panel>
  );
}
