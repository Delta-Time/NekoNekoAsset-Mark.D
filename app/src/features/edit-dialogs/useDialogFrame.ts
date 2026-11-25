import { useEffect, useRef, useState } from 'react';

/** The dialog around the fields: busy and in-flight guards, the error shown at the bottom, the
 * unsaved-changes confirmation and the elements focus moves between. */
export function useDialogFrame() {
  const alive = useRef(true);
  const [error, setError] = useState('');
  const [busy, setBusy] = useState(false);
  const inFlight = useRef(false);
  const [confirmClose, setConfirmClose] = useState(false);
  const box = useRef<HTMLDivElement>(null);
  const confirmation = useRef<HTMLDivElement>(null);
  const stayButton = useRef<HTMLButtonElement>(null);
  const saved = useRef(false);
  useEffect(() => {
    if (!error) return;
    const frame = requestAnimationFrame(() =>
      box.current?.querySelector('[role="alert"]')?.scrollIntoView({ block: 'nearest' }),
    );
    return () => cancelAnimationFrame(frame);
  }, [error]);
  const confirmationOpen = useRef(confirmClose);
  confirmationOpen.current = confirmClose;
  return {
    alive,
    error,
    setError,
    busy,
    setBusy,
    inFlight,
    confirmClose,
    setConfirmClose,
    box,
    confirmation,
    stayButton,
    saved,
    confirmationOpen,
  };
}
export type DialogFrame = ReturnType<typeof useDialogFrame>;
