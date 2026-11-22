// Focused deterministic assertions; no framework or visual acceptance claims.
import assert from 'node:assert/strict';
import {
  i18nUrl,
  localeFiles,
  model,
  modelUrl,
  moduleUrl,
  readTs,
  withMessages,
} from './modules.mjs';
assert.equal(
  model.currentPriceLabel('en', { price: 0, priceCurrency: 'JPY', currentPriceText: '¥500~' }),
  '¥0',
);
assert.equal(
  model.currentPriceLabel('en', { price: 1200, priceCurrency: 'JPY', currentPriceText: '¥500~' }),
  '¥1,200',
);
assert.equal(
  model.currentPriceLabel('en', {
    price: 12.5,
    priceCurrency: 'USD',
    currentPriceText: 'USD 12.5',
  }),
  '$12.50',
);
assert.equal(
  model.currentPriceLabel('en', { price: 1200, priceCurrency: 'JPY', currentPriceText: null }),
  '¥1,200',
);
assert.equal(
  model.currentPriceLabel('en', { price: 0, priceCurrency: 'USD', currentPriceText: null }),
  '$0.00',
);
assert.equal(
  model.currentPriceLabel('en', { price: 0, priceCurrency: 'JPY', currentPriceText: null }),
  '¥0',
);
assert.equal(
  model.currentPriceLabel('en', {
    price: 12.5,
    priceCurrency: 'ZZZ',
    currentPriceText: ' USD 12.5 ',
  }),
  'USD 12.5',
);
assert.equal(
  model.currentPriceLabel('en', { price: 12.5, priceCurrency: 'ZZZ', currentPriceText: null }),
  null,
);
assert.equal(model.currentPriceLabel('en', { price: null, currentPriceText: ' ¥500~ ' }), '¥500~');
for (const price of [undefined, NaN, Infinity, -1, 0.5, Number.MAX_SAFE_INTEGER + 1]) {
  assert.equal(model.currentPriceLabel('en', { price, currentPriceText: null }), null);
  assert.equal(model.currentPriceLabel('en', { price, currentPriceText: '¥500~' }), '¥500~');
  assert.equal(model.priceLabel('en', price, 'JPY'), null);
}
assert.equal(model.currentPriceLabel('en', { price: null, currentPriceText: '  ' }), null);
assert.equal(
  model.currentPriceLabel('en', {
    provider: 'local',
    price: 1800,
    priceCurrency: 'JPY',
    currentPriceText: null,
  }),
  '¥1,800',
);
assert.equal(
  model.currentPriceLabel('en', {
    provider: 'local',
    price: null,
    priceCurrency: 'JPY',
    currentPriceText: '¥1,000',
  }),
  '¥1,000',
);
assert.equal(
  model.currentPriceLabel('en', {
    provider: 'local',
    price: null,
    priceCurrency: null,
    currentPriceText: null,
  }),
  null,
);
assert.equal(model.onlyGifts([{ source: 'gift' }, { source: 'gift' }]), true);
assert.equal(model.onlyGifts([{ source: 'gift' }, { source: 'library' }]), false);
assert.equal(model.onlyGifts([]), false);
const giftAndFree = [
  { source: 'gift', accountId: 'a', variantIds: ['1'] },
  { source: 'free', accountId: 'a', variantIds: ['2'] },
  { source: 'gift', accountId: 'b' },
  { source: 'library', accountId: 'c' },
];
assert.equal(model.giftedVariant(giftAndFree, 'a', '1'), true, 'A gift bound this variant');
assert.equal(
  model.giftedVariant(giftAndFree, 'a', '2'),
  false,
  'The free variant stays unknown beside it',
);
assert.equal(
  model.giftedVariant(giftAndFree, 'b', '9'),
  true,
  'An account holding only gifts needs no binding',
);
assert.equal(model.giftedVariant(giftAndFree, 'c', '9'), false);
assert.equal(model.giftedVariant(giftAndFree, 'z', '9'), false);
assert.equal(model.priceLabel('en', 0, 'JPY'), '¥0');
assert.equal(model.priceLabel('en', 12.5, 'USD'), '$12.50');
assert.equal(model.priceLabel('en', 12.5, null), null);
assert.equal(model.priceLabel('en', 12.5, 'ZZZ'), null);
const cardVariant = (variantId, name, price, currency) => ({
  variantId,
  name,
  price,
  currency,
  accountIds: ['authored-account'],
});
const cardPrices = {
  price: null,
  priceCurrency: null,
  ownedCurrentPrices: [
    cardVariant('alpha', 'Long authored Alpha variant name', 1500, 'JPY'),
    cardVariant('beta', 'Long authored Beta variant name', 2500, 'JPY'),
  ],
};
assert.equal(
  model.cardPriceLabel('en', cardPrices),
  '¥1,500 / ¥2,500',
  'Card retains separate numeric prices without variant names, sums or ranges',
);
assert.equal(
  model.cardPriceLabel('en', {
    ...cardPrices,
    ownedCurrentPrices: [
      cardVariant('alpha', 'First variant', 1500, 'JPY'),
      cardVariant('beta', 'Different variant same price', 1500, 'JPY'),
    ],
  }),
  '¥1,500',
  'Equal formatted amounts deduplicate independently of variant identity/name',
);
assert.equal(
  model.cardPriceLabel('en', {
    ...cardPrices,
    ownedCurrentPrices: [
      cardVariant('alpha', 'Known price', 1500, 'JPY'),
      cardVariant('unknown', 'Unknown price', null, null),
      cardVariant('unknown2', 'Another unknown price', null, 'JPY'),
    ],
  }),
  '¥1,500 / —',
  'Unknown rows remain visible beside known amounts without repeated dashes',
);
assert.equal(
  model.cardPriceLabel('en', {
    ...cardPrices,
    price: 9999,
    priceCurrency: 'JPY',
    ownedCurrentPrices: [cardVariant('unknown', 'Unknown owned variant', null, null)],
  }),
  null,
  'Unknown owned rows do not borrow an unrelated listing price',
);
assert.equal(
  model.cardPriceLabel('en', {
    ...cardPrices,
    ownedCurrentPrices: [
      cardVariant('jpy', 'JPY variant', 1500, 'JPY'),
      cardVariant('usd', 'USD variant', 10.25, 'USD'),
      cardVariant('unknown', 'Missing currency', 10, null),
    ],
  }),
  '¥1,500 / $10.25 / —',
  'Mixed currencies remain distinct and missing currency is unknown',
);
assert.equal(
  model.cardPriceLabel('en', {
    ...cardPrices,
    ownedCurrentPrices: [
      cardVariant('jpy', 'Zero yen', 0, 'JPY'),
      cardVariant('usd', 'Zero dollars', 0, 'USD'),
    ],
  }),
  '¥0 / $0',
  'Known zero is retained in each established currency',
);
assert.equal(
  model.cardPriceLabel('en', { price: 1200, priceCurrency: 'JPY', currentPriceText: '¥500~' }),
  '¥1,200',
);
assert.equal(
  model.cardPriceLabel('en', { price: null, priceCurrency: 'JPY', currentPriceText: '¥500~' }),
  null,
  'Card never falls back to listing range/from text',
);
assert.equal(
  model.cardPriceLabel('en', { price: 1200, priceCurrency: null }),
  null,
  'Card never invents a currency',
);
assert.equal(
  model.cardPriceLabel('en', { price: 1.234, priceCurrency: 'KWD' }),
  'KWD\u00a01.234',
  'Card retains the currency precision while omitting unnecessary trailing zeroes',
);
console.log(
  'PASS: numeric-only owned variant card prices, equal-label dedupe, preserved unknown rows, mixed currencies/known zero and no listing sums/ranges/fallback.',
);
const { nativeErrorText } = await import(
  moduleUrl(withMessages(await readTs('../src/error-i18n.ts')))
);
for (const locale of ['en', 'ja', 'ko', 'zh-Hans', 'zh-Hant']) {
  for (const code of [
    'STORE_IDENTITY_CHANGED',
    'STORE_IDENTITY_MISMATCH',
    'ACCOUNT_IDENTITY_MISMATCH',
  ])
    assert.equal(nativeErrorText(locale, code), nativeErrorText(locale, 'auth'));
  for (const provider of ['JINXXY', 'GUMROAD'])
    for (const suffix of ['SOURCE_UNSUPPORTED', 'SCOPE_CHANGED', 'PAGINATION_UNPROVEN'])
      assert.equal(
        nativeErrorText(locale, `${provider}_${suffix}`),
        nativeErrorText(locale, 'operationFailed'),
      );
  for (const code of ['FILE_UNAVAILABLE', 'DOWNLOAD_UNAVAILABLE', 'DOWNLOADS_UNAVAILABLE'])
    assert.equal(nativeErrorText(locale, code), nativeErrorText(locale, 'missing'));
  for (const code of ['unsupported_image', 'unsafe_link'])
    assert.equal(nativeErrorText(locale, code), nativeErrorText(locale, 'validation'));
  assert.ok(nativeErrorText(locale, 'COLLECTOR_UNSUPPORTED'));
  assert.notEqual(
    nativeErrorText(locale, 'COLLECTOR_UNSUPPORTED'),
    nativeErrorText(locale, 'validation'),
  );
  for (const code of ['UNSAFE_DOWNLOAD_URL', 'UNSAFE_REDIRECT', 'REDIRECT_LIMIT'])
    assert.equal(nativeErrorText(locale, code), nativeErrorText(locale, 'operationFailed'));
  for (const code of [
    'archive_listing_invalid',
    'archive_inventory_mismatch',
    'destination_not_fresh',
  ]) {
    const text = nativeErrorText(locale, code);
    assert.ok(text, `${locale}: ${code}`);
    assert.notEqual(text, nativeErrorText(locale, 'operationFailed'));
    assert.notEqual(text, nativeErrorText(locale, 'archive'));
    assert.equal(nativeErrorText(locale, `storage.${code}`), text);
    assert.equal(nativeErrorText(locale, code.toUpperCase()), text);
  }
}
console.log(
  'PASS: mixed USD/JPY current prices, known zero, established currency/listing-text fallback; precise store identity/source/download/selected-file errors across five locales.',
);
const data = await import(
  moduleUrl((await readTs('./data.ts')).replace("from '../src/model'", `from '${modelUrl}'`))
);
const { locales, t } = await import(i18nUrl);
const dictionaries = Object.fromEntries(
  Object.entries(localeFiles).map(([locale, text]) => [locale, JSON.parse(text)]),
);
const products = data.createProducts();
assert.equal(products.length, 192);
assert.equal(new Set(products.map((p) => p.id)).size, products.length);
assert.ok(
  products.every(
    (p) => p.id.startsWith('demo-') && p.files.every((f) => f.id.startsWith('demo-file-')),
  ),
);
assert.equal(
  model.summarizeFiles(products[0].files, true).status,
  'attention',
  'An actually failed member remains an actionable failure',
);
const pending29Of30 = products[0].files.map((file) =>
  ['failed', 'missing'].includes(file.status) ? { ...file, status: 'notDownloaded' } : file,
);
assert.deepEqual(model.summarizeFiles(pending29Of30, true), {
  downloaded: 29,
  total: 30,
  status: 'partial',
  usable: false,
});
assert.equal(model.summarizeFiles(products[2].files, true).status, 'attention');
assert.equal(model.summarizeFiles(products[2].files, true).downloaded, products[2].total);
assert.equal(model.summarizeFiles(products[3].files).status, 'unknown');
assert.equal(model.isProductDownloaded(products[0]), false, '29 of 30 is not downloaded');
assert.equal(model.isProductDownloaded(products[1]), true);
assert.equal(
  model.isProductDownloaded(products[2]),
  true,
  'Extraction failure retains complete bytes without Ready',
);
assert.notEqual(model.getProductDisplayStatus(products[2]), 'ready');
assert.equal(
  model.isProductDownloaded({ ...products[1], manifestComplete: false }),
  true,
  'Attached byte-complete files remain downloaded while source coverage is incomplete',
);
assert.equal(model.isProductDownloaded({ ...products[1], files: [] }), false);
const ephemeral = {
  ...products[1],
  thumbnailOverrideUrl: 'blob:synthetic-draft',
  sourcePreviewUrl: 'blob:synthetic-source',
  previewUrl: 'blob:synthetic-public',
};
assert.equal(
  Object.hasOwn(model.stripEphemeralThumbnail(ephemeral), 'thumbnailOverrideUrl'),
  false,
);
assert.equal(Object.hasOwn(model.stripEphemeralThumbnail(ephemeral), 'sourcePreviewUrl'), false);
assert.equal(Object.hasOwn(model.stripEphemeralThumbnail(ephemeral), 'previewUrl'), false);
assert.equal(
  ephemeral.thumbnailOverrideUrl,
  'blob:synthetic-draft',
  'Persistence projection does not mutate live state',
);
assert.equal(model.summarizeFiles(products[1].files, true).usable, true);
assert.ok(model.productMatches(products[0], 'ＦＯＬＤＥＤ'));
assert.ok(model.productMatches(products[0], '原本_構造'));
assert.ok(model.categoryKeys.every((key) => products.some((p) => p.category === key)));
const edited = model.applyProductPatch(products, [products[0].id], {
  note: 'My synthetic note',
  tags: ['chosen'],
});
assert.equal(edited.products[1], products[1]);
const synced = model.refreshSource(edited.products[0], {
  title: 'Synthetic source revision',
  shop: 'Synthetic updated shop',
});
assert.equal(synced.note, 'My synthetic note');
assert.deepEqual(synced.tags, ['chosen']);
assert.equal(synced.sourceTitle, 'Synthetic source revision');
assert.equal(
  synced.title,
  'Synthetic source revision',
  'A note edit must not freeze an untouched title',
);
const renamed = model.applyProductPatch(products, [products[0].id], {
  title: 'My chosen synthetic title',
});
const renamedSync = model.refreshSource(renamed.products[0], { title: 'Changed source title' });
assert.equal(renamedSync.title, 'My chosen synthetic title');
assert.equal(renamedSync.sourceTitle, 'Changed source title');
const reset = model.resetSourceTitle({
  ...renamedSync,
  note: 'Retained correction',
  tags: ['Retained tag'],
});
const resetSync = model.refreshSource(reset, { title: 'Next source title' });
assert.equal(resetSync.title, 'Next source title');
assert.equal(resetSync.note, 'Retained correction');
assert.deepEqual(resetSync.tags, ['Retained tag']);
assert.equal(resetSync.titleOverride, false);
const renameUndo = model.undoProductPatch(renamed.products, renamed.undo);
assert.equal(renameUndo.products[0].titleOverride, products[0].titleOverride);
assert.equal(
  model.refreshSource(renameUndo.products[0], { title: 'After undo source' }).title,
  'After undo source',
);
const afterUnrelatedEdit = edited.products.map((p, index) =>
  index === 0 ? { ...p, category: 'Shader' } : p,
);
const undone = model.undoProductPatch(afterUnrelatedEdit, edited.undo);
assert.deepEqual(undone.conflicts, []);
assert.equal(undone.products[0].note, products[0].note);
assert.equal(undone.products[0].category, 'Shader');
const conflicting = edited.products.map((p, index) =>
  index === 0 ? { ...p, note: 'A later edit' } : p,
);
const conflictUndo = model.undoProductPatch(conflicting, edited.undo);
assert.deepEqual(conflictUndo.conflicts, [products[0].id]);
assert.equal(conflictUndo.products[0].note, 'A later edit');
for (const locale of locales) {
  assert.deepEqual(Object.keys(dictionaries[locale]), Object.keys(dictionaries.en));
  assert.ok(
    Object.values(dictionaries[locale]).every((value) => typeof value === 'string' && value.length),
  );
  assert.equal(t(locale, 'absent_key'), 'absent_key');
}
console.log(
  `PASS: synthetic identity/counts, 29/30 aggregation, extraction/unknown states, search, category coverage, scoped undo/conflicts, source overlay preservation, ${Object.keys(dictionaries.en).length} labels × ${locales.length} locales.`,
);
console.log('PASS: field-specific title override, source reset, title undo.');
assert.equal(
  model.summarizeFiles(products[1].files).status,
  'ready',
  'All attached usable files remain Ready independently of source coverage',
);
assert.equal(products[3].manifestComplete, false);
const large = data.createProducts(1882);
assert.equal(large.length, 1882);
assert.equal(new Set(large.map((p) => p.id)).size, 1882);
assert.ok(large.every((p) => /^2026-\d{2}-\d{2}$/.test(p.acquired)));
assert.equal(data.createProducts(10000).length, 10000);
assert.equal(data.createProducts(0).length, 0);
console.log(
  'PASS: source coverage independent of attached-file readiness and 1,882 / 10,000-product synthetic sizing.',
);
assert.equal(
  model.inferCategory({ name: 'Avatar', parentName: '3D' }, ['衣装']),
  'Clothes',
  'Specific tags outrank broad avatar category',
);
assert.equal(model.inferCategory({ name: 'World', parentName: '3D' }, ['UdonSharp']), 'Udon');
assert.equal(model.inferCategory('Hair', ['avatar']), 'Hair');
assert.equal(model.inferCategory(undefined, ['shader']), 'Shader');
assert.equal(model.inferCategory(undefined, ['unrecognized synthetic tag']), 'NonVRChat');
console.log('PASS: tags and source categories infer fixed best-fit categories.');

assert.equal(data.demoAccounts.length, 4);
assert.equal(data.demoAccounts.filter((account) => account.store === 'BOOTH').length, 3);
assert.equal(data.demoAccounts.filter((account) => account.store === 'Gumroad').length, 1);
assert.equal(new Set(data.demoAccounts.map((account) => account.id)).size, 4);
assert.ok(
  products.every(
    (product) =>
      product.accountIds.length > 0 &&
      product.accountIds.every((id) => data.demoAccounts.some((account) => account.id === id)),
  ),
);
assert.ok(model.accountMatches(products[0], null));
assert.equal(model.accountMatches(products[0], []), false);
assert.ok(model.accountMatches(products[0], [data.demoAccounts[1].id]));
assert.equal(model.accountMatches(products[0], [data.demoAccounts[3].id]), false);
assert.equal(
  model.accountMatches({ ...products[0], accountIds: undefined }, [data.demoAccounts[0].id]),
  false,
);

const sensitive = { ...products[0], sensitive: true };
const ordinary = { ...products[0], sensitive: false };
for (const mode of ['show', 'blur'])
  assert.deepEqual(
    [model.sensitiveMatches(sensitive, mode), model.sensitiveMatches(ordinary, mode)],
    [true, true],
  );
assert.deepEqual(
  [model.sensitiveMatches(sensitive, 'hide'), model.sensitiveMatches(ordinary, 'hide')],
  [false, true],
);
assert.deepEqual(
  [model.sensitiveMatches(sensitive, 'only'), model.sensitiveMatches(ordinary, 'only')],
  [true, false],
);
assert.equal(model.sensitiveMatches({ ...sensitive, sensitive: undefined }, 'only'), false);
assert.equal(
  model.sensitiveMatches(
    model.applyProductPatch([ordinary], [ordinary.id], { sensitive: true }).products[0],
    'only',
  ),
  true,
);
assert.equal(
  products.filter((product) => model.sensitiveMatches(product, 'only')).length,
  products.filter((product) => product.sensitive === true).length,
);
assert.equal([ordinary].filter((product) => model.sensitiveMatches(product, 'only')).length, 0);

assert.deepEqual(model.statusKeys, [
  'notDownloaded',
  'partial',
  'processing',
  'ready',
  'attention',
]);
assert.equal(model.getProductDisplayStatus(products[0]), 'attention');
const neverDownloaded = {
  ...products[1],
  status: 'notDownloaded',
  files: products[1].files.map((file) => ({ ...file, status: 'notDownloaded' })),
  downloaded: 0,
};
assert.equal(model.getProductDisplayStatus(neverDownloaded), 'notDownloaded');
assert.equal(
  model.getProductDisplayStatus({
    ...neverDownloaded,
    files: neverDownloaded.files.map((file) => ({ ...file, status: 'missing' })),
  }),
  'attention',
  'Actually missing local files are actionable even with zero remaining bytes',
);
assert.equal(model.summarizeFiles(neverDownloaded.files, true).status, 'notDownloaded');
assert.equal(
  model.summarizeFiles(
    neverDownloaded.files.map((file) => ({ ...file, status: 'missing' })),
    true,
  ).status,
  'attention',
);
assert.equal(model.getProductDisplayStatus(products[1]), 'ready');
assert.equal(model.getProductDisplayStatus(products[2]), 'attention');
assert.equal(model.getProductDisplayStatus(products[3]), 'notDownloaded');
assert.equal(
  model.getProductDisplayStatus(products[4]),
  'ready',
  'Removed source retains validated local files',
);
assert.equal(model.getProductDisplayStatus(products[5]), 'processing');
const pendingFiles = products[0].files.map((file) => ({ ...file, status: 'paused' }));
assert.equal(
  model.getProductDisplayStatus({
    ...products[0],
    status: 'paused',
    files: pendingFiles,
    downloaded: 0,
  }),
  'notDownloaded',
);
pendingFiles[0].status = 'ready';
assert.equal(
  model.getProductDisplayStatus({
    ...products[0],
    status: 'paused',
    files: pendingFiles,
    downloaded: 1,
  }),
  'partial',
  'Paused remaining files do not turn verified partial bytes into failure',
);
assert.equal(model.getProductDisplayStatus({ ...products[0], status: 'queued' }), 'processing');
assert.equal(
  model.getProductDisplayStatus({ ...products[1], manifestComplete: false }),
  'ready',
  'Verified usable attached files remain Ready before source closure',
);
assert.equal(
  model.getProductDisplayStatus({ ...products[2], manifestComplete: false }),
  'attention',
  'Known file errors remain actionable with an unverified manifest',
);
assert.equal(
  model.getProductDisplayStatus({
    ...products[1],
    files: products[1].files.map((file) => ({ ...file, status: 'downloaded' })),
  }),
  'partial',
  'Bytes alone do not validate extracted output or imply failure',
);

const ownershipProduct = {
  ...products[0],
  avatar: 'Offered / Offer',
  listingAvatars: ['Owned / Own', 'Offered / Offer'],
  ownedAvatars: ['Owned / Own'],
  ownedAvatarEvidence: [
    {
      avatar: 'Owned / Own',
      source: 'accountVariant',
      accountId: 'synthetic-account-A',
      fileId: 'synthetic-owned-file',
    },
  ],
  tags: ['Own', 'Offer', 'Offered / Offer', 'keep', 'rejected'],
  rejectedTags: ['rejected'],
};
assert.deepEqual(model.getEffectiveAvatarTags(ownershipProduct), ['Owned / Own']);
assert.deepEqual(model.getEffectiveAvatarTags(ownershipProduct, ['synthetic-account-B']), []);
assert.deepEqual(model.getEffectiveAvatarTags(ownershipProduct, []), []);
assert.deepEqual(
  model.getEffectiveAvatarTags({ ...ownershipProduct, provider: 'local' }, []),
  ['Owned / Own'],
  'Offline-only scope retains local compatibility evidence',
);
assert.deepEqual(
  model.getEffectiveAvatarTags({ ...ownershipProduct, custom: true }, []),
  [],
  'Editing a store product does not make its account evidence local',
);
assert.deepEqual(model.getEffectiveAvatarTags(ownershipProduct, ['synthetic-account-A']), [
  'Owned / Own',
]);
assert.deepEqual(model.getAvatarAliases('Owned / Own'), ['Owned / Own', 'Owned', 'Own']);
assert.equal(model.canonicalizeTag('ＭＡＨＵＹＵ'), '真冬 / Mafuyu');
assert.equal(model.canonicalizeTag('真冬對應'), '真冬 / Mafuyu');
assert.equal(model.canonicalizeTag('まふゆ'), '真冬 / Mafuyu');
assert.equal(model.canonicalizeTag('unknown custom tag'), 'unknown custom tag');
const aliasTags = {
  ...ownershipProduct,
  listingAvatars: ['真冬 / Mafuyu'],
  ownedAvatars: ['真冬 / Mafuyu'],
  tags: ['真冬', 'Mahuyu', 'まふゆ', '真冬用', '真冬對應', 'keep'],
  rejectedTags: ['Mafuyu'],
};
assert.deepEqual(
  model.getEffectiveTags(aliasTags),
  ['keep'],
  'Rejecting one known alias blocks synonymous suggestions without deleting raw evidence',
);
assert.deepEqual(
  [...new Set(aliasTags.tags.map(model.canonicalizeTag))],
  ['真冬 / Mafuyu', 'keep'],
  'Exact known aliases consolidate to the official name and English alias',
);
assert.deepEqual(
  model.getEffectiveTags({ ...aliasTags, rejectedTags: [], ownedAvatars: [] }),
  ['keep'],
  'Known listing aliases never establish owned compatibility',
);
assert.deepEqual(model.getEffectiveTags(ownershipProduct), ['Own', 'keep']);
assert.equal(
  model.productMatches(ownershipProduct, 'Offered'),
  false,
  'Listing-only avatar evidence must not match owned filters/search',
);
assert.deepEqual(
  model.getEffectiveAvatarTags({ ...ownershipProduct, ownedAvatars: [] }),
  [],
  'A manual removal remains authoritative over raw account evidence',
);
const tagEdit = model.applyProductPatch([ownershipProduct], [ownershipProduct.id], {
  tags: ['Own', 'keep'],
  rejectedTags: ['rejected'],
}).products[0];
assert.deepEqual(
  tagEdit.sourceTags,
  ownershipProduct.tags,
  'Editing effective tags retains raw source evidence',
);
const tagRefresh = model.refreshSource(tagEdit, { tags: ['rejected', 'Offer', 'new source tag'] });
assert.deepEqual(tagRefresh.tags, ['Own', 'keep']);
assert.deepEqual(tagRefresh.rejectedTags, ['rejected']);
assert.deepEqual(tagRefresh.sourceTags, ['rejected', 'Offer', 'new source tag']);
const importedProduct = {
  ...products[0],
  importSource: {
    kind: 'remote',
    location: 'https://example.invalid/synthetic-folder',
    files: ['synthetic-source.zip'],
    productUrl: 'https://example.invalid/synthetic-product',
    thumbnailName: 'synthetic-thumbnail.png',
  },
};
assert.deepEqual(
  model.refreshSource(
    model.applyProductPatch([importedProduct], [importedProduct.id], { note: 'Chosen note' })
      .products[0],
    { title: 'Updated synthetic title' },
  ).importSource,
  importedProduct.importSource,
  'Import evidence survives ordinary metadata edits/refresh without I/O',
);
console.log(
  'PASS: R-18 Show/Blur/Hide/Only and corrections; account scope; five display badges and readiness integrity; listing-only aliases suppressed, account-owned tags and raw source tag evidence retained.',
);

assert.ok(
  products.some((p) => p.newlyDiscovered === true && p.status !== 'notDownloaded'),
  'New acquisitions are independent of download state',
);
assert.ok(
  products.some((p) => !p.newlyDiscovered && p.status === 'notDownloaded'),
  'Old undownloaded assets are not newly discovered',
);
console.log('PASS: authored newly-discovered flag is independent of download status.');

// Third pass deterministic boundaries: direction cannot move unknown values to the front.
const sortRows = [
  { ...products[0], id: 'synthetic-sort-A', title: 'Alpha', acquired: '2026-01-01', price: 10 },
  { ...products[1], id: 'synthetic-sort-B', title: 'Beta', acquired: '2026-02-01', price: 20 },
  { ...products[3], id: 'synthetic-sort-unknown', title: 'Unknown', acquired: '', price: null },
];
for (const key of ['acquired', 'price'])
  for (const direction of ['asc', 'desc'])
    assert.equal(
      [...sortRows].sort((a, b) => model.compareProducts(a, b, key, direction, 'en')).at(-1).id,
      'synthetic-sort-unknown',
    );
assert.deepEqual(
  sortRows
    .slice(0, 2)
    .sort((a, b) => model.compareProducts(a, b, 'title', 'asc', 'en'))
    .map((p) => p.id),
  ['synthetic-sort-A', 'synthetic-sort-B'],
);
assert.equal(model.defaultSortDirection('acquired'), 'desc');
assert.equal(model.defaultSortDirection('title'), 'asc');
const statusRows = [products[1], products[3]];
for (const direction of ['asc', 'desc'])
  assert.equal(
    [...statusRows].sort((a, b) => model.compareProducts(a, b, 'status', direction, 'en')).at(-1),
    direction === 'asc' ? products[1] : products[3],
    'Not downloaded is a known display state and participates in directional sorting',
  );
assert.equal(model.thumbnailSizes.length, 9);
assert.equal(model.thumbnailSizes[0], 0);
assert.ok(model.thumbnailSizes.at(-1) > 224 && model.thumbnailSizes[1] < 112);
for (const width of [180, 420, 800, 1400])
  for (const size of model.thumbnailSizes)
    for (const density of [40, 74, 90]) {
      const g = model.resultGeometry(width, size, density);
      assert.ok(g.columns >= 1 && g.row > 0);
      if (size !== 0) {
        assert.ok(Math.abs(g.actual * g.columns + g.gap * (g.columns - 1) - (width - 24)) < 0.001);
        assert.equal(g.row, g.actual / 1.15 + g.info + g.gap);
      }
    }
for (const density of [40, 74, 90]) {
  const counts = [0, 1, 2].map((textRows) => model.resultGeometry(640, 128, density, textRows));
  assert.deepEqual(
    counts.map((g) => g.info),
    [74, 92, 110],
    'Only optional shop/acquired text rows add card information height',
  );
  assert.equal(counts[1].row - counts[0].row, 18);
  assert.equal(counts[2].row - counts[1].row, 18);
  assert.ok(
    counts.every((g) => g.actual === counts[0].actual && g.columns === counts[0].columns),
    'Optional text rows do not alter fitted thumbnail width/columns',
  );
  for (const textRows of [0, 1, 2])
    assert.equal(
      model.resultGeometry(640, 0, density, textRows).row,
      density,
      'List row height does not grow with optional card metadata',
    );
}
const originalCollections = { Existing: ['old', 'chosen'], Other: ['retained'] };
const addedCollections = model.addToCollections(
  originalCollections,
  ['Existing', 'New', 'Existing'],
  ['chosen', 'second'],
);
assert.deepEqual(addedCollections, {
  Existing: ['old', 'chosen', 'second'],
  Other: ['retained'],
  New: ['chosen', 'second'],
});
assert.deepEqual(originalCollections, { Existing: ['old', 'chosen'], Other: ['retained'] });
// Shipped-preset matching API was removed with pre-release compatibility handling.
console.log(
  'PASS: sorting directions/unknown-last, defaults, fitted stepped geometry/optional text rows and additive atomic collections.',
);

assert.equal(model.isLightTheme({ '--bg': '#eef5f0' }), true);
assert.equal(model.isLightTheme({ '--bg': '#211b29' }), false);
assert.equal(model.isLightTheme({ '--bg': 'invalid' }), false);
assert.deepEqual(
  model.addToCollections({}, ['__proto__'], ['synthetic-safe-name']),
  Object.fromEntries([['__proto__', ['synthetic-safe-name']]]),
);
console.log(
  'PASS: selected background chooses native light/dark controls and collection names remain own properties.',
);

const nonVR = { ...products[0], category: 'NonVRChat' },
  vrAsset = { ...products[0], category: 'Base' };
assert.equal(model.libraryCategoryMatches(nonVR, 'all'), false);
assert.equal(model.libraryCategoryMatches(nonVR, 'all', 'all', '', false), true);
assert.equal(model.libraryCategoryMatches(vrAsset, 'all'), true);
assert.equal(model.libraryCategoryMatches(nonVR, 'NonVRChat'), true);
assert.equal(model.libraryCategoryMatches(nonVR, 'Base'), false);
assert.equal(model.libraryCategoryMatches(nonVR, 'all', 'downloaded'), true);
assert.equal(model.libraryCategoryMatches(nonVR, 'all', 'attention'), true);
assert.equal(model.libraryCategoryMatches(nonVR, 'all', 'all', 'Chosen collection'), true);
for (const status of ['ready', 'downloaded', 'extracting', 'extractionFailed'])
  assert.equal(model.isFileDownloaded({ ...products[0].files[0], status }), true);
for (const status of [
  'notDownloaded',
  'missing',
  'failed',
  'unknown',
  'queued',
  'paused',
  'downloading',
])
  assert.equal(model.isFileDownloaded({ ...products[0].files[0], status }), false);
console.log(
  'PASS: All assets hides NonVRChat unless listed there, while explicit scopes retain membership; local file intent requires completed bytes.',
);
// Native list projection has unloaded files but authoritative verified summaries.
const lite = {
  ...products[1],
  files: [],
  filesLoaded: false,
  fullyDownloaded: true,
  status: 'ready',
};
assert.equal(model.getProductDisplayStatus(lite), 'ready');
assert.equal(model.isProductDownloaded(lite), true);
assert.equal(model.getProductDisplayStatus({ ...lite, status: 'processing' }), 'processing');
assert.equal(
  model.getProductDisplayStatus({
    ...lite,
    status: 'attention',
    downloaded: 29,
    total: 30,
    fullyDownloaded: false,
  }),
  'attention',
);
assert.equal(model.isProductDownloaded({ ...lite, fullyDownloaded: false }), false);
assert.equal(
  model.isProductDownloaded({ ...lite, manifestComplete: false }),
  true,
  'Authoritative byte completeness remains independent of source coverage',
);
assert.equal(
  model.getProductDisplayStatus({ ...lite, status: 'invalid' }),
  'partial',
  'Positive native byte evidence stays incomplete when its status is unavailable',
);
assert.equal(
  model.getProductDisplayStatus({ ...lite, filesLoaded: true }),
  'notDownloaded',
  'An empty loaded manifest never invents usable files',
);
console.log(
  'PASS: unloaded native file projections retain authoritative readiness/byte-completeness without inventing file facts.',
);
