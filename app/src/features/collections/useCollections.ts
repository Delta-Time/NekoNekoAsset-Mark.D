import { useRef, useState } from 'react';
import { errorCode, invokeAction } from '../../backend';
import type { NativeCollection } from '../../backend';
import { coalescedRefresh } from '../../refresh';
import {
  collectionMutationQueue,
  collectionQueryKey,
  collectionSnapshotKey,
  reconciledCollectionName,
} from '../../collections';
import type { Notices } from '../shell/useNotice';
import type { Filters } from '../filters/useFilters';

/** Collections: native writes go through one queue, so the sidebar shows pending changes at
 * once and a refresh waits until the queue is idle. */
export function useCollections({
  notices,
  filters,
  q,
}: {
  notices: Notices;
  filters: Filters;
  q: (key: string) => string;
}) {
  const { notify } = notices;
  const { setCollection, collection } = filters;
  const [collections, setCollections] = useState<Record<string, string[]>>({});
  const reconciledCollections = useRef<NativeCollection[]>([]),
    collectionRows = useRef<NativeCollection[]>([]),
    collectionPending = useRef(false),
    collectionEpoch = useRef(0),
    collectionRefreshNeeded = useRef(false),
    collectionTask = useRef<(() => Promise<void>) | null>(null);
  const [collectionBusy, setCollectionBusy] = useState(false),
    [committedCollections, setCommittedCollections] = useState<NativeCollection[]>([]);
  const collectionError = useRef<(error: unknown) => void>(() => {});
  collectionError.current = (error) => notify(q(errorCode(error)));
  const collectionQueue = useRef<ReturnType<typeof collectionMutationQueue> | null>(null);
  if (!collectionQueue.current)
    collectionQueue.current = collectionMutationQueue(
      (args) => invokeAction<{ collections: NativeCollection[] }>('catalogue', 'collection', args),
      (rows, committed, busy) => {
        collectionEpoch.current++;
        collectionPending.current = busy;
        if (!busy) {
          const before = reconciledCollections.current;
          reconciledCollections.current = committed;
          setCollection((name) => reconciledCollectionName(before, committed, name));
        }
        setCollectionBusy(busy);
        setCommittedCollections((before) =>
          collectionSnapshotKey(before) === collectionSnapshotKey(committed) ? before : committed,
        );
        if (collectionSnapshotKey(rows) !== collectionSnapshotKey(collectionRows.current)) {
          collectionRows.current = rows;
          setCollections(Object.fromEntries(rows.map((row) => [row.name, row.productIds ?? []])));
        }
        if (!busy && collectionRefreshNeeded.current) {
          collectionRefreshNeeded.current = false;
          queueMicrotask(
            () => void refreshCollections().catch((error) => collectionError.current(error)),
          );
        }
      },
      (error) => collectionError.current(error),
      () => crypto.randomUUID(),
    );
  const applyCollections = (rows: NativeCollection[]) =>
    collectionQueue.current!.setCommitted(rows);
  const reconcileCollections = async () => {
    if (collectionPending.current) return;
    const epoch = collectionEpoch.current;
    const result = await invokeAction<{ collections: NativeCollection[] }>(
      'catalogue',
      'collections',
    );
    if (!collectionPending.current && epoch === collectionEpoch.current)
      applyCollections(result.collections);
  };
  const refreshCollections = () => {
    if (collectionPending.current) {
      collectionRefreshNeeded.current = true;
      return Promise.resolve();
    }
    if (!collectionTask.current) collectionTask.current = coalescedRefresh(reconcileCollections);
    return collectionTask.current();
  };
  const changeCollection = (input: Record<string, unknown>) =>
    collectionQueue.current!.enqueue(input);
  const selectedCollectionKey = collectionQueryKey(committedCollections, collection);
  return {
    collections,
    collectionRows,
    collectionPending,
    collectionEpoch,
    collectionRefreshNeeded,
    collectionBusy,
    committedCollections,
    applyCollections,
    refreshCollections,
    changeCollection,
    selectedCollectionKey,
  };
}
export type CollectionStore = ReturnType<typeof useCollections>;
