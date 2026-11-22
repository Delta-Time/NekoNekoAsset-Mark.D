import type { Account, AssetFile, Product } from '../src/model';
import { categoryKeys, summarizeFiles } from '../src/model';

// Independently authored design fixtures. No record, title, identity, preview or
// filename comes from a real store or library. Nothing is fetched.
const concepts = [
  ['折り紙の庭 / Folded Garden', 'World'],
  ['余白のコート / Margin Coat', 'Clothes'],
  ['Paper Kite Atelier', 'Accessory'],
  ['Glassline Hair Study', 'Hair'],
  ['小さな灯台の部屋', 'World'],
  ['雨粒のブローチ / Raindrop Pin', 'Accessory'],
  ['Loomwork Material Set', 'Texture'],
  ['歩幅のスケッチ', 'Motion'],
  ['Pebble Avatar Study', 'Base'],
  ['机上の天体模型', 'Accessory'],
  ['Quiet Switch Laboratory', 'Gimmick'],
  ['影の輪郭ツール', 'Tool'],
  ['Cloudstep Room Controls', 'Udon'],
  ['Print Studio Brush Set', 'NonVRChat'],
  ['月明かりの襟と袖のセット', 'Clothes'],
  ['Ribbon Field Notes', 'NonVRChat'],
  ['積み木の駅舎', 'World'],
  ['Fernleaf Hair', 'Hair'],
  ['Woven Pocket Collection', 'Clothes'],
  ['紺碧ではない灰色の街', 'World'],
  ['Orbit Notes / Floating Desk Objects', 'Accessory'],
  ['透明な窓辺の素材集', 'Texture'],
  ['Threadline Gesture Pack', 'Motion'],
  ['Paperwing Attachment Kit', 'Accessory'],
] as const;
const shops = [
  '余白制作室',
  'Paperplane Workshop',
  '小石工房',
  'Line & Loom',
  '窓辺の研究室',
  'Folded Field',
];
export const syntheticAvatars = [
  '紙羽 / Kamihane',
  'こいし / Koishi Study',
  'ルーム / Loom',
  'しずく / Shizuku Study',
];
export const demoAccounts: Account[] = [
  { id: 'demo-booth-main', name: 'Paper Library', store: 'BOOTH', status: 'connected' },
  { id: 'demo-booth-second', name: 'Studio Library', store: 'BOOTH', status: 'connected' },
  { id: 'demo-booth-third', name: 'Study Library', store: 'BOOTH', status: 'connected' },
  { id: 'demo-gumroad', name: 'Sketch Library', store: 'Gumroad', status: 'connected' },
];
const tags = [
  'handmade',
  'soft',
  'geometric',
  'casual',
  'monochrome',
  'minimal',
  '室内',
  '衣装',
  '小物',
  '素材',
  '夜景',
  '試作',
];
const syntheticLength = (text: string, length: number) =>
  Array.from(text.repeat(Math.ceil(length / Array.from(text).length)))
    .slice(0, length)
    .join('');
const fileTypes = ['unitypackage', 'zip', 'psd', 'fbx', 'pdf', 'png', '7z', 'rar'];
function makeFiles(index: number, total: number, state: string): AssetFile[] {
  return Array.from({ length: total }, (_, fileIndex) => {
    const type = fileTypes[fileIndex % fileTypes.length];
    let status = state === 'ready' || state === 'sourceRemoved' ? 'ready' : 'notDownloaded';
    if (state === 'partial') status = fileIndex < total - 1 ? 'ready' : 'failed';
    if (state === 'attention') status = fileIndex === total - 1 ? 'extractionFailed' : 'ready';
    if (state === 'extracting') status = fileIndex === total - 1 ? 'extracting' : 'ready';
    if (state === 'unknown') status = 'unknown';
    if (state === 'queued' || state === 'paused') status = state;
    const stem =
      fileIndex % 4 === 0
        ? '原本_構造を保持した合成サンプル'
        : ['sample_source', 'materials', 'readme', 'sample_legacy'][fileIndex % 4];
    return {
      id: `demo-file-${index + 1}-${fileIndex + 1}`,
      name: `${stem}_${String(fileIndex + 1).padStart(2, '0')}.${type}`,
      type,
      size:
        state === 'unknown' ? '—' : `${(3.5 + ((index * 7 + fileIndex * 11) % 260)).toFixed(1)} MB`,
      status,
    };
  });
}
export function createProducts(count = 192): Product[] {
  const states = [
    'ready',
    'notDownloaded',
    'ready',
    'partial',
    'notDownloaded',
    'ready',
    'queued',
    'ready',
    'paused',
    'ready',
    'attention',
    'notDownloaded',
  ];
  const products = Array.from(
    { length: Math.max(0, Math.min(10000, Math.floor(count))) },
    (_, index): Product => {
      const concept = concepts[index % concepts.length];
      const edition = Math.floor(index / concepts.length) + 1;
      const state =
        index === 0
          ? 'partial'
          : index === 1
            ? 'ready'
            : index === 2
              ? 'attention'
              : index === 3
                ? 'unknown'
                : index === 4
                  ? 'sourceRemoved'
                  : index === 5
                    ? 'extracting'
                    : states[index % states.length];
      const total = index === 10 ? 68 : index === 0 ? 30 : index === 3 ? 1 : 1 + ((index * 7) % 19);
      const files = makeFiles(index, total, state);
      if (index === 9)
        files[0].name =
          syntheticLength('合成資料_原形を保持した構造検証用の長いファイル名_', 37) +
          '.unitypackage';
      if (index === 8)
        files[0].name =
          syntheticLength('Synthetic_structure_preservation_sample_', 29) + '.unitypackage';
      const manifestComplete = state !== 'unknown';
      const summary = summarizeFiles(files, manifestComplete);
      const avatar = syntheticAvatars[index % syntheticAvatars.length];
      const accountIds =
        index % 5 === 0
          ? [demoAccounts[0].id, demoAccounts[1].id]
          : [demoAccounts[index % demoAccounts.length].id];
      const title =
        index === 9
          ? '【合成資料】机上の天体模型と透明な台座・構造保持用の長い商品名 / Orbital Desk Objects — Source Files and Documentation Collection'
          : `${concept[0]}${edition > 1 ? ` · Study ${String(edition).padStart(2, '0')}` : ''}`;
      return {
        id: `demo-${String(index + 1).padStart(4, '0')}`,
        newlyDiscovered: index % 31 === 0,
        title:
          index === 9
            ? syntheticLength(
                '合成資料・紙と線の庭・原形を保持する制作研究 / Paper and Line Garden — ',
                181,
              )
            : index === 8
              ? syntheticLength('合成資料・窓辺の研究室 / Window Study ', 52)
              : title,
        sourceTitle:
          index === 9
            ? syntheticLength(
                '合成資料・紙と線の庭・原形を保持する制作研究 / Paper and Line Garden — ',
                181,
              )
            : index === 8
              ? syntheticLength('合成資料・窓辺の研究室 / Window Study ', 52)
              : title,
        shop: shops[index % shops.length],
        category: concept[1],
        accountIds,
        tags:
          index === 11 || index === 12
            ? Array.from(
                { length: index === 11 ? 100 : 59 },
                (_, j) => `synthetic-study-${String(j + 1).padStart(3, '0')}`,
              )
            : Array.from(
                { length: index === 8 ? 10 : 2 + (index % 4) },
                (_, j) => tags[(index + j) % tags.length],
              ),
        avatar,
        listingAvatars:
          index % 11 === 3 ? [] : [avatar, syntheticAvatars[(index + 1) % syntheticAvatars.length]],
        ownedAvatars: index % 7 === 3 ? [] : [avatar],
        ownedAvatarEvidence:
          index % 7 === 3
            ? []
            : [{ avatar, source: 'accountVariant', accountId: accountIds[0], fileId: files[0].id }],
        files,
        status: state,
        manifestComplete,
        downloaded: summary.downloaded,
        total,
        price: index % 13 === 0 ? null : index % 9 === 0 ? 0 : 300 + ((index * 137) % 24) * 100,
        acquired: `2026-${String(10 - Math.floor((index % 192) / 24)).padStart(2, '0')}-${String(1 + (index % 28)).padStart(2, '0')}`,
        note:
          index === 0
            ? 'Synthetic working set — retry the remaining file.'
            : index === 4
              ? 'Last known source retained; local package is still available.'
              : index % 17 === 0
                ? 'Keep original filenames and folder structure.'
                : '',
        sensitive: index > 15 && index % 23 === 0,
        kind: index === 3 || index % 19 === 0 ? -1 : index % 8,
      };
    },
  );
  // Every approved category appears; custom assets have no guessed source identity.
  categoryKeys.forEach((category, index) => {
    const product = products[products.length - 1 - index];
    if (product) product.category = category;
  });
  return products;
}
