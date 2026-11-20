import { builtInTheme } from '../../EditDialogs';

export type Preset = { id: string; name: string; tokens: Record<string, string> };

/** Every theme token, with the built-in theme's colors. */
export const neutralTokens = Object.fromEntries(
  Object.entries(builtInTheme).map(([key, value]) => ['--' + key, value]),
);
export const initialPresets: Preset[] = [
  {
    id: 'mint',
    name: 'Mint',
    tokens: {
      '--secondary-accent': '#375e9a',
      '--secondary-accent-text': '#ffffff',
      '--bg': '#eef5f0',
      '--panel': '#f8fcf9',
      '--raised': '#ffffff',
      '--border': '#acbfb2',
      '--text': '#20392d',
      '--muted': '#496454',
      '--accent': '#306449',
      '--control': '#dceadf',
      '--positive': '#326545',
      '--warning': '#815516',
      '--negative': '#a33745',
    },
  },
  {
    id: 'violet',
    name: 'Violet',
    tokens: {
      '--secondary-accent': '#8ebdec',
      '--secondary-accent-text': '#211b29',
      '--bg': '#211b29',
      '--panel': '#2c2337',
      '--raised': '#382d45',
      '--border': '#665573',
      '--text': '#f5edf9',
      '--muted': '#c0adcf',
      '--accent': '#d2b5ee',
      '--control': '#493957',
      '--positive': '#aad3b4',
      '--warning': '#e5c386',
      '--negative': '#f0acb9',
    },
  },
  {
    id: 'sky',
    name: 'Sky',
    tokens: {
      '--secondary-accent': '#6a43a5',
      '--secondary-accent-text': '#ffffff',
      '--bg': '#eaf2f8',
      '--panel': '#f6faff',
      '--raised': '#ffffff',
      '--border': '#a8bdcd',
      '--text': '#20394d',
      '--muted': '#496174',
      '--accent': '#315d85',
      '--control': '#d5e5f1',
      '--positive': '#326448',
      '--warning': '#825619',
      '--negative': '#a33644',
    },
  },
  {
    id: 'amber',
    name: 'Amber',
    tokens: {
      '--secondary-accent': '#96bfea',
      '--secondary-accent-text': '#292117',
      '--bg': '#292117',
      '--panel': '#352c20',
      '--raised': '#433728',
      '--border': '#75644a',
      '--text': '#f6edda',
      '--muted': '#c9b693',
      '--accent': '#f0cb83',
      '--control': '#50412e',
      '--positive': '#b8d29a',
      '--warning': '#f1cc8b',
      '--negative': '#f0a6a0',
    },
  },
  {
    id: 'rose',
    name: 'Rose',
    tokens: {
      '--secondary-accent': '#416eb4',
      '--secondary-accent-text': '#ffffff',
      '--bg': '#f7eef1',
      '--panel': '#fff8fa',
      '--raised': '#ffffff',
      '--border': '#cbb0ba',
      '--text': '#4b2d39',
      '--muted': '#765565',
      '--accent': '#8e3e62',
      '--control': '#eddce3',
      '--positive': '#38634c',
      '--warning': '#865617',
      '--negative': '#aa3047',
    },
  },
  {
    id: 'silver',
    name: 'Silver',
    tokens: {
      '--secondary-accent': '#7cb9f2',
      '--secondary-accent-text': '#10161c',
      '--bg': '#10161c',
      '--panel': '#1b242d',
      '--raised': '#283440',
      '--border': '#526373',
      '--text': '#e9f1f8',
      '--muted': '#abbccc',
      '--accent': '#c3d4e3',
      '--control': '#354554',
      '--positive': '#a6d4bb',
      '--warning': '#e2c28b',
      '--negative': '#eda6ad',
    },
  },
];

/** A stored preset's tokens when every key is a theme token and every value a hex color. */
export function safeTokens(value: unknown): Record<string, string> | null {
  if (!value || typeof value !== 'object' || Array.isArray(value)) return null;
  const entries = Object.entries(value);
  if (
    !entries.length ||
    entries.some(
      ([key, color]) =>
        !Object.hasOwn(neutralTokens, key) ||
        typeof color !== 'string' ||
        !/^#[a-f0-9]{6}$/i.test(color),
    )
  )
    return null;
  return { ...neutralTokens, ...value };
}
