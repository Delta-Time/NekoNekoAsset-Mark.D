import {
  type Ctx,
  useAction,
  choose,
  native,
  Panel,
  Field,
  displayWindowsPath,
  Buttons,
  Row,
  Status,
} from './parts';
import { useState, useRef, useEffect } from 'react';
import { isActive, unfinishedLocalImport } from './jobs';
import { invokeAction } from '../../backend';
import { RefreshCw } from 'lucide-react';
import RestoreConfirmation from '../../RestoreConfirmation';
import ResetApp from '../../ResetApp';

export function Storage(c: Ctx) {
  const { L } = c;
  const task = useAction();
  const [restorePath, setRestorePath] = useState<string | null>(null);
  const restoreTrigger = useRef<HTMLButtonElement>(null);
  const roots = c.nativeState?.roots ?? [];
  const root = roots.find((r) => r.status === 'active' || r.status === 'online') ?? roots[0];
  const [rootId, setRootId] = useState(root?.id ?? '');
  const current = roots.find((r) => r.id === rootId) ?? root;
  const storageJobs = (c.nativeState?.jobs ?? []).filter(
    (j) => j.type.startsWith('storage:') && isActive(j),
  );
  const settled = !(c.nativeState?.jobs ?? []).some(isActive);
  const [usage, setUsage] = useState<{ rootId: string; usedBytes?: number; freeBytes?: number }>();
  useEffect(() => {
    if (!current) return;
    let live = true;
    void invokeAction('storage', 'root_usage', {
      rootId: current.id,
    }).then(
      (result: { usedBytes: number; freeBytes: number }) => {
        if (live) setUsage({ rootId: current.id, ...result });
      },
      () => {
        if (live) setUsage({ rootId: current.id });
      },
    );
    return () => {
      live = false;
    };
  }, [current?.id, current?.path, current?.status, settled]);
  const bytes = (value?: number) => {
    if (usage?.rootId !== current?.id) return <RefreshCw size={13} className="sync-spinning" />;
    if (value === undefined) return '—';
    const unit =
      value >= 1024 ** 4
        ? 4
        : value >= 1024 ** 3
          ? 3
          : value >= 1024 ** 2
            ? 2
            : value >= 1024
              ? 1
              : 0;
    return (
      new Intl.NumberFormat(c.locale, { maximumFractionDigits: 1 }).format(value / 1024 ** unit) +
      ' ' +
      ['B', 'KB', 'MB', 'GB', 'TB'][unit]
    );
  };
  const [moving, setMoving] = useState(false);
  const location = (action: 'choose' | 'move' | 'remap') =>
    task.run(async () => {
      const path = await choose(c, 'folder', { defaultPath: current?.path });
      if (!path) return;
      if (action === 'choose') {
        await c.onLibraryReady?.(path, undefined, true);
      } else if (current) {
        setMoving(action === 'move');
        try {
          await native(c, 'storage', action === 'move' ? 'migrate_root' : 'remap_root', {
            rootId: current.id,
            path,
          });
        } finally {
          setMoving(false);
        }
      }
    });
  return (
    <Panel>
      {roots.length > 1 && (
        <Field label={L('currentLocation')}>
          <select
            aria-label={L('currentLocation')}
            value={current?.id ?? ''}
            onChange={(e) => setRootId(e.target.value)}
          >
            {roots.map((r) => (
              <option key={r.id} value={r.id}>
                {r.label ?? displayWindowsPath(r.path)}
              </option>
            ))}
          </select>
        </Field>
      )}
      <Field label={L('libraryLocation')}>
        <input
          readOnly
          value={displayWindowsPath(current?.path ?? '')}
          title={displayWindowsPath(current?.path ?? '')}
          placeholder={L('notConfigured')}
        />
      </Field>
      <Buttons>
        <button disabled={task.busy || !!storageJobs.length} onClick={() => location('choose')}>
          {L('openLibrary')}
        </button>
        <button
          disabled={task.busy || !current}
          onClick={() =>
            task.run(async () => {
              await native(c, 'storage', 'open_root_folder', { rootId: current!.id });
            })
          }
        >
          {L('openFolder')}
        </button>
        <button
          disabled={task.busy || !current || !!storageJobs.length}
          onClick={() => location('move')}
        >
          {L('move')}
        </button>
      </Buttons>
      <Row label={L('usedSpace')}>{bytes(usage?.usedBytes)}</Row>
      <Row label={L('space')}>{bytes(usage?.freeBytes)}</Row>
      {current && !['active', 'online', 'ready', 'configured'].includes(current.status) && (
        <div className="wf-section">
          <Status value={current.status} L={L} error />
          <button disabled={task.busy} onClick={() => location('remap')}>
            {L('remap')}
          </button>
        </div>
      )}
      <div className="wf-section">
        <Buttons>
          <button
            disabled={task.busy || !!storageJobs.length}
            onClick={() =>
              task.run(async () => {
                const path = await choose(c, 'save_file', {
                  filters: [{ name: 'ZIP', extensions: ['zip'] }],
                });
                if (path) await native(c, 'storage', 'export_backup', { path });
              })
            }
          >
            {L('export')}
          </button>
          <button
            ref={restoreTrigger}
            disabled={task.busy || !!storageJobs.length}
            onClick={() =>
              task.run(async () => {
                const path = await choose(c, 'open_file', {
                  filters: [{ name: 'ZIP', extensions: ['zip'] }],
                });
                if (path) setRestorePath(path);
              })
            }
          >
            {L('restore')}
          </button>
        </Buttons>
      </div>
      {moving && (
        <div className="wf-progress" role="status">
          <span>{L('migrate_root')}</span>
          <progress />
        </div>
      )}
      {storageJobs.map((job) => (
        <div className="wf-progress" key={job.id}>
          <span>
            {L(job.type.replace('storage:', ''))} · {L(job.state)}
          </span>
          {typeof job.progress === 'number' ? (
            <progress value={job.progress} max={100} />
          ) : (
            <progress />
          )}
          <button
            disabled={task.busy}
            onClick={() =>
              task.run(async () => {
                await native(c, 'storage', 'cancel_job', { jobId: job.id });
              })
            }
          >
            {L('cancel')}
          </button>
        </div>
      ))}
      {restorePath && (
        <RestoreConfirmation
          locale={c.locale}
          path={restorePath}
          onRestore={() => native(c, 'storage', 'restore_backup', { path: restorePath })}
          onClose={() => {
            setRestorePath(null);
            requestAnimationFrame(() => restoreTrigger.current?.focus({ preventScroll: true }));
          }}
        />
      )}
      <StorageRecovery c={c} rootId={current?.id} />
      <ResetApp
        libraryPath={current?.path}
        locale={c.locale}
        disabled={task.busy || (c.nativeState?.jobs ?? []).some(isActive)}
        onReset={(deleteLibrary) => native(c, 'storage', 'reset_app', { deleteLibrary })}
      />
      <Status value={task.error} L={L} error />
    </Panel>
  );
}
function StorageRecovery({ c, rootId }: { c: Ctx; rootId?: string }) {
  const task = useAction();
  const [cleanup, setCleanup] = useState(false);
  const { L } = c;
  const root = c.nativeState?.roots.find((r) => r.id === rootId) as
    | Record<string, unknown>
    | undefined;
  const operationBusy = (c.nativeState?.jobs ?? []).some(
    (job) => job.type.startsWith('storage:') && isActive(job),
  );
  const problems = (c.nativeState?.jobs ?? [])
    .filter(
      (job) =>
        job.type.startsWith('storage:') &&
        (['needsAction', 'failed'].includes(job.state) ||
          (job.state === 'cancelled' && unfinishedLocalImport(job))),
    )
    .slice(-3);
  return (
    <>
      {root?.previousMigration && (
        <div className="wf-section">
          {cleanup ? (
            <>
              <Status value="confirmCleanup" L={L} />
              <Buttons>
                <button disabled={task.busy} onClick={() => setCleanup(false)}>
                  {L('cancel')}
                </button>
                <button
                  disabled={task.busy || operationBusy}
                  onClick={() =>
                    task.run(async () => {
                      await native(c, 'storage', 'cleanup_old_root', { rootId });
                      setCleanup(false);
                    })
                  }
                >
                  {L('cleanup')}
                </button>
              </Buttons>
            </>
          ) : (
            <button disabled={task.busy || operationBusy} onClick={() => setCleanup(true)}>
              {L('cleanup')}
            </button>
          )}
        </div>
      )}
      {problems.map((job) => (
        <div className="wf-section" key={job.id}>
          <Status value={job.errorCode ?? job.state} L={L} error={job.state !== 'cancelled'} />
          {job.type === 'storage:import_local' && (
            <button
              disabled={task.busy || operationBusy}
              onClick={() =>
                task.run(async () => {
                  await native(c, 'storage', 'retry_job', { jobId: job.id });
                })
              }
            >
              {L('retry')}
            </button>
          )}
        </div>
      ))}
      <Status value={task.error} L={L} error />
    </>
  );
}
