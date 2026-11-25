import { useEffect, useRef, useState } from 'react';
import type { AiImage } from '../../ai-backend';
import { encoder } from './chat-parts';

/** The message being written and its attached images. */
export function useComposer({ active }: { active: boolean }) {
  const [prompt, setPrompt] = useState('');
  const [images, setImages] = useState<AiImage[]>([]);
  const [imageBusy, setImageBusy] = useState(false);
  const [dropActive, setDropActive] = useState(false);
  const [imageTrayOpen, setImageTrayOpen] = useState(false);
  const composing = useRef(false);
  const conversion = useRef(0);
  const imageLock = useRef(false);
  const imageInput = useRef<HTMLInputElement>(null);
  const promptTooLong = encoder.encode(prompt.trim()).length > 4000;
  useEffect(() => {
    if (!images.length) setImageTrayOpen(false);
  }, [images.length]);
  useEffect(() => {
    if (!active) setDropActive(false);
  }, [active]);
  return {
    prompt,
    setPrompt,
    images,
    setImages,
    imageBusy,
    setImageBusy,
    dropActive,
    setDropActive,
    imageTrayOpen,
    setImageTrayOpen,
    composing,
    conversion,
    imageLock,
    imageInput,
    promptTooLong,
  };
}
export type Composer = ReturnType<typeof useComposer>;
