import { type Ctx, useAction, Panel, savePreferences, Field, Status } from './parts';
import { useState, useEffect } from 'react';
import Toggle from '../../Toggle';

export function Downloads(c: Ctx) {
  const { L } = c;
  const task = useAction();
  const prefs = c.nativeState?.preferences ?? {};
  const raw = Number(prefs.downloadConcurrency ?? 3);
  const count = Number.isInteger(raw) && raw >= 1 ? Math.min(6, raw) : 3;
  const rate = Number(prefs.downloadRateBytesPerSecond ?? 0) / 1_000_000;
  const [rateDraft, setRateDraft] = useState(String(rate));
  useEffect(() => setRateDraft(String(rate)), [rate]);
  const validRate =
    rateDraft !== '' &&
    Number.isInteger(Number(rateDraft)) &&
    Number(rateDraft) >= 0 &&
    Number(rateDraft) <= 10000;
  return (
    <Panel>
      <Toggle
        label={L('autoAll')}
        checked={
          typeof prefs.autoDownloadAll === 'boolean'
            ? prefs.autoDownloadAll
            : (c.autoDownloadAll ?? false)
        }
        onChange={(value) => {
          void task.run(async () => {
            await savePreferences(c, { autoDownloadAll: value });
            c.onPolicyChange?.(value);
          });
        }}
      />
      <div className="wf-download-preferences">
        <Field label={L('concurrentDownloads')}>
          <select
            disabled={task.busy}
            aria-label={L('concurrentDownloads')}
            value={count}
            onChange={(e) => {
              void task.run(() =>
                savePreferences(c, { downloadConcurrency: Number(e.target.value) }),
              );
            }}
          >
            {Array.from({ length: 6 }, (_, i) => (
              <option key={i + 1} value={i + 1}>
                {i + 1}
              </option>
            ))}
          </select>
        </Field>
        <Field label={L('downloadRate')}>
          <span className="wf-number-unit">
            <input
              disabled={task.busy}
              type="number"
              min={0}
              max={10000}
              step={1}
              aria-label={L('downloadRate')}
              aria-invalid={!validRate}
              value={rateDraft}
              onChange={(e) => setRateDraft(e.target.value)}
              onBlur={() => {
                if (!validRate) {
                  setRateDraft(String(rate));
                  return;
                }
                if (Number(rateDraft) !== rate)
                  void task.run(() =>
                    savePreferences(c, {
                      downloadRateBytesPerSecond: Number(rateDraft) * 1_000_000,
                    }),
                  );
              }}
            />
            <span>MB/s</span>
          </span>
          <small>{L('zeroUnlimited')}</small>
        </Field>
      </div>
      <Status value={task.error} L={L} error />
    </Panel>
  );
}
