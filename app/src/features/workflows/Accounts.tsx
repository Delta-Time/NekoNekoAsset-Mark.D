import { type Ctx, native, type ActionTask, Buttons, useAction, Panel, Status } from './parts';
import type { WorkflowAccount } from '../../Workflows';
import { useState, useEffect } from 'react';
import { Check, X, Pencil } from 'lucide-react';
import { isActive, needsStoreSignIn, isStoreJob } from './jobs';

export function accounts(c: Ctx) {
  return c.nativeState?.accounts ?? c.accounts ?? [];
}
export async function signIn(c: Ctx, name?: string, store = 'BOOTH') {
  let label = name?.trim();
  if (!label) {
    const used = new Set(accounts(c).map((account) => account.name));
    label = store;
    for (let suffix = 2; used.has(label); suffix++) label = store + ' ' + suffix;
  }
  const result = await native(c, 'booth', 'account_add', { name: label, store });
  await native(c, 'booth', 'account_login', { accountId: result.accountId });
}
function AccountName({
  account,
  c,
  task,
  disabled,
}: {
  account: WorkflowAccount;
  c: Ctx;
  task: ActionTask;
  disabled: boolean;
}) {
  const { L } = c;
  const [editing, setEditing] = useState(false),
    [name, setName] = useState(account.name ?? '');
  useEffect(() => {
    if (!editing) setName(account.name ?? '');
  }, [account.name, editing]);
  const cancel = () => {
    setName(account.name ?? '');
    setEditing(false);
  };
  const save = () =>
    task.run(async () => {
      await native(c, 'booth', 'account_rename', { accountId: account.id, name: name.trim() });
      setEditing(false);
    });
  return (
    <div className="wf-account-name">
      {editing ? (
        <>
          <input
            autoFocus
            aria-label={L('accountName')}
            value={name}
            maxLength={200}
            disabled={task.busy}
            onChange={(event) => setName(event.target.value)}
            onKeyDown={(event) => {
              if (event.nativeEvent.isComposing) return;
              if (event.key === 'Escape') {
                event.preventDefault();
                event.stopPropagation();
                if (!task.busy) cancel();
              } else if (event.key === 'Enter') {
                event.preventDefault();
                if (!task.busy && name.trim()) void save();
              }
            }}
          />
          <button
            type="button"
            title={L('save')}
            aria-label={L('save')}
            disabled={task.busy || !name.trim()}
            onClick={() => void save()}
          >
            <Check size={13} />
          </button>
          <button
            type="button"
            title={L('cancel')}
            aria-label={L('cancel')}
            disabled={task.busy}
            onClick={cancel}
          >
            <X size={13} />
          </button>
        </>
      ) : (
        <>
          <strong title={account.name ?? undefined}>{account.name}</strong>
          <button
            type="button"
            title={L('renameAccount')}
            aria-label={L('renameAccount') + ' ' + account.name}
            disabled={task.busy || disabled}
            onClick={() => setEditing(true)}
          >
            <Pencil size={12} />
          </button>
        </>
      )}
    </div>
  );
}
export function AccountRows({ c, task }: { c: Ctx; task: ActionTask }) {
  const jobs = c.nativeState?.jobs ?? [];
  const { L } = c;
  const rows = accounts(c);
  const [removing, setRemoving] = useState<string | null>(null),
    [removeProducts, setRemoveProducts] = useState(false);
  const target = rows.find((account) => account.id === removing);
  useEffect(() => {
    if (removing && !target) setRemoving(null);
  }, [removing, target]);
  if (!rows.length) return null;
  return (
    <>
      <div className="data-list wf-account-list">
        {rows.map((account) => {
          const accountJobs = jobs.filter((job) => job.accountId === account.id);
          const active = accountJobs.some(isActive);
          const latest = accountJobs
            .sort((a, b) => String(a.updatedAt ?? '').localeCompare(String(b.updatedAt ?? '')))
            .at(-1);
          const failure =
            latest && ['failed', 'needsAction'].includes(latest.state) ? latest : undefined;
          const login = accountJobs.find((job) => /login/i.test(job.type) && isActive(job));
          const needsLogin =
            account.status !== 'connected' ||
            accountJobs.some((job) => needsStoreSignIn(job, account));
          return (
            <div className="data-row wf-account" key={account.id}>
              <div>
                <AccountName account={account} c={c} task={task} disabled={active || !!target} />
                <small>
                  {account.store} · {L(active ? 'running' : account.status)}
                </small>
                {failure && !active && <small>{L(failure.errorCode ?? failure.state)}</small>}
              </div>
              <Buttons>
                {login ? (
                  <button
                    disabled={task.busy}
                    onClick={() =>
                      task.run(async () => {
                        await native(c, 'booth', 'cancel_sync', { jobId: login.id });
                      })
                    }
                  >
                    {L('cancel')}
                  </button>
                ) : (
                  <button
                    disabled={task.busy || active || !!target}
                    onClick={() =>
                      task.run(async () => {
                        await native(c, 'booth', needsLogin ? 'account_login' : 'account_logout', {
                          accountId: account.id,
                        });
                      })
                    }
                  >
                    {L(
                      !needsLogin
                        ? 'signOut'
                        : account.status === 'expired'
                          ? 'reconnect'
                          : 'signIn',
                    )}
                  </button>
                )}
                <button
                  className="danger"
                  disabled={task.busy || active || !!target}
                  onClick={() => {
                    setRemoveProducts(false);
                    setRemoving(account.id);
                  }}
                >
                  {L('removeAccount')}
                </button>
              </Buttons>
            </div>
          );
        })}
      </div>
      {target && (
        <div
          className="wf-account-confirm"
          role="alertdialog"
          aria-label={L('removeAccount')}
          onKeyDown={(event) => {
            if (event.key === 'Escape') {
              event.stopPropagation();
              if (!task.busy) setRemoving(null);
            }
          }}
        >
          <strong>{L('removeAccountQuestion').replace('{name}', target.name ?? '')}</strong>
          <label className="wf-check">
            <input
              type="checkbox"
              disabled={task.busy}
              checked={removeProducts}
              onChange={(event) => setRemoveProducts(event.target.checked)}
            />
            {L('removeAccountAssets')}
          </label>
          <small>{L('removeAccountFilesKept')}</small>
          <Buttons>
            <button autoFocus disabled={task.busy} onClick={() => setRemoving(null)}>
              {L('cancel')}
            </button>
            <button
              className="danger"
              disabled={task.busy}
              onClick={() =>
                task.run(async () => {
                  await native(c, 'booth', 'account_remove', {
                    accountId: target.id,
                    removeProducts,
                  });
                  setRemoving(null);
                })
              }
            >
              {L('removeAccount')}
            </button>
          </Buttons>
        </div>
      )}
    </>
  );
}
export function Accounts(c: Ctx) {
  const { L } = c;
  const task = useAction();
  const [name, setName] = useState('');
  const [store, setStore] = useState('BOOTH');
  return (
    <Panel>
      <AccountRows c={c} task={task} />
      <div className="wf-inline-add">
        <select aria-label={L('store')} value={store} onChange={(e) => setStore(e.target.value)}>
          {['BOOTH', 'Gumroad', 'Jinxxy'].map((s) => (
            <option key={s}>{s}</option>
          ))}
        </select>
        <input
          disabled={task.busy}
          aria-label={L('accountName')}
          placeholder={L('accountName')}
          value={name}
          onChange={(e) => setName(e.target.value)}
        />
        <button
          disabled={task.busy}
          onClick={() =>
            task.run(async () => {
              await signIn(c, name, store);
              setName('');
            })
          }
        >
          {L('signIn')}
        </button>
      </div>
      <Status value={task.error} L={L} error />
    </Panel>
  );
}
export function DeviceCredentials(c: Ctx) {
  const task = useAction(),
    { L } = c;
  const missing = c.missingCredentials ?? [];
  return (
    <Panel
      actions={
        <button disabled={task.busy} onClick={c.onClose}>
          {L('later')}
        </button>
      }
    >
      <p className="wf-device-signin-message">{L('deviceSignInRequired')}</p>
      <div className="wf-account-list">
        {missing.map((account) => {
          const login = c.nativeState?.jobs.find(
            (job) =>
              job.accountId === account.id &&
              isStoreJob(job.type) &&
              /login/i.test(job.type) &&
              isActive(job),
          );
          return (
            <div className="data-row wf-account" key={account.id}>
              <div>
                <strong>
                  {account.store} · {account.name}
                </strong>
                {login && (
                  <small role="status">
                    {L('signIn')} · {L(login.state)}
                  </small>
                )}
              </div>
              <button
                disabled={task.busy || !!login}
                onClick={() =>
                  task.run(async () => {
                    await native(c, 'booth', 'account_login', { accountId: account.id });
                  })
                }
              >
                {L(login ? 'running' : 'signIn')}
              </button>
            </div>
          );
        })}
      </div>
      <Status value={task.error} L={L} error />
    </Panel>
  );
}
