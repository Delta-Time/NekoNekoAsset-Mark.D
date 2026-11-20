import type { NativeJob } from '../../backend';
import type { Account, Locale } from '../../model';
import { nativeWorkflowText } from '../../native-workflow-i18n';

export const isActive = (job: NativeJob) => ['queued', 'running'].includes(job.state);
export const isStoreJob = (type: string) => /^(booth|gumroad|jinxxy)/.test(type.toLowerCase());
export const isStoreSyncJob = (type: string) =>
  /^(booth|gumroad|jinxxy).*(sync|update)/i.test(type);
export function needsStoreSignIn(
  job: NativeJob | undefined,
  account: Pick<Account, 'status' | 'authenticatedAt'> | undefined,
): boolean {
  if (
    !job ||
    !isStoreJob(job.type) ||
    !['AUTH_REQUIRED', 'AUTH_CHALLENGE', 'AUTH_TIMEOUT'].includes(job.errorCode ?? '')
  )
    return false;
  const authenticated = Date.parse(account?.authenticatedAt ?? ''),
    failedAt = Date.parse(job.completedAt ?? job.updatedAt ?? job.createdAt ?? '');
  return (
    account?.status !== 'connected' ||
    !Number.isFinite(authenticated) ||
    !Number.isFinite(failedAt) ||
    authenticated <= failedAt
  );
}
export function syncProgressLabel(locale: Locale, job: NativeJob): string {
  const L = (key: string) => nativeWorkflowText(locale, key),
    progress = job.progress;
  if (job.state === 'queued') return L('queued');
  if (!progress || typeof progress === 'number') return L(job.state);
  const stage =
    isActive(job) &&
    ['authenticating', 'collecting', 'enriching', 'publishing', 'finalizing'].includes(
      progress.stage ?? '',
    )
      ? L('syncStage_' + progress.stage)
      : L(job.state);
  // An update reads only its assets, so its closing "all" is no scope to show.
  const source =
    progress.source && !(progress.source === 'all' && /update$/i.test(job.type))
      ? L(
          ({ purchase: 'orders', gift: 'gifts' } as Record<string, string>)[progress.source] ??
            progress.source,
        )
      : '';
  const count = (value: number) => value.toLocaleString(locale);
  const counts = [
    typeof progress.pages === 'number' && progress.pages > 0
      ? `${L('syncListPages')} ${count(progress.pages)}`
      : '',
    typeof progress.detailsCompleted === 'number' && progress.detailsCompleted > 0
      ? `${L('syncDetails')} ${count(progress.detailsCompleted)}`
      : '',
    typeof progress.published === 'number'
      ? `${L('syncSaved')} ${count(progress.published)}`
      : progress.stage !== 'collecting' &&
          typeof progress.metadataTotal === 'number' &&
          progress.metadataTotal > 0
        ? `${L('syncDetails')} ${count(progress.metadataCompleted ?? 0)}/${count(progress.metadataTotal)}`
        : '',
  ];
  const warning =
    typeof progress.availabilityWarnings === 'number' && progress.availabilityWarnings > 0
      ? `${L('syncUnavailableOffers')} ${count(progress.availabilityWarnings)}`
      : '';
  const incomplete =
    typeof progress.incompleteProducts === 'number' && progress.incompleteProducts > 0
      ? `${L('syncIncompleteAssets')} ${count(progress.incompleteProducts)}`
      : '';
  return [stage, source, ...counts, warning, incomplete].filter(Boolean).join(' · ');
}
export const unfinishedLocalImport = (job: NativeJob) =>
  job.type === 'storage:import_local' &&
  (!job.productIds?.length || job.productIds.some((id) => !job.completedProductIds?.includes(id)));
