import type { AiImage, AiChatSummary } from '../../ai-backend';

export const MAX_IMAGES = 4;
const MAX_IMAGE_BYTES = 2 * 1024 * 1024;
export const encoder = new TextEncoder();
/** Re-encoding strips original image metadata before upload or local chat storage. */
export async function normalizeImage(file: File): Promise<AiImage> {
  if (!['image/jpeg', 'image/png', 'image/webp'].includes(file.type))
    throw new Error('ai_image_invalid');
  if (file.size > 20 * 1024 * 1024) throw new Error('ai_image_too_large');
  let image: ImageBitmap;
  try {
    image = await createImageBitmap(file, { imageOrientation: 'from-image' });
  } catch {
    throw new Error('ai_image_invalid');
  }
  try {
    if (!image.width || !image.height || image.width * image.height > 64_000_000)
      throw new Error('ai_image_too_large');
    const scale = Math.min(1, 1536 / Math.max(image.width, image.height));
    const canvas = document.createElement('canvas');
    canvas.width = Math.max(1, Math.round(image.width * scale));
    canvas.height = Math.max(1, Math.round(image.height * scale));
    const context = canvas.getContext('2d');
    if (!context) throw new Error('ai_image_invalid');
    const encode = (mime: 'image/jpeg' | 'image/png', quality?: number) =>
      new Promise<Blob>((resolve, reject) =>
        canvas.toBlob(
          (blob) => (blob ? resolve(blob) : reject(new Error('ai_image_invalid'))),
          mime,
          quality,
        ),
      );
    let mime: AiImage['mimeType'] = file.type === 'image/png' ? 'image/png' : 'image/jpeg';
    if (mime === 'image/jpeg') {
      context.fillStyle = '#fff';
      context.fillRect(0, 0, canvas.width, canvas.height);
    }
    context.drawImage(image, 0, 0, canvas.width, canvas.height);
    let blob = await encode(mime, 0.86);
    if (blob.size > MAX_IMAGE_BYTES) {
      mime = 'image/jpeg';
      context.fillStyle = '#fff';
      context.fillRect(0, 0, canvas.width, canvas.height);
      context.drawImage(image, 0, 0, canvas.width, canvas.height);
      blob = await encode(mime, 0.75);
    }
    if (blob.size > MAX_IMAGE_BYTES) throw new Error('ai_image_too_large');
    const data = await new Promise<string>((resolve, reject) => {
      const reader = new FileReader();
      reader.onload = () =>
        typeof reader.result === 'string'
          ? resolve(reader.result.split(',')[1] ?? '')
          : reject(new Error('ai_image_invalid'));
      reader.onerror = () => reject(new Error('ai_image_invalid'));
      reader.readAsDataURL(blob);
    });
    if (!data) throw new Error('ai_image_invalid');
    return { mimeType: mime, dataBase64: data };
  } finally {
    image.close();
  }
}
export type SearchScope = 'library' | 'online' | 'both';
export type Store = 'booth' | 'gumroad' | 'jinxxy';
export type DeleteState = { id: string; ready: boolean };
/** A sent message whose arrival in the chat history is not confirmed yet. */
export type UnconfirmedTurn = {
  chatId: string;
  requestId: string;
  content: string;
  images: AiImage[];
  priorIds: string[];
};
export const imageSource = (image: AiImage) => `data:${image.mimeType};base64,${image.dataBase64}`;
/** Most recently updated first. */
export const sortChats = (chats: AiChatSummary[]) =>
  [...chats].sort((a, b) => b.updatedAt.localeCompare(a.updatedAt));
/** The assistant's avatar in the conversation. */
export function AiCatAvatar() {
  return (
    <svg
      width="22"
      height="22"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.65"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      <path d="M5 9 4.5 4.8Q4.4 3.8 5.4 4.2L9 6.1Q12 5.2 15 6.1L18.6 4.2Q19.6 3.8 19.5 4.8L19 9Q21 11 21 14.5Q21 21 12 21Q3 21 3 14.5Q3 11 5 9Z" />
    </svg>
  );
}
