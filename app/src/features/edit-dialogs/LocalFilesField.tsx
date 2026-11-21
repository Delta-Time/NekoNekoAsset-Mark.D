import { X } from 'lucide-react';
import { localImportText } from '../../local-import-i18n';
import { localFileName } from './parts';
import type { EditSubject } from './useEditSubject';
import type { LocalFileDraft } from './useLocalFileDraft';

/** A local product's file list: kept and added files, Add files, and dropping files onto it. */
export default function LocalFilesField({
  subject,
  files,
}: {
  subject: EditSubject;
  files: LocalFileDraft;
}) {
  const { L, p, active } = subject;
  const {
    chooseLocalFiles,
    filesDropping,
    localFileList,
    setFilesDropping,
    removedFiles,
    setRemovedFiles,
    returnToFileList,
    addedFiles,
    setAddedFiles,
  } = files;
  return (
    <div className="editor-local-files">
      <div className="unity-file-heading editor-local-file-heading">
        <strong>{L('files')}</strong>
        <button type="button" onClick={() => void chooseLocalFiles()}>
          {L('addFiles')}
        </button>
      </div>
      <div
        className={`unity-file-list editor-local-file-list${filesDropping ? ' is-dropping' : ''}`}
        ref={localFileList}
        tabIndex={0}
        role="group"
        aria-label={L('files')}
        title={localImportText(p.locale, 'dropFiles')}
        onDragOver={(event) => {
          if (event.dataTransfer.types.includes('Files')) {
            event.preventDefault();
            setFilesDropping(true);
          }
        }}
        onDragLeave={(event) => {
          if (
            !(event.relatedTarget instanceof Node) ||
            !event.currentTarget.contains(event.relatedTarget)
          )
            setFilesDropping(false);
        }}
        onDrop={(event) => {
          event.preventDefault();
          setFilesDropping(false);
        }}
      >
        {active?.files
          .filter((file) => !removedFiles.includes(file.id))
          .map((file) => (
            <div className="unity-file-row editor-local-file-row" key={file.id}>
              <span className="unity-file-name">
                <span title={file.name}>{file.name}</span>
              </span>
              <button
                type="button"
                title={L('removeFile')}
                aria-label={`${L('removeFile')} ${file.name}`}
                onClick={() => {
                  setRemovedFiles((ids) => [...new Set([...ids, file.id])]);
                  returnToFileList();
                }}
              >
                <X size={14} />
              </button>
            </div>
          ))}
        {addedFiles.map((path) => (
          <div className="unity-file-row editor-local-file-row" key={path}>
            <span className="unity-file-name">
              <span title={path}>{localFileName(path)}</span>
            </span>
            <button
              type="button"
              title={L('removeFile')}
              aria-label={`${L('removeFile')} ${localFileName(path)}`}
              onClick={() => {
                setAddedFiles((paths) => paths.filter((value) => value !== path));
                returnToFileList();
              }}
            >
              <X size={14} />
            </button>
          </div>
        ))}
        {!addedFiles.length && !active?.files.some((file) => !removedFiles.includes(file.id)) && (
          <div className="editor-local-file-empty">{localImportText(p.locale, 'dropFiles')}</div>
        )}
      </div>
    </div>
  );
}
