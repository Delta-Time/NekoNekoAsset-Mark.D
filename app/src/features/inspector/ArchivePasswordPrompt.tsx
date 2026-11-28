import { useEffect, useState } from 'react';
import { subscribeArchivePasswords, type LockedArchive } from '../../backend';
import SmallDialog from '../../shared/SmallDialog';
import ArchivePassword from './ArchivePassword';

export default function ArchivePasswordPrompt({
  blocked,
  register,
  q,
}: {
  blocked: boolean;
  register: (id: string, password: string, save: boolean) => Promise<unknown>;
  q: (key: string) => string;
}) {
  const [files, setFiles] = useState<LockedArchive[]>([]);
  useEffect(() => {
    let active = true;
    const listener = subscribeArchivePasswords((file) => {
      if (active)
        setFiles((queue) =>
          queue.some((f) => f.fileId === file.fileId) ? queue : [...queue, file],
        );
    });
    return () => {
      active = false;
      void listener.then((stop) => stop());
    };
  }, []);
  const file = files[0];
  if (!file || blocked) return null;
  const close = () => setFiles((queue) => queue.filter((f) => f.fileId !== file.fileId));
  return (
    <SmallDialog
      key={file.fileId}
      label={file.name}
      close={close}
      q={(key) => (key === 'close' ? q('skipArchivePassword') : q(key))}
    >
      <p>{q('archivePasswordRequired')}</p>
      <ArchivePassword
        fileName={file.name}
        manual={file.manual}
        L={q}
        onClose={close}
        onExtract={async (password, save) => {
          await register(file.fileId, password, save);
          close();
        }}
      />
    </SmallDialog>
  );
}
