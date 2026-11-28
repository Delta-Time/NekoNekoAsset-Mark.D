import { useEffect, useRef, useState } from 'react';
import type { Product } from '../../model';

/** The details and files panes scroll back to the top, and a pending file removal or archive
 * password is dropped, when another product is shown. */
export function useInspectorPanes({ p }: { p: Product }) {
  const detailScroll = useRef<HTMLDivElement>(null),
    fileScroll = useRef<HTMLDivElement>(null);
  const [removingFile, setRemovingFile] = useState<string | null>(null),
    [unlockingFile, setUnlockingFile] = useState<string | null>(null);
  useEffect(() => {
    setRemovingFile(null);
    setUnlockingFile(null);
  }, [p.id]);
  useEffect(() => {
    detailScroll.current?.scrollTo(0, 0);
    fileScroll.current?.scrollTo(0, 0);
  }, [p.id]);
  return {
    detailScroll,
    fileScroll,
    removingFile,
    setRemovingFile,
    unlockingFile,
    setUnlockingFile,
  };
}
export type InspectorPanes = ReturnType<typeof useInspectorPanes>;
