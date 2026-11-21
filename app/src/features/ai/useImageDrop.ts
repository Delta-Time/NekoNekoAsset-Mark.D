import { useEffect, useRef, type ClipboardEvent, type DragEvent } from 'react';
import { isTauri } from '@tauri-apps/api/core';
import { getCurrentWebview } from '@tauri-apps/api/webview';
import { errorCode } from '../../backend';
import { readAiDroppedImages } from '../../ai-backend';
import type { AiImage } from '../../ai-backend';
import { MAX_IMAGES, normalizeImage } from './chat-parts';
import type { ChatRequest } from './useChatRequest';
import type { ChatWorkspace } from './useChatWorkspace';
import type { Composer } from './useComposer';

/** Adding images by dropping files on the window, dropping or pasting in the view, or choosing
 * them; each is normalized and checked against the API and the image limit. */
export function useImageDrop({
  request,
  workspace,
  composer,
  active,
  frozen,
}: {
  request: ChatRequest;
  workspace: ChatWorkspace;
  composer: Composer;
  active: boolean;
  frozen: boolean;
}) {
  const { busy, unconfirmed } = request;
  const { loading, entries, entryId, setError, mounted, vision } = workspace;
  const { images, setDropActive, imageLock, setImageBusy, conversion, setImages } = composer;
  const latest = useRef({ active, frozen, busy, loading, images, entries, entryId, unconfirmed });
  latest.current = { active, frozen, busy, loading, images, entries, entryId, unconfirmed };
  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | undefined;
    if (active && isTauri())
      void getCurrentWebview()
        .onDragDropEvent((event) => {
          if (latest.current.frozen) {
            setDropActive(false);
            return;
          }
          if (event.payload.type === 'enter' || event.payload.type === 'over') {
            setDropActive(true);
            return;
          }
          setDropActive(false);
          if (event.payload.type !== 'drop') return;
          const paths = event.payload.paths;
          const state = latest.current;
          const selected = state.entries.find((entry) => entry.id === state.entryId);
          if (
            !state.active ||
            state.busy ||
            state.loading ||
            state.unconfirmed ||
            imageLock.current
          )
            return;
          if (!selected?.visionSupported) {
            setError('ai_images_unavailable');
            return;
          }
          if (paths.length + state.images.length > MAX_IMAGES) {
            setError('ai_image_limit');
            return;
          }
          imageLock.current = true;
          setImageBusy(true);
          setError('');
          const epoch = conversion.current;
          void readAiDroppedImages(paths)
            .then((value) => {
              if (mounted.current && epoch === conversion.current) {
                setImages((previous) => [...previous, ...value]);
              }
            })
            .catch((value) => {
              if (mounted.current && epoch === conversion.current) setError(errorCode(value));
            })
            .finally(() => {
              imageLock.current = false;
              if (mounted.current) setImageBusy(false);
            });
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
  }, [active]);
  async function addImages(files: File[]) {
    if (!active || frozen || loading || busy || unconfirmed || imageLock.current) return;
    if (!vision) {
      setError('ai_images_unavailable');
      return;
    }
    const accepted = files.filter((file) => file.type.startsWith('image/'));
    if (!accepted.length) {
      setError('ai_image_invalid');
      return;
    }
    if (accepted.length + images.length > MAX_IMAGES) {
      setError('ai_image_limit');
      return;
    }
    imageLock.current = true;
    setImageBusy(true);
    setError('');
    const epoch = conversion.current;
    try {
      const normalized: AiImage[] = [];
      for (const file of accepted) normalized.push(await normalizeImage(file));
      if (mounted.current && epoch === conversion.current) {
        setImages((previous) => [...previous, ...normalized]);
      }
    } catch (value) {
      if (mounted.current && epoch === conversion.current) setError(errorCode(value));
    } finally {
      imageLock.current = false;
      if (mounted.current) setImageBusy(false);
    }
  }
  const drop = (event: DragEvent<HTMLElement>) => {
    if (event.dataTransfer.types.includes('Files')) {
      event.preventDefault();
      setDropActive(false);
      void addImages(Array.from(event.dataTransfer.files));
    }
  };
  const paste = (event: ClipboardEvent<HTMLElement>) => {
    const files = Array.from(event.clipboardData.items)
      .filter((item) => item.kind === 'file' && item.type.startsWith('image/'))
      .map((item) => item.getAsFile())
      .filter((file): file is File => !!file);
    if (files.length) {
      event.preventDefault();
      void addImages(files);
    }
  };
  return { addImages, drop, paste };
}
export type ImageDrop = ReturnType<typeof useImageDrop>;
