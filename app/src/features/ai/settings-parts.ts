import type { AiScopes } from '../../backend';

export function savedScopes(value: unknown): AiScopes {
  const saved = value && typeof value === 'object' ? (value as Partial<AiScopes>) : {};
  return {
    title: saved.title !== false,
    tags: saved.tags !== false,
    description: saved.description !== false,
    files: saved.files !== false,
    images: saved.images === true,
  };
}
