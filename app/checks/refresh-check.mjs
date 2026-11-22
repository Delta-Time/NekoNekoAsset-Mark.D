import assert from 'node:assert/strict';
import fs from 'node:fs/promises';
import { stripTypeScriptTypes } from 'node:module';
const moduleUrl = (text) => 'data:text/javascript;base64,' + Buffer.from(text).toString('base64');
const readSource = async (name) =>
  stripTypeScriptTypes(await fs.readFile(new URL('../src/' + name, import.meta.url), 'utf8'));
const backendSource = await readSource('backend.ts');
const progressStart = backendSource.indexOf('export function progressPercent');
const { progressPercent } = await import(
  moduleUrl(backendSource.slice(progressStart, backendSource.indexOf('\n}', progressStart) + 2))
);
for (const stage of ['waitingForUser', 'importing', 'refreshing', 'compiling', 'completed']) {
  assert.equal(
    progressPercent({ id: 'authored-unity', type: 'unity:import', state: 'running', stage }),
    100,
  );
  assert.equal(
    progressPercent({
      id: 'authored-unity',
      type: 'unity:import',
      state: 'running',
      stage,
      progress: 0,
    }),
    100,
  );
}
assert.equal(
  progressPercent({
    id: 'authored-unity',
    type: 'unity:import',
    state: 'running',
    stage: 'preparing',
    preparationPhase: 'verify',
    preparationBytes: 25,
    preparationTotalBytes: 100,
  }),
  25,
);
assert.equal(
  progressPercent({
    id: 'authored-unity',
    type: 'unity:import',
    state: 'running',
    stage: 'preparing',
    preparationWaitingForEditor: true,
  }),
  undefined,
);
assert.equal(
  progressPercent({ id: 'authored-download', type: 'download', state: 'running', progress: 42 }),
  42,
);
const localeFiles = Object.fromEntries(
  await Promise.all(
    ['en', 'ja', 'ko', 'zh-Hans', 'zh-Hant'].map(async (locale) => [
      locale,
      await fs.readFile(new URL(`../src/locales/${locale}.json`, import.meta.url), 'utf8'),
    ]),
  ),
);
const messagesUrl = moduleUrl(
  (await readSource('messages.ts')).replace(
    /import (\w+) from '\.\/locales\/([\w-]+)\.json';/g,
    (_, name, locale) => `const ${name} = ${localeFiles[locale]};`,
  ),
);
const withMessages = (text) => text.replace("'./messages'", JSON.stringify(messagesUrl));
const uxUrl = moduleUrl(withMessages(await readSource('ux-i18n.ts')));
const i18nUrl = moduleUrl(
  withMessages((await readSource('i18n.ts')).replace("'./ux-i18n'", JSON.stringify(uxUrl))),
);
const source = async (name) =>
  import(moduleUrl((await readSource(name)).replace("'./i18n'", JSON.stringify(i18nUrl))));
const { coalescedRefresh, changedPreferences } = await source('refresh.ts');
const originalPreferences = {
  theme: 'dark',
  columns: ['shop', 'files'],
  presets: [{ id: 'synthetic' }],
};
assert.deepEqual(changedPreferences(originalPreferences, structuredClone(originalPreferences)), {});
assert.deepEqual(
  changedPreferences(originalPreferences, { ...originalPreferences, theme: 'light' }),
  { theme: 'light' },
);
assert.deepEqual(
  changedPreferences(originalPreferences, { ...originalPreferences, columns: ['files', 'shop'] }),
  { columns: ['files', 'shop'] },
);
const {
  collectionMembership,
  pendingCollections,
  collectionMutationQueue,
  collectionQueryKey,
  collectionSnapshotKey,
  reconciledCollectionName,
} = await source('collections.ts');
const { getProductDisplayStatus, getEffectiveAvatarTags } = await source('model.ts');
let active = 0,
  maxActive = 0,
  calls = 0;
const gates = [];
const request = coalescedRefresh(async () => {
  calls++;
  maxActive = Math.max(maxActive, ++active);
  try {
    await new Promise((resolve, reject) => gates.push({ resolve, reject }));
  } finally {
    active--;
  }
});
const first = request();
const trailing = Array.from({ length: 100 }, request);
assert.equal(calls, 1);
gates[0].resolve();
await first;
await Promise.resolve();
assert.equal(calls, 2);
gates[1].resolve();
await Promise.all(trailing);
assert.equal(maxActive, 1);
assert.equal(calls, 2);
const failed = request().catch((error) => error);
const next = request();
gates[2].reject('synthetic_failure');
assert.equal(await failed, 'synthetic_failure');
await Promise.resolve();
assert.equal(calls, 4);
gates[3].resolve();
await next;
const original = [{ id: 'a', name: 'A', productIds: ['one'] }];
assert.deepEqual(collectionMembership(['one'], ['one']), { checked: true, mixed: false });
assert.deepEqual(collectionMembership(['one'], ['one', 'two']), { checked: false, mixed: true });
assert.deepEqual(collectionMembership(['outside'], ['one', 'two']), {
  checked: false,
  mixed: false,
});
assert.deepEqual(collectionMembership(['one', 'two', 'outside'], ['one', 'two']), {
  checked: true,
  mixed: false,
});
assert.deepEqual(collectionMembership(['one'], []), { checked: false, mixed: false });
assert.deepEqual(collectionMembership([], ['one']), { checked: false, mixed: false });
assert.deepEqual(collectionMembership(['one', 'one'], ['one', 'one', 'two']), {
  checked: false,
  mixed: true,
});
assert.deepEqual(collectionMembership(['one', 'two'], ['one', 'one', 'two']), {
  checked: true,
  mixed: false,
});
assert.equal(
  reconciledCollectionName(original, [{ id: 'a', name: 'Renamed', productIds: ['one'] }], 'A'),
  'Renamed',
);
assert.equal(reconciledCollectionName(original, [], 'A'), '');
assert.equal(reconciledCollectionName(original, [{ id: 'b', name: 'A', productIds: [] }], 'A'), '');
assert.deepEqual(
  pendingCollections(original, { op: 'add', names: ['A', 'B'], ids: ['one', 'two'] }, () => 'new'),
  [
    { id: 'a', name: 'A', productIds: ['one', 'two'] },
    { id: 'new', name: 'B', productIds: ['one', 'two'] },
  ],
);
assert.deepEqual(original, [{ id: 'a', name: 'A', productIds: ['one'] }]);
assert.equal(
  collectionQueryKey(original, ''),
  collectionQueryKey([...original, { id: 'b', name: 'B', productIds: ['two'] }], ''),
);
assert.equal(
  collectionQueryKey(original, 'A'),
  collectionQueryKey([...original, { id: 'b', name: 'B', productIds: ['two'] }], 'A'),
);
assert.notEqual(
  collectionQueryKey(original, 'A'),
  collectionQueryKey([{ id: 'a', name: 'A', productIds: ['one', 'two'] }], 'A'),
);
assert.notEqual(collectionQueryKey(original, 'A'), collectionQueryKey([], 'A'));
assert.equal(
  collectionSnapshotKey([{ id: 'a', name: 'A', productIds: ['two', 'one', 'one'] }]),
  collectionSnapshotKey([{ id: 'a', name: 'A', productIds: ['one', 'two'] }]),
);

assert.deepEqual(
  pendingCollections(original, { op: 'delete', id: 'a' }, () => 'unused'),
  [],
);
assert.deepEqual(
  pendingCollections(original, { op: 'remove_members', id: 'a', ids: ['one'] }, () => 'unused'),
  [{ id: 'a', name: 'A', productIds: [] }],
);
const product = {
  status: 'notDownloaded',
  fullyDownloaded: false,
  manifestComplete: true,
  files: [{ status: 'downloaded' }, { status: 'notDownloaded' }],
  ownedAvatars: ['Authored Avatar'],
  ownedAvatarEvidence: [
    { avatar: 'Authored Avatar', source: 'sourceTag', entitlementEstablished: false },
  ],
  accountIds: ['A'],
};
assert.equal(getProductDisplayStatus(product), 'notDownloaded');
assert.equal(
  getProductDisplayStatus({ ...product, status: 'unknown' }),
  'notDownloaded',
  'Unknown native status without downloaded evidence uses the ordinary incomplete badge',
);
assert.equal(
  getProductDisplayStatus({ ...product, status: 'unknown', downloaded: 1 }),
  'partial',
  'Unknown native status retains positive downloaded evidence',
);
assert.equal(getProductDisplayStatus({ ...product, status: 'attention' }), 'attention');
assert.deepEqual(getEffectiveAvatarTags(product, ['A']), ['Authored Avatar']);
assert.deepEqual(getEffectiveAvatarTags(product, []), []);
assert.deepEqual(
  getEffectiveAvatarTags(
    {
      ...product,
      ownedAvatarEvidence: [
        { avatar: 'Authored Avatar', source: 'userTag', entitlementEstablished: false },
      ],
    },
    ['A'],
  ),
  ['Authored Avatar'],
);
console.log(
  'PASS: one active / one trailing refresh, burst deduplication, failure recovery, immutable collection pending changes, authoritative native non-ready status, account-scoped tag compatibility.',
);

// Pending native writes never lock presentation, and a rejected middle write
// rebases later operations without skipping or duplicating their dispatch.
const sent = [],
  writeGates = [],
  queueErrors = [];
let presented = [],
  committed = [],
  queued = false,
  idSequence = 0;
const queue = collectionMutationQueue(
  (args) => {
    sent.push(structuredClone(args));
    return new Promise((resolve, reject) => writeGates.push({ resolve, reject }));
  },
  (rows, base, busy) => {
    presented = rows;
    committed = base;
    queued = busy;
  },
  (error) => queueErrors.push(error),
  () => `synthetic-${++idSequence}`,
);
queue.setCommitted([]);
queue.enqueue({ op: 'create', name: 'A' });
const stable = presented[0].id;
const renameCompletion = queue
  .enqueue({ op: 'update', id: stable, name: 'B' })
  .catch((error) => error);
queue.enqueue({ op: 'create', name: 'C' });
queue.enqueue({ op: 'delete', id: stable });
assert.equal(sent.length, 1);
assert.equal(queued, true);
assert.deepEqual(
  presented.map((row) => row.name),
  ['C'],
);
assert.deepEqual(committed, []);
writeGates[0].resolve({ collections: [{ id: stable, name: 'A', productIds: [] }] });
await new Promise(setImmediate);
assert.equal(sent.length, 2);
assert.deepEqual(
  presented.map((row) => row.name),
  ['C'],
);
writeGates[1].reject('synthetic_rename_failure');
await new Promise(setImmediate);
assert.equal(await renameCompletion, 'synthetic_rename_failure');
assert.equal(queueErrors.length, 1);
assert.equal(sent.length, 3);
assert.deepEqual(
  presented.map((row) => row.name),
  ['C'],
);
const cId = sent[2].id;
writeGates[2].resolve({
  collections: [
    { id: stable, name: 'A', productIds: [] },
    { id: cId, name: 'C', productIds: [] },
  ],
});
await new Promise(setImmediate);
assert.equal(sent.length, 4);
assert.equal(sent[3].id, stable);
writeGates[3].resolve({ collections: [{ id: cId, name: 'C', productIds: [] }] });
await new Promise(setImmediate);
assert.equal(queued, false);
assert.deepEqual(committed, presented);
assert.equal(sent.length, 4);
// Native add creates its own identity; a rename queued before commit follows it.
queue.enqueue({ op: 'add', names: ['D'], ids: ['member'] });
const provisional = presented.find((row) => row.name === 'D').id;
queue.enqueue({ op: 'update', id: provisional, name: 'E' });
writeGates[4].resolve({
  collections: [
    { id: cId, name: 'C', productIds: [] },
    { id: 'native-d', name: 'D', productIds: ['member'] },
  ],
});
await new Promise(setImmediate);
assert.equal(sent[5].id, 'native-d');
assert.equal(presented.find((row) => row.name === 'E').id, 'native-d');
writeGates[5].resolve({
  collections: [
    { id: cId, name: 'C', productIds: [] },
    { id: 'native-d', name: 'E', productIds: ['member'] },
  ],
});
await new Promise(setImmediate);
assert.equal(queued, false);
assert.deepEqual(committed, presented);
assert.throws(() => queue.enqueue({ op: 'create', name: 'C' }), /catalogue.already_exists/);
assert.equal(sent.length, 6);
console.log(
  'PASS: sequential collection queue, immediate pending presentation, failed-middle rebase, stable create/add identities, duplicate create guard.',
);

// A mixed checkbox adds the whole captured selection. A subsequent uncheck
// queues only that collection's removal, then restores committed membership
// on failure without disturbing unrelated products or collections.
const checkboxRows = [
    { id: 'checkbox', name: 'Checkbox', productIds: ['one', 'outside'] },
    { id: 'other', name: 'Other', productIds: ['one', 'two'] },
  ],
  checkboxSelection = ['one', 'two'];
queue.setCommitted(checkboxRows);
assert.deepEqual(collectionMembership(presented[0].productIds, checkboxSelection), {
  checked: false,
  mixed: true,
});
const checkboxAdd = queue.enqueue({
  op: 'add',
  collectionIds: ['checkbox'],
  ids: checkboxSelection,
});
assert.deepEqual(collectionMembership(presented[0].productIds, checkboxSelection), {
  checked: true,
  mixed: false,
});
const checkboxRemove = queue
  .enqueue({ op: 'remove_members', id: 'checkbox', ids: checkboxSelection })
  .catch((error) => error);
assert.deepEqual(presented[0].productIds, ['outside']);
assert.deepEqual(presented[1], checkboxRows[1]);
assert.equal(sent.length, 7);
assert.deepEqual(sent[6].collectionIds, ['checkbox']);
writeGates[6].resolve({
  collections: [{ ...checkboxRows[0], productIds: ['one', 'outside', 'two'] }, checkboxRows[1]],
});
await checkboxAdd;
await new Promise(setImmediate);
assert.equal(sent.length, 8);
assert.equal(sent[7].op, 'remove_members');
assert.deepEqual(sent[7].ids, checkboxSelection);
assert.deepEqual(presented[0].productIds, ['outside']);
assert.deepEqual(presented[1], checkboxRows[1]);
writeGates[7].reject('synthetic_checkbox_remove_failure');
await new Promise(setImmediate);
assert.equal(await checkboxRemove, 'synthetic_checkbox_remove_failure');
assert.equal(queued, false);
assert.deepEqual(collectionMembership(presented[0].productIds, checkboxSelection), {
  checked: true,
  mixed: false,
});
assert.deepEqual(presented, committed);
assert.deepEqual(presented[1], checkboxRows[1]);
assert.equal(sent.length, 8);
console.log(
  'PASS: collection checked/mixed/empty membership, captured batch add/remove ordering, scoped writes and failed checkbox rollback.',
);
