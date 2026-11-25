import { useLayoutEffect, useRef, useState } from 'react';
import type { LicenseCells } from '../../Licenses';

/** The license table edited in its own dialog; focus returns to its button when it closes. */
export function useLicenseDraft() {
  // undefined: untouched; null: back to the synced license; cells: the user's table.
  const [licenseDraft, setLicenseDraft] = useState<LicenseCells | null | undefined>(undefined),
    [licenseOpen, setLicenseOpen] = useState(false);
  const licenseShown = useRef(licenseOpen);
  licenseShown.current = licenseOpen;
  const licenseTrigger = useRef<HTMLButtonElement>(null),
    licenseWasOpen = useRef(false);
  useLayoutEffect(() => {
    if (licenseWasOpen.current && !licenseOpen) licenseTrigger.current?.focus();
    licenseWasOpen.current = licenseOpen;
  }, [licenseOpen]);
  return {
    licenseDraft,
    setLicenseDraft,
    licenseOpen,
    setLicenseOpen,
    licenseShown,
    licenseTrigger,
  };
}
export type LicenseDraft = ReturnType<typeof useLicenseDraft>;
