import { errorCode } from '../../backend';
import {
  type AiScopes,
  cancelAi,
  retryAi,
  type AiKeyStatus,
  getAiKeyStatus,
  startAi,
} from '../../ai-backend';
import { type Ctx, type ActionTask, Status, useAction, Panel, Row } from './parts';
import { useState, useEffect } from 'react';

export function aiScopes(value: unknown): AiScopes {
  const saved = value && typeof value === 'object' ? (value as Partial<AiScopes>) : {};
  return {
    title: typeof saved.title === 'boolean' ? saved.title : true,
    tags: typeof saved.tags === 'boolean' ? saved.tags : true,
    description: typeof saved.description === 'boolean' ? saved.description : true,
    files: typeof saved.files === 'boolean' ? saved.files : true,
    images: saved.images === true,
  };
}
export function AIScopes({
  value,
  onChange,
  L,
  disabled = false,
  imageAvailable = false,
}: {
  value: AiScopes;
  onChange: (value: AiScopes) => void;
  L: Ctx['L'];
  disabled?: boolean;
  imageAvailable?: boolean;
}) {
  return (
    <div className="wf-scope">
      {(['title', 'tags', 'description', 'files', 'images'] as const).map((key) => (
        <label className="form-row" key={key}>
          <input
            type="checkbox"
            checked={value[key]}
            disabled={disabled || (key === 'images' && !imageAvailable && !value.images)}
            onChange={(event) => onChange({ ...value, [key]: event.target.checked })}
          />
          {L('scope-' + key)}
        </label>
      ))}
    </div>
  );
}
export function AIJob({
  c,
  jobId,
  task,
  onRetry,
}: {
  c: Ctx;
  jobId: string;
  task: ActionTask;
  onRetry: (id: string) => void;
}) {
  const job = c.nativeState?.jobs.find((value) => value.id === jobId);
  if (!jobId) return null;
  const active = !job || ['queued', 'running'].includes(job.state);
  return (
    <div className="wf-progress">
      <Status value={job?.errorCode || job?.state || 'queued'} L={c.L} error={!!job?.errorCode} />
      {active &&
        (typeof job?.progress === 'number' ? (
          <progress value={job.progress} max={100} />
        ) : (
          <progress />
        ))}
      {active ? (
        <button
          disabled={task.busy}
          onClick={() =>
            void task.run(async () => {
              await cancelAi(jobId);
            })
          }
        >
          {c.L('cancel')}
        </button>
      ) : (
        job &&
        ['failed', 'needsAction', 'cancelled', 'interrupted', 'paused'].includes(job.state) && (
          <button
            disabled={task.busy}
            onClick={() =>
              void task.run(async () => {
                const result = await retryAi(jobId);
                onRetry(result.jobId);
              })
            }
          >
            {c.L('retry')}
          </button>
        )
      )}
    </div>
  );
}
export function AI(c: Ctx) {
  const { L } = c;
  const task = useAction();
  const [scopes, setScopes] = useState(() => aiScopes(c.nativeState?.preferences.aiScopes));
  const [jobId, setJobId] = useState('');
  const [settings, setSettings] = useState<AiKeyStatus | null>(null),
    [loadError, setLoadError] = useState('');
  useEffect(() => {
    let current = true;
    void getAiKeyStatus()
      .then((value) => {
        if (current) setSettings(value);
      })
      .catch((error) => {
        if (current) setLoadError(errorCode(error));
      });
    return () => {
      current = false;
    };
  }, []);
  const [ids] = useState(() => [...new Set(c.selected)]);
  const job = c.nativeState?.jobs.find((value) => value.id === jobId);
  const active = !!jobId && (!job || ['queued', 'running'].includes(job.state));
  const imageAvailable = settings?.visionSupported === true;
  const imageBlocked = scopes.images && !imageAvailable;
  return (
    <Panel
      actions={
        <>
          <button onClick={c.onClose}>{L('close')}</button>
          <button
            className="primary"
            disabled={
              task.busy ||
              active ||
              !settings?.configured ||
              !ids.length ||
              !Object.values(scopes).some(Boolean) ||
              imageBlocked
            }
            onClick={() =>
              void task.run(async () => {
                const result = await startAi({ mode: 'classify', productIds: ids, scopes });
                setJobId(result.jobId);
              })
            }
          >
            {L('generate')}
          </button>
        </>
      }
    >
      <Row label={L('selected')}>{ids.length}</Row>
      <AIScopes
        value={scopes}
        onChange={setScopes}
        L={L}
        disabled={task.busy || active}
        imageAvailable={imageAvailable}
      />
      <AIJob c={c} jobId={jobId} task={task} onRetry={setJobId} />
      <Status
        value={task.error || loadError || (settings && imageBlocked ? 'ai_vision_unsupported' : '')}
        L={L}
        error
      />
    </Panel>
  );
}
