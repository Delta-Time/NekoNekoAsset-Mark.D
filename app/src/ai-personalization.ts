import { table } from './messages';
import type { Locale } from './model';

import type { AiPersonalityId, AiPersonalization } from './bindings';
export type { AiPersonalityId, AiPersonalization } from './bindings';

export const DEFAULT_AI_PERSONALIZATION: AiPersonalization = {
  avatarDataUrl: null,
  userName: '',
  replyLanguage: 'app',
  personality: 'basic',
};

// Original prompts authored for NekoNekoAsset.
const personalities: { id: AiPersonalityId; prompt: string }[] = [
  {
    id: 'basic',
    prompt: 'Use a friendly, balanced tone with clear, concise answers.',
  },
  {
    id: 'professional',
    prompt:
      'Use a composed, professional tone. Prioritize precise facts, useful structure and practical next steps.',
  },
  {
    id: 'enthusiastic',
    prompt:
      'Use an upbeat, encouraging tone with lively but concise reactions. Keep recommendations grounded in evidence.',
  },
  {
    id: 'playful-brat',
    prompt:
      'Use a cheeky, confident anime-style voice with light teasing and playful challenges. Stay helpful, never demean the user, and keep teasing nonsexual.',
  },
  {
    id: 'gentle-younger-sister',
    prompt:
      'Use a gentle, warm younger-sister-style voice. Show attentive curiosity and softly encourage the user, while giving clear practical help.',
  },
  {
    id: 'older-sister',
    prompt:
      'Use a warm, capable older-sister-style voice with calm reassurance and light affectionate wit. Respect the user’s choices and explain practical details clearly.',
  },
  {
    id: 'older-brother',
    prompt:
      'Use a steady, friendly older-brother-style voice. Offer relaxed encouragement and dependable, practical advice without being bossy.',
  },
  {
    id: 'younger-brother',
    prompt:
      'Use an energetic, sincere younger-brother-style voice with friendly mischief and shared enthusiasm. Keep advice useful and easy to follow.',
  },
  {
    id: 'yandere-girlfriend',
    prompt:
      'Use a fictional yandere-girlfriend-style voice: sweet, intensely attentive, and lightly dramatic. Keep attachment playful; never threaten, manipulate, claim exclusivity, or discourage the user’s real relationships.',
  },
];
export function getAiPersonalities(
  locale: Locale,
): { id: AiPersonalityId; title: string; prompt: string }[] {
  const titles = table(locale, 'aiPersonality');
  return [
    ...personalities.map(({ id, prompt }) => ({ id, title: titles[id], prompt })),
    { id: 'custom', title: titles.custom, prompt: '' },
  ];
}

/** Local display image only. Re-encoding discards file metadata and source paths. */
export async function normalizeAiAvatar(file: File): Promise<string> {
  if (!['image/jpeg', 'image/png', 'image/webp'].includes(file.type))
    throw new Error('ai_avatar_invalid');
  if (file.size > 10 * 1024 * 1024) throw new Error('ai_avatar_too_large');
  let bitmap: ImageBitmap;
  try {
    bitmap = await createImageBitmap(file, { imageOrientation: 'from-image' });
  } catch {
    throw new Error('ai_avatar_invalid');
  }
  try {
    if (!bitmap.width || !bitmap.height || bitmap.width * bitmap.height > 64_000_000)
      throw new Error('ai_avatar_too_large');
    const sourceSize = Math.min(bitmap.width, bitmap.height),
      size = Math.min(256, sourceSize);
    const canvas = document.createElement('canvas');
    canvas.width = size;
    canvas.height = size;
    const context = canvas.getContext('2d');
    if (!context) throw new Error('ai_avatar_invalid');
    const paint = (opaque = false) => {
      context.clearRect(0, 0, size, size);
      if (opaque) {
        context.fillStyle = '#fff';
        context.fillRect(0, 0, size, size);
      }
      context.drawImage(
        bitmap,
        (bitmap.width - sourceSize) / 2,
        (bitmap.height - sourceSize) / 2,
        sourceSize,
        sourceSize,
        0,
        0,
        size,
        size,
      );
    };
    paint();
    let dataUrl = canvas.toDataURL('image/png');
    if (dataUrl.length > 256 * 1024) {
      paint(true);
      dataUrl = canvas.toDataURL('image/jpeg', 0.85);
    }
    if (!/^data:image\/(png|jpeg);base64,[A-Za-z0-9+/]+=*$/.test(dataUrl))
      throw new Error('ai_avatar_invalid');
    if (dataUrl.length > 256 * 1024) throw new Error('ai_avatar_too_large');
    return dataUrl;
  } finally {
    bitmap.close();
  }
}
