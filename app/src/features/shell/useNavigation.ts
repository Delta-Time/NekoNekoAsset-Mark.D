import { useRef, useState } from 'react';
import type { ImportSeed } from '../../Workflows';
import type { UnitySeed } from '../../UnityWorkflow';

/** Where the App is: the main view, settings page, open flow or dialog, details tab and pane, the
 * context menu and the license filter dialog. */
export function useNavigation() {
  const [view, setView] = useState('library'),
    [settingsRoute, setSettingsRoute] = useState('appearance');
  const [activityJobId, setActivityJobId] = useState<string | null>(null);
  const openActivityJob = (jobId: string) => {
    setActivityJobId(jobId);
    setView('activity');
  };
  const [unitySeed, setUnitySeed] = useState<UnitySeed>({ productIds: [] });
  const openUnity = (productIds: string[], leafRefs?: string[]) => {
    setUnitySeed({ productIds: [...new Set(productIds)], leafRefs });
    setFlow('unity');
  };
  // The products AI tagging works on, named by whatever opened it: the selection or one product.
  const [aiSeed, setAiSeed] = useState<string[]>([]);
  const openAi = (productIds: string[]) => {
    setAiSeed([...new Set(productIds)]);
    setFlow('ai');
  };
  const [flow, setFlow] = useState<string | null>(null),
    [dialog, setDialog] = useState<string | null>(null),
    [tab, setTab] = useState<'details' | 'files'>('details');
  const [inspector, setInspector] = useState(true);
  const [context, setContext] = useState<{ x: number; y: number; ids: string[] } | null>(null),
    [licenseOpen, setLicenseOpen] = useState(false),
    licenseButton = useRef<HTMLButtonElement>(null);
  const [importSeed, setImportSeed] = useState<ImportSeed | undefined>();
  const settingsReturn = useRef({
    inspector: true,
    tab: 'details' as 'details' | 'files',
    view: 'library',
  });
  return {
    view,
    setView,
    activityJobId,
    openActivityJob,
    settingsRoute,
    setSettingsRoute,
    unitySeed,
    openUnity,
    aiSeed,
    openAi,
    flow,
    setFlow,
    dialog,
    setDialog,
    tab,
    setTab,
    inspector,
    setInspector,
    context,
    setContext,
    licenseOpen,
    setLicenseOpen,
    licenseButton,
    importSeed,
    setImportSeed,
    settingsReturn,
  };
}
export type Navigation = ReturnType<typeof useNavigation>;
