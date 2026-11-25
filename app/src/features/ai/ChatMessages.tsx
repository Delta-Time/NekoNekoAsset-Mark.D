import { MessageCircle, User } from 'lucide-react';
import type { AiSuggestionResponse } from '../../ai-backend';
import { AiCatAvatar, imageSource } from './chat-parts';
import ChatResult from './ChatResult';
import type { ChatWorkspace } from './useChatWorkspace';
import type { Conversation } from './useConversation';
import type { ChatRequest } from './useChatRequest';

/** The conversation: the preset's introduction, each turn with its images, results and store
 * notes, and the thinking indicator. */
export default function ChatMessages({
  workspace,
  conversation,
  request,
  onOpenProduct,
}: {
  workspace: ChatWorkspace;
  conversation: Conversation;
  request: ChatRequest;
  onOpenProduct: (productId: string) => void;
}) {
  const { L, personalization, date, storeLabel } = workspace;
  const { messages, turns, chatLoading, selectedPreset } = conversation;
  const { busy, unconfirmed } = request;
  return (
    <div className="ai-chat-messages" ref={messages} role="log" aria-label={L('chat')}>
      {!turns.length && !busy && !chatLoading && (
        <div className="ai-preset-intro">
          <MessageCircle size={28} />
          <h2>{selectedPreset?.title}</h2>
          <p>{selectedPreset?.description}</p>
        </div>
      )}
      {turns.map((turn) => (
        <article
          className={
            'ai-chat-turn ' + (turn.role === 'user' ? 'ai-chat-user' : 'ai-chat-assistant')
          }
          key={turn.id}
        >
          <div className="ai-chat-avatar" aria-hidden="true">
            {turn.role === 'user' ? (
              personalization.avatarDataUrl ? (
                <img src={personalization.avatarDataUrl} alt="" />
              ) : (
                <User size={20} />
              )
            ) : (
              <AiCatAvatar />
            )}
          </div>
          <div className="ai-chat-bubble">
            <header>
              <strong>
                {turn.role === 'user' ? personalization.userName || L('user') : L('assistant')}
              </strong>
              <time dateTime={turn.createdAt}>{date(turn.createdAt)}</time>
            </header>
            {turn.images?.length > 0 && (
              <div className="ai-sent-images">
                {turn.images.map((image, index) => (
                  <img src={imageSource(image)} key={index} alt={`${L('image')} ${index + 1}`} />
                ))}
              </div>
            )}
            {turn.content && <div className="ai-chat-message">{turn.content}</div>}
            {turn.status === 'pending' && (
              <div className="ai-chat-note" role="status">
                {L('sending')}
              </div>
            )}
            {turn.status &&
              turn.status !== 'sent' &&
              turn.status !== 'pending' &&
              turn.id !== unconfirmed?.requestId && (
                <div className="ai-chat-error">
                  {L(turn.status === 'cancelled' ? 'ai_chat_cancelled' : 'ai_chat_failed')}
                </div>
              )}
            {(['library', 'store'] as const).map((origin) => {
              const recommendations =
                turn.response?.recommendations.filter((result) => result.origin === origin) ?? [];
              return (
                recommendations.length > 0 && (
                  <div key={origin}>
                    <div className="ai-chat-result-heading">
                      {L(origin === 'library' ? 'library' : 'online')}
                    </div>
                    <div className="ai-chat-results">
                      {recommendations.map(
                        (result: AiSuggestionResponse['recommendations'][number]) => (
                          <ChatResult
                            key={result.candidateId}
                            result={result}
                            workspace={workspace}
                            onOpenProduct={onOpenProduct}
                          />
                        ),
                      )}
                    </div>
                  </div>
                )
              );
            })}
            {turn.response?.searchStatus
              .filter((item) => item.status !== 'ok')
              .map((item) => (
                <div className="ai-chat-note" key={item.provider}>
                  {storeLabel(item.provider)} ·{' '}
                  {L(item.status === 'unavailable' ? 'storeUnavailable' : 'noResults')}
                </div>
              ))}
          </div>
        </article>
      ))}
      {(busy || chatLoading) && (
        <div className="ai-chat-note" role="status">
          {L('thinking')}
        </div>
      )}
    </div>
  );
}
