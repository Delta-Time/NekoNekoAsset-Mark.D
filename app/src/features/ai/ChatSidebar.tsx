import { ArrowLeft, MessageCircle, Pencil, Plus, Trash2, X } from 'lucide-react';
import PresetButton from './PresetButton';
import type { Conversation } from './useConversation';
import type { ChatRequest } from './useChatRequest';
import type { ChatActions } from './useChatActions';
import type { ChatWorkspace } from './useChatWorkspace';
import type { HistoryEdits } from './useHistoryEdits';

/** The sidebar: chat history with rename and delete, or the presets and Add preset. */
export default function ChatSidebar({
  conversation,
  request,
  chatActions,
  workspace,
  history,
}: {
  conversation: Conversation;
  request: ChatRequest;
  chatActions: ChatActions;
  workspace: ChatWorkspace;
  history: HistoryEdits;
}) {
  const { historyMode, setHistoryMode, chatId, chatTitle } = conversation;
  const { locked } = request;
  const { newChat, renameChat, openChat, deleteChat } = chatActions;
  const { L, chats, date, loading, presets, setEditor } = workspace;
  const {
    resetConfirmation,
    deleting,
    renaming,
    renameDraft,
    historyBusy,
    setRenameDraft,
    setRenaming,
    deleteButton,
  } = history;
  return (
    <aside className="ai-chat-sidebar" aria-label={L(historyMode ? 'chatHistory' : 'presets')}>
      {historyMode ? (
        <>
          <button
            type="button"
            className="ai-chat-back subtle"
            disabled={locked}
            onClick={() => {
              resetConfirmation();
              setHistoryMode(false);
            }}
          >
            <ArrowLeft size={15} />
            {L('backPresets')}
          </button>
          <button
            type="button"
            className="primary ai-new-chat"
            disabled={locked}
            onClick={() => newChat()}
          >
            <Plus size={16} />
            {L('newChat')}
          </button>
          <div className="ai-chat-history">
            {chats.map((chat) => (
              <div
                className={
                  'ai-history-row ' +
                  (chatId === chat.id ? 'selected ' : '') +
                  (deleting?.id === chat.id ? 'confirming' : '')
                }
                key={chat.id}
                onKeyDown={(event) => {
                  if (event.key === 'Escape') {
                    event.stopPropagation();
                    resetConfirmation();
                  }
                }}
              >
                {renaming === chat.id ? (
                  <form
                    className="ai-history-rename"
                    onSubmit={(event) => {
                      event.preventDefault();
                      void renameChat(chat);
                    }}
                  >
                    <input
                      autoFocus
                      aria-label={L('chatTitle')}
                      value={renameDraft}
                      disabled={historyBusy}
                      maxLength={160}
                      onChange={(event) => setRenameDraft(event.target.value)}
                      onKeyDown={(event) => {
                        if (event.key === 'Escape') {
                          event.stopPropagation();
                          setRenaming(null);
                        }
                      }}
                    />
                    <button type="submit" disabled={historyBusy || !renameDraft.trim()}>
                      {L('save')}
                    </button>
                    <button
                      type="button"
                      className="icon-button"
                      aria-label={L('cancel')}
                      disabled={historyBusy}
                      onClick={() => setRenaming(null)}
                    >
                      <X size={13} />
                    </button>
                  </form>
                ) : (
                  <>
                    <button
                      type="button"
                      className="ai-history-open"
                      title={chatTitle(chat)}
                      disabled={locked || deleting?.id === chat.id}
                      onClick={() => void openChat(chat.id)}
                    >
                      <MessageCircle size={16} />
                      <span>
                        <strong>{chatTitle(chat)}</strong>
                        <small>{date(chat.updatedAt)}</small>
                      </span>
                    </button>
                    <div className="ai-history-actions">
                      <button
                        type="button"
                        className="icon-button"
                        disabled={locked || deleting?.id === chat.id}
                        aria-label={`${L('renameChat')} ${chatTitle(chat)}`}
                        onClick={() => {
                          resetConfirmation();
                          setRenaming(chat.id);
                          setRenameDraft(chatTitle(chat));
                        }}
                      >
                        <Pencil size={14} />
                      </button>
                      <button
                        ref={deleting?.id === chat.id ? deleteButton : undefined}
                        type="button"
                        className={
                          'ai-history-delete ' + (deleting?.id === chat.id ? 'expanded' : '')
                        }
                        disabled={locked || (deleting?.id === chat.id && !deleting.ready)}
                        aria-label={
                          deleting?.id === chat.id
                            ? L('confirmDeleteChat')
                            : `${L('deleteChat')} ${chatTitle(chat)}`
                        }
                        onKeyDown={(event) => {
                          if (event.repeat && (event.key === 'Enter' || event.key === ' '))
                            event.preventDefault();
                        }}
                        onClick={() => void deleteChat(chat)}
                      >
                        <span className="ai-delete-text">{L('confirmDeleteChat')}</span>
                        <Trash2 size={14} />
                      </button>
                    </div>
                  </>
                )}
              </div>
            ))}
            {!chats.length && !loading && (
              <div className="ai-chat-note ai-history-empty">{L('noChats')}</div>
            )}
          </div>
        </>
      ) : (
        <>
          <div className="ai-chat-free">
            {presets[0] && (
              <PresetButton
                preset={presets.find((preset) => preset.id === 'chat') ?? presets[0]}
                conversation={conversation}
                request={request}
                chatActions={chatActions}
                workspace={workspace}
              />
            )}
          </div>
          <div className="ai-chat-presets">
            {presets
              .filter((preset) => preset.id !== 'chat')
              .map((preset: ChatWorkspace['presets'][number]) => (
                <PresetButton
                  key={preset.id}
                  preset={preset}
                  conversation={conversation}
                  request={request}
                  chatActions={chatActions}
                  workspace={workspace}
                />
              ))}
          </div>
          <button
            type="button"
            className="ai-add-preset"
            disabled={locked}
            onClick={() => setEditor({ preset: null })}
          >
            <Plus size={16} />
            {L('addPreset')}
          </button>
        </>
      )}
    </aside>
  );
}
