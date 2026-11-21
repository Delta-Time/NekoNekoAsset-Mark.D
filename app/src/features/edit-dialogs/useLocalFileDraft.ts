import { useEffect, useRef, useState } from 'react';
import { getCurrentWebview } from '@tauri-apps/api/webview';
import { isTauri } from '@tauri-apps/api/core';
import { dialog, errorCode } from '../../backend';
import type { EditSubject } from './useEditSubject';
import type { DialogFrame } from './useDialogFrame';
import type { AvatarDraft } from './useAvatarDraft';

/** A local product's files: added paths and removed file ids, chosen or dropped onto the list. */
export function useLocalFileDraft({
  subject,
  frame,
  avatars,
}: {
  subject: EditSubject;
  frame: DialogFrame;
  avatars: AvatarDraft;
}) {
  const { p, active } = subject;
  const { inFlight, alive, confirmationOpen, setError } = frame;
  const { avatarPickerShown } = avatars;
  const [addedFiles, setAddedFiles] = useState<string[]>([]);
  const [removedFiles, setRemovedFiles] = useState<string[]>([]);
  const [filesDropping, setFilesDropping] = useState(false);
  const localFileList = useRef<HTMLDivElement>(null);
  const filesDraft = useRef(crypto.randomUUID());
  const localFilesEditable =
    p.dialog === 'edit' && active?.provider === 'local' && !!p.onLocalFiles;
  const appendLocalFiles = (paths: string[]) =>
    setAddedFiles((current) => [...new Set([...current, ...paths.filter(Boolean)])]);
  const chooseLocalFiles = async () => {
    if (inFlight.current) return;
    try {
      const picked = await dialog('open_file', { multiple: true });
      if (picked && alive.current && !inFlight.current && !confirmationOpen.current)
        appendLocalFiles(picked.paths ?? [picked.path]);
    } catch (error) {
      if (alive.current) setError(errorCode(error));
    }
  };
  const returnToFileList = () =>
    requestAnimationFrame(() => localFileList.current?.focus({ preventScroll: true }));
  useEffect(() => {
    if (!localFilesEditable || !isTauri()) return;
    let disposed = false;
    let unlisten: (() => void) | undefined;
    void getCurrentWebview()
      .onDragDropEvent((event) => {
        if (disposed) return;
        const payload = event.payload;
        if (
          payload.type === 'leave' ||
          inFlight.current ||
          confirmationOpen.current ||
          avatarPickerShown.current
        ) {
          setFilesDropping(false);
          return;
        }
        const target = document.elementFromPoint(
          payload.position.x / devicePixelRatio,
          payload.position.y / devicePixelRatio,
        );
        const inside = !!target && localFileList.current?.contains(target) === true;
        setFilesDropping(inside && (payload.type === 'enter' || payload.type === 'over'));
        if (payload.type === 'drop' && inside) appendLocalFiles(payload.paths);
      })
      .then((stop) => {
        if (disposed) stop();
        else unlisten = stop;
      })
      .catch(() => {});
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [localFilesEditable]);
  return {
    addedFiles,
    setAddedFiles,
    removedFiles,
    setRemovedFiles,
    filesDropping,
    setFilesDropping,
    localFileList,
    filesDraft,
    localFilesEditable,
    chooseLocalFiles,
    returnToFileList,
  };
}
export type LocalFileDraft = ReturnType<typeof useLocalFileDraft>;
