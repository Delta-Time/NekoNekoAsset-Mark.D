import { table } from './messages';
import type { Locale } from './model';

type BuiltinAiPresetId =
  | 'chat'
  | 'similar'
  | 'outfit'
  | 'accessories'
  | 'purchase'
  | 'group'
  | 'style';
interface BuiltinAiPreset {
  id: BuiltinAiPresetId;
  title: string;
  description: string;
  systemPrompt: string;
  builtin: true;
}
const presets: {
  id: BuiltinAiPresetId;
  systemPrompt: string;
}[] = [
  {
    id: 'chat',
    systemPrompt:
      'Answer the user’s VRChat and digital-asset questions conversationally. Recommend products only when useful and supported by the supplied candidates.',
  },
  {
    id: 'similar',
    systemPrompt:
      'Find clothing or accessories similar to the user’s reference image or description. Compare silhouette, colors, materials and visible details, and explain the closest supported matches.',
  },
  {
    id: 'outfit',
    systemPrompt:
      'Suggest a coordinated VRChat outfit from the supplied candidates. Explain how clothing, accessories, colors and proportions work together and state compatibility uncertainty.',
  },
  {
    id: 'accessories',
    systemPrompt:
      'Suggest accessories that complement the user’s avatar or outfit. Consider style, scale, color and supported compatibility evidence.',
  },
  {
    id: 'purchase',
    systemPrompt:
      'Help the user evaluate whether a candidate asset suits their needs. Compare visible features, stated compatibility and supplied price evidence; do not claim ownership or purchase anything.',
  },
  {
    id: 'group',
    systemPrompt:
      'Suggest coordinated outfits for a group, considering a shared theme, each avatar’s supported compatibility and individual variations.',
  },
  {
    id: 'style',
    systemPrompt:
      'Propose outfit options in a different style from the user’s reference. Describe the new direction and recommend only supported candidates.',
  },
];
export function getBuiltinAiPresets(locale: Locale): BuiltinAiPreset[] {
  const titles = table(locale, 'aiPresetTitle');
  const descriptions = table(locale, 'aiPresetDescription');
  return presets.map(({ id, systemPrompt }) => ({
    id,
    title: titles[id],
    description: descriptions[id],
    systemPrompt,
    builtin: true,
  }));
}
