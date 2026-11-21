import { ImagePlus, Send, Square, X } from 'lucide-react';
import type { Locale } from '../../model';
import { aiSettingsText } from '../../ai-settings-i18n';
import { MAX_IMAGES, imageSource } from './chat-parts';
import type { SendMessage } from './useSendMessage';
import type { Composer } from './useComposer';
import type { ImageDrop } from './useImageDrop';
import type { ChatRequest } from './useChatRequest';
import type { ChatWorkspace } from './useChatWorkspace';
import type { Conversation } from './useConversation';
import type { SearchOptions } from './useSearchOptions';

/** The message box with images, Send or Cancel, the API choice, and the notes and errors below. */
export default function ChatComposer({
  sending,
  composer,
  imageDrop,
  request,
  workspace,
  conversation,
  search,
  locale,
  frozen,
}: {
  sending: SendMessage;
  composer: Composer;
  imageDrop: ImageDrop;
  request: ChatRequest;
  workspace: ChatWorkspace;
  conversation: Conversation;
  search: SearchOptions;
  locale: Locale;
  frozen: boolean;
}) {
  const { send, cancel, checkUnconfirmed, discardUnconfirmed } = sending;
  const {
    imageInput,
    imageBusy,
    images,
    imageTrayOpen,
    setImageTrayOpen,
    setImages,
    promptTooLong,
    prompt,
    setPrompt,
    composing,
  } = composer;
  const { addImages } = imageDrop;
  const { locked, busy, unconfirmed, canSend } = request;
  const {
    vision,
    L,
    provider,
    apiLabel,
    entryId,
    loading,
    setEntryId,
    entries,
    keyMissing,
    error,
    status,
  } = workspace;
  const { chatLoading } = conversation;
  const { hasOnline, stores } = search;
  return (
    <form
      className="ai-chat-composer"
      onSubmit={(event) => {
        event.preventDefault();
        void send();
      }}
    >
      <div className="ai-chat-composer-row">
        <div className="ai-chat-image-zone">
          <input
            ref={imageInput}
            type="file"
            accept="image/jpeg,image/png,image/webp"
            multiple
            hidden
            onChange={(event) => {
              const files = Array.from(event.target.files ?? []);
              event.target.value = '';
              void addImages(files);
            }}
          />
          <button
            type="button"
            className="ai-add-images"
            disabled={locked || imageBusy || !vision || images.length >= MAX_IMAGES}
            title={!vision ? L('ai_images_unavailable') : L('addImages')}
            onClick={() => imageInput.current?.click()}
          >
            {images.length ? (
              <img src={imageSource(images[0])} alt={L('image')} />
            ) : (
              <ImagePlus size={24} />
            )}
            <span>{L('addImages')}</span>
          </button>
          {images.length > 0 && (
            <button
              type="button"
              className="ai-image-count"
              aria-label={L('attachments')}
              aria-expanded={imageTrayOpen}
              onClick={() => setImageTrayOpen((value) => !value)}
            >
              {images.length}
            </button>
          )}
          {images.length > 0 && imageTrayOpen && (
            <div className="ai-chat-images">
              {images.map((image, index) => (
                <div className="ai-chat-image" key={index}>
                  <img src={imageSource(image)} alt={`${L('image')} ${index + 1}`} />
                  <button
                    type="button"
                    disabled={busy || imageBusy}
                    aria-label={`${L('removeImage')} ${index + 1}`}
                    onClick={() =>
                      setImages((previous) => previous.filter((_, position) => position !== index))
                    }
                  >
                    <X size={12} />
                  </button>
                </div>
              ))}
            </div>
          )}
        </div>
        <textarea
          aria-label={L('prompt')}
          aria-invalid={promptTooLong}
          placeholder={L('promptPlaceholder')}
          value={prompt}
          disabled={busy || chatLoading || !!unconfirmed}
          maxLength={4000}
          onChange={(event) => setPrompt(event.target.value)}
          onCompositionStart={() => {
            composing.current = true;
          }}
          onCompositionEnd={() => {
            composing.current = false;
          }}
          onKeyDown={(event) => {
            if (
              event.key === 'Enter' &&
              !event.shiftKey &&
              !composing.current &&
              !event.nativeEvent.isComposing &&
              event.nativeEvent.keyCode !== 229
            ) {
              event.preventDefault();
              void send();
            }
          }}
        />
        <div className="ai-chat-composer-actions">
          {busy ? (
            <button type="button" className="ai-chat-send" onClick={() => void cancel()}>
              <Square size={13} />
              {L('cancel')}
            </button>
          ) : (
            <button type="submit" className="primary ai-chat-send" disabled={!canSend}>
              <Send size={15} />
              {L('send')}
            </button>
          )}
          <select
            aria-label={L('api')}
            title={provider ? apiLabel(provider) : L('selectApi')}
            value={entryId}
            disabled={loading || busy}
            onChange={(event) => setEntryId(event.target.value)}
          >
            {!entries.length && <option value="">{L('selectApi')}</option>}
            {entries.map((entry) => (
              <option key={entry.id} value={entry.id}>
                {apiLabel(entry)}
              </option>
            ))}
          </select>
        </div>
      </div>
      {unconfirmed && !busy && (
        <div className="ai-chat-unconfirmed" role="status">
          <span>{L('unconfirmedMessage')}</span>
          <button
            type="button"
            disabled={chatLoading || frozen}
            onClick={() => void checkUnconfirmed()}
          >
            {L('checkHistory')}
          </button>
          <button type="button" disabled={chatLoading || frozen} onClick={discardUnconfirmed}>
            {L('discardPending')}
          </button>
        </div>
      )}
      {!loading && !entries.length && (
        <div className="ai-chat-error" role="alert">
          {aiSettingsText(locale, 'ai_not_configured')}
        </div>
      )}
      {keyMissing && (
        <div className="ai-chat-error" role="alert">
          {L('ai_key_missing')}
        </div>
      )}
      {!loading && !vision && images.length > 0 && (
        <div className="ai-chat-note" role="status">
          {L('ai_images_unavailable')}
        </div>
      )}
      {hasOnline && !stores.length && (
        <div className="ai-chat-note">{L('ai_online_scope_empty')}</div>
      )}
      {promptTooLong && (
        <div className="ai-chat-error" role="alert">
          {L('ai_prompt_too_long')}
        </div>
      )}
      {error && (
        <div className="ai-chat-error" role="alert">
          {L(error)}
        </div>
      )}
      {status && (
        <div className="ai-chat-note" role="status">
          {L(status)}
        </div>
      )}
    </form>
  );
}
