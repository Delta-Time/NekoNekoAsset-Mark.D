import { useState } from 'react';
import { nativeWorkflowText } from '../../native-workflow-i18n';
import { normalizeProductUrl } from './parts';
import type { EditSubject } from './useEditSubject';

/** The title and listing URL being edited, and the AI's suggested listing URLs not yet ignored. */
export function useListingDraft({ subject }: { subject: EditSubject }) {
  const { active, p } = subject;
  const [title, setTitle] = useState(active?.title ?? '');
  const [sourceUrl, setSourceUrl] = useState(active?.sourceUrl ?? '');
  const [ignoredAiUrls, setIgnoredAiUrls] = useState<string[]>([]);
  const AIText = (key: string) => nativeWorkflowText(p.locale, key);
  const aiUrlCandidates = (p.aiFeaturesEnabled ? (active?.aiSourceUrlCandidates ?? []) : []).filter(
    (candidate) =>
      !ignoredAiUrls.includes(candidate.url) && normalizeProductUrl(candidate.url) !== null,
  );
  const [resetTitle, setResetTitle] = useState(false);
  return {
    title,
    setTitle,
    sourceUrl,
    setSourceUrl,
    setIgnoredAiUrls,
    AIText,
    aiUrlCandidates,
    resetTitle,
    setResetTitle,
  };
}
export type ListingDraft = ReturnType<typeof useListingDraft>;
