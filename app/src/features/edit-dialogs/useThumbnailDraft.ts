import { useState } from 'react';
import { previewSource } from '../../backend';
import type { EditSubject } from './useEditSubject';

/** The thumbnail: the listing's image, an uploaded one or none, and what the preview shows. */
export function useThumbnailDraft({ subject }: { subject: EditSubject }) {
  const { opening, active } = subject;
  const initialSourcePreview =
    opening?.sourcePreviewUrl ?? (opening?.thumbnailPath ? undefined : opening?.previewUrl);
  const [sourceThumbnailUrl, setSourceThumbnailUrl] = useState(active?.sourceThumbnailUrl);
  const sourcePreview = sourceThumbnailUrl ?? initialSourcePreview;
  const initialThumbnailChoice = opening?.thumbnailHidden
    ? 'none'
    : opening?.thumbnailPath || opening?.thumbnailOverrideUrl
      ? 'custom'
      : opening?.previewUrl
        ? 'source'
        : 'none';
  const initialThumbnailURL =
    previewSource(
      opening?.thumbnailOverrideUrl ?? (opening?.thumbnailPath ? opening.previewUrl : undefined),
    ) ?? '';
  const [thumbnailChoice, setThumbnailChoice] = useState(initialThumbnailChoice);
  const [thumbnailURL, setThumbnailURL] = useState(initialThumbnailURL);
  const [thumbnailPath, setThumbnailPath] = useState<string | null>();
  const thumbnailPreview =
    thumbnailChoice === 'custom'
      ? thumbnailURL
      : thumbnailChoice === 'source'
        ? previewSource(sourcePreview)
        : undefined;
  return {
    sourceThumbnailUrl,
    setSourceThumbnailUrl,
    initialThumbnailChoice,
    initialThumbnailURL,
    thumbnailChoice,
    setThumbnailChoice,
    thumbnailURL,
    setThumbnailURL,
    thumbnailPath,
    setThumbnailPath,
    thumbnailPreview,
  };
}
export type ThumbnailDraft = ReturnType<typeof useThumbnailDraft>;
