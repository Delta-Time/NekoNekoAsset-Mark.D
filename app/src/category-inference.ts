import {
  type categoryKeys,
  type SourceCategory,
  type CategoryInference,
  normalizeSearch,
} from './model';

// The App does not call these: the native taxonomy decides categories.
// Original best-fit rules. Specific component evidence outranks a broad avatar/world parent.
const categoryRules: {
  category: (typeof categoryKeys)[number];
  pattern: RegExp;
  specific: boolean;
}[] = [
  {
    category: 'Udon',
    pattern: /\budon(?:sharp)?\b|udonシステム|udon系统|udon系統/i,
    specific: true,
  },
  { category: 'Shader', pattern: /\bshaders?\b|シェーダー?|셰이더|着色器|著色器/i, specific: true },
  { category: 'Hair', pattern: /\bhair(?:styles?)?\b|髪型|ヘア|헤어|发型|髮型/i, specific: true },
  {
    category: 'Clothes',
    pattern:
      /\b(?:cloth(?:es|ing)?|outfits?|costumes?|coats?|dresses?)\b|衣装|衣服|服装|服裝|의상/i,
    specific: true,
  },
  {
    category: 'Accessory',
    pattern:
      /\b(?:accessor(?:y|ies)|props?|brooch(?:es)?|jewel(?:ry|lery))\b|アクセサリー|小物|配饰|配飾|액세서리/i,
    specific: true,
  },
  {
    category: 'Texture',
    pattern:
      /\b(?:textures?|materials?)\b|テクスチャ|マテリアル|素材|贴图|貼圖|材质|材質|텍스처|머티리얼/i,
    specific: true,
  },
  {
    category: 'Motion',
    pattern:
      /\b(?:motions?|animations?|gestures?|poses?)\b|モーション|アニメーション|动作|動作|모션|애니메이션/i,
    specific: true,
  },
  {
    category: 'Tool',
    pattern: /\b(?:tools?|utilities|utility|editor extension)\b|ツール|工具|도구/i,
    specific: true,
  },
  {
    category: 'Gimmick',
    pattern: /\b(?:gimmicks?|interactions?)\b|ギミック|交互组件|互動元件|기믹/i,
    specific: true,
  },
  {
    category: 'World',
    pattern: /\b(?:worlds?|environments?)\b|ワールド|世界|월드/i,
    specific: false,
  },
  {
    category: 'Base',
    pattern: /\b(?:avatar(?: base)?|base avatar|3d characters?)\b|アバター|素体|素體|아바타/i,
    specific: false,
  },
  {
    category: 'NonVRChat',
    pattern:
      /\b(?:non[ -]?vrchat|brush(?:es)?|print(?:ing)?|illustrations?)\b|VRChat以外|イラスト|笔刷|筆刷/i,
    specific: true,
  },
];
export function inferCategoryEvidence(
  sourceCategory: SourceCategory | string | undefined,
  tags: readonly string[] = [],
): CategoryInference {
  const leaf = typeof sourceCategory === 'string' ? sourceCategory : (sourceCategory?.name ?? '');
  const parent = typeof sourceCategory === 'string' ? '' : (sourceCategory?.parentName ?? '');
  let winner: CategoryInference = { category: 'NonVRChat', basis: 'fallback' };
  let best = 0;
  for (const rule of categoryRules) {
    const leafMatch = rule.pattern.test(normalizeSearch(leaf));
    const tagMatch = tags.some((tag) => rule.pattern.test(normalizeSearch(tag)));
    const parentMatch = rule.pattern.test(normalizeSearch(parent));
    const score =
      (leafMatch ? 6 : tagMatch ? 4 : parentMatch ? 2 : 0) +
      ((leafMatch || tagMatch || parentMatch) && rule.specific ? 5 : 0);
    if (score > best) {
      best = score;
      winner = {
        category: rule.category,
        basis: leafMatch || (parentMatch && !tagMatch) ? 'sourceCategory' : 'tags',
      };
    }
  }
  return winner;
}
/** Inferred classification never establishes VRChat compatibility or avatar ownership. */
export function inferCategory(
  sourceCategory: SourceCategory | string | undefined,
  tags: readonly string[] = [],
): (typeof categoryKeys)[number] {
  return inferCategoryEvidence(sourceCategory, tags).category as (typeof categoryKeys)[number];
}
