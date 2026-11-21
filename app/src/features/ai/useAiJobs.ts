import { useState } from 'react';
import { saveAiPreferences } from '../../ai-backend';
import type { NativeState } from '../../backend';
import type { AiScopes } from '../../ai-backend';
import { savedScopes } from './settings-parts';
import type { SettingsRun } from './useSettingsRun';

/** The automatic-AI preference and scopes, and the AI job this page started. */
export function useAiJobs({
  runner,
  nativeState,
}: {
  runner: SettingsRun;
  nativeState: NativeState | undefined;
}) {
  const { run } = runner;
  const [jobId, setJobId] = useState('');
  const prefs = nativeState?.preferences,
    scopes = savedScopes(prefs?.aiScopes),
    auto = prefs?.aiEnabled === true,
    featuresEnabled = prefs?.aiFeaturesEnabled === true;
  const jobsActive = (nativeState?.jobs ?? []).some(
    (job) => job.type.startsWith('ai:') && ['queued', 'running'].includes(job.state),
  );
  const job = nativeState?.jobs.find((value) => value.id === jobId),
    jobActive = !!jobId && (!job || ['queued', 'running'].includes(job.state));
  const savePrefs = (value: AiScopes, enabled = auto) =>
    void run(async () => {
      await saveAiPreferences({ aiEnabled: enabled, aiScopes: value });
    });
  return { jobId, setJobId, scopes, auto, featuresEnabled, jobsActive, job, jobActive, savePrefs };
}
export type AiJobs = ReturnType<typeof useAiJobs>;
