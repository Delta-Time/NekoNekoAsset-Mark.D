import { message } from './messages';
import type { Locale } from './model';

export function updateText(locale: Locale, key: string): string {
  return message(locale, 'update', key) ?? message(locale, 'update', 'operationFailed') ?? key;
}

// Mirrors native updates::blocking_job: store syncs, downloads and local imports resume after the restart.
const jobBlockers: Record<string, string> = {
  'storage:migrate_root': 'updates_busy_move',
  'storage:export_backup': 'updates_busy_backup',
  'storage:restore_backup': 'updates_busy_restore',
  'storage:export_zip': 'updates_busy_export',
  'storage:delete_local_products': 'updates_busy_delete',
  'unity:import': 'updates_busy_unity',
  boothLogin: 'updates_busy_login',
  gumroadLogin: 'updates_busy_login',
  jinxxyLogin: 'updates_busy_login',
};
const active = (job: { state: string }) => job.state === 'queued' || job.state === 'running';
export const resumesAfterRestart = (job: { type: string; state: string }) =>
  active(job) &&
  /^((booth|gumroad|jinxxy)(Sync|Update)|download|storage:import_local|licenses:sync)$/.test(
    job.type,
  );
export function updateJobBlocker(jobs: { type: string; state: string }[]): string {
  const job = jobs.find((job) => active(job) && !resumesAfterRestart(job));
  return !job
    ? ''
    : (jobBlockers[job.type] ??
        (job.type.startsWith('ai:') ? 'updates_busy_ai' : 'updates_busy_job'));
}

// Bridge releases share numeric App versions; absent or unrecognized versions stay unknown.
export function isNewerBridge(latest: string, current: string | null | undefined): boolean {
  const parse = (value: string) =>
    /^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$/.exec(value)?.slice(1).map(Number);
  const a = parse(latest),
    b = current ? parse(current) : undefined;
  if (!a || !b) return false;
  for (let i = 0; i < 3; i++) {
    if (a[i] !== b[i]) return a[i] > b[i];
  }
  return false;
}
