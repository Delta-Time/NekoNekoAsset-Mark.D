import ActivityHistory from '../../ActivityHistory';
import { cancelUnity, errorCode, invokeAction, retryUnityInventory } from '../../backend';
import { cancelAi, retryAi } from '../../ai-backend';
import { isStoreJob } from '../workflows/jobs';
import type { UiPreferences } from '../preferences/useUiPreferences';
import type { Library } from '../library/useLibrary';
import type { Results } from '../results/useResults';
import type { ProductActions } from '../products/useProductActions';
import type { Native } from '../library/useNative';
import type { Navigation } from '../shell/useNavigation';
import type { Notices } from '../shell/useNotice';
import type { SettingsPane } from '../settings/useSettingsPane';

/** The Activity view: job history with cancel, retry, sign-in and Unity results. */
export default function ActivityView({
  uiPrefs,
  library,
  results,
  actions,
  native,
  nav,
  notices,
  settingsPane,
  q,
}: {
  uiPrefs: UiPreferences;
  library: Library;
  results: Results;
  actions: ProductActions;
  native: Native;
  nav: Navigation;
  notices: Notices;
  settingsPane: SettingsPane;
  q: (key: string) => string;
}) {
  const { locale } = uiPrefs;
  const { nativeState, accounts, aiEnabled } = library;
  const { products } = results;
  const { cancelDownloads } = actions;
  const { onNative, refreshState } = native;
  const { setFlow, openUnity } = nav;
  const { notify, run } = notices;
  const { openSettings } = settingsPane;
  return (
    <ActivityHistory
      locale={locale}
      jobs={nativeState.jobs.filter((job) => !job.activityCleared)}
      accounts={accounts}
      products={products}
      aiEnabled={aiEnabled}
      onCancelDownloads={() => cancelDownloads()}
      onClear={async () => {
        await onNative('catalogue', 'clear_activity');
        await refreshState();
      }}
      onUnityResults={(job) => setFlow('unity-results:' + job.id)}
      onUnityRetry={(job) => {
        void retryUnityInventory(job.id)
          .then((seed) => openUnity(seed.productIds, seed.leafRefs))
          .catch((error) => notify(q(errorCode(error))));
      }}
      onCancel={(job) =>
        run(
          job.type === 'licenses:sync'
            ? invokeAction('licenses', 'cancel', { jobId: job.id })
            : job.type === 'unity:import'
              ? cancelUnity(job.id)
              : job.type.startsWith('ai:')
                ? cancelAi(job.id)
                : onNative(
                    isStoreJob(job.type) ? 'booth' : 'storage',
                    isStoreJob(job.type) ? 'cancel_sync' : 'cancel_job',
                    { jobId: job.id },
                  ),
        )
      }
      onRetry={(job) =>
        run(
          job.type === 'licenses:sync'
            ? invokeAction('licenses', 'retry', { jobId: job.id })
            : job.type.startsWith('ai:')
              ? retryAi(job.id)
              : onNative(
                  isStoreJob(job.type) ? 'booth' : 'storage',
                  isStoreJob(job.type) ? 'retry_sync' : 'retry_job',
                  { jobId: job.id },
                ),
        )
      }
      onSignIn={(job) => {
        if (job.accountId) run(onNative('booth', 'account_login', { accountId: job.accountId }));
      }}
      onOpenStorage={() => openSettings('storage')}
    />
  );
}
