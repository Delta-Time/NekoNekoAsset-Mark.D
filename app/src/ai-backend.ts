import { isTauri, invoke } from '@tauri-apps/api/core';
import { invokeAction } from './backend';
import { getCurrentWebview } from '@tauri-apps/api/webview';
import type {
  AiJobRequest,
  AiJobResponses,
  AiMode,
  AiRequest,
  AiResponses,
  AiScopes,
} from './bindings';
export type {
  AiConnectionResult,
  AiDraftTest,
  AiEntry,
  AiEntryList,
  AiKeyStatus,
  AiMode,
  AiProviderPreset,
  AiScopes,
} from './bindings';

/** A provider entry being tested or saved. */
export type AiDraft = Omit<Extract<AiRequest, { action: 'test_draft' }>, 'action'>;
async function invokeAi<A extends AiRequest['action']>(
  request: Extract<AiRequest, { action: A }>,
): Promise<AiResponses[A]> {
  if (!isTauri()) throw new Error('nativeUnavailable');
  return invoke('ai_action', { request });
}
export function getAiKeyStatus() {
  return invokeAi({ action: 'key_status' });
}
export function testAiConnection() {
  return invokeAi({ action: 'test_connection' });
}
export function getAiProviders() {
  return invokeAi({ action: 'providers' });
}
export function listAiEntries() {
  return invokeAi({ action: 'list_entries' });
}
export function testAiDraft(draft: AiDraft) {
  return invokeAi({ action: 'test_draft', ...draft });
}
export function saveAiEntry(draft: AiDraft & { testToken: string }) {
  return invokeAi({ action: 'save_entry', ...draft });
}
export function selectAiEntry(entryId: string) {
  return invokeAi({ action: 'select_entry', entryId });
}
export function removeAiEntry(entryId: string) {
  return invokeAi({ action: 'remove_entry', entryId });
}
export function testAiEntry(entryId: string) {
  return invokeAi({ action: 'test_entry', entryId });
}
export interface AiImage {
  mimeType: 'image/jpeg' | 'image/png';
  dataBase64: string;
}
export interface AiChatMessage {
  role: 'user' | 'assistant';
  content: string;
}
export interface AiSuggestionResponse {
  chatId?: string;
  answer: string;
  recommendations: {
    candidateId: string;
    origin: 'library' | 'store';
    productId?: string;
    title: string;
    provider: string;
    url?: string;
    thumbnailUrl?: string;
    thumbnailPath?: string;
    reason: string;
  }[];
  searchStatus: { provider: string; status: 'ok' | 'unavailable' | 'no_results' }[];
}
export interface AiSuggestionInput {
  requestId: string;
  chatId: string;
  entryId?: string;
  appLanguage: import('./model').Locale;
  prompt: string;
  preset: 'outfit' | 'similar' | 'accessories' | 'chat';
  scope: 'library' | 'online' | 'both';
  stores: ('booth' | 'gumroad' | 'jinxxy')[];
  category?: string;
  avatarId?: string;
  contextProductIds?: string[];
  history?: AiChatMessage[];
  images: AiImage[];
}
export async function sendAiSuggestion(input: AiSuggestionInput): Promise<AiSuggestionResponse> {
  if (!isTauri()) throw new Error('nativeUnavailable');
  return invoke('ai_suggestion_action', { request: { action: 'send', ...input } });
}
export async function cancelAiSuggestion(requestId: string): Promise<unknown> {
  if (!isTauri()) throw new Error('nativeUnavailable');
  return invoke('ai_suggestion_action', { request: { action: 'cancel', requestId } });
}
async function invokeAiAnalysis<A extends AiJobRequest['action']>(
  request: Extract<AiJobRequest, { action: A }>,
): Promise<AiJobResponses[A]> {
  if (!isTauri()) throw new Error('nativeUnavailable');
  return invoke('ai_analysis_action', { request });
}
export function startAi(request: {
  mode: AiMode;
  productIds: string[];
  scopes: AiScopes;
}): Promise<{ jobId: string }> {
  return invokeAiAnalysis({ action: 'start', ...request });
}
export function cancelAi(jobId: string): Promise<unknown> {
  return invokeAiAnalysis({ action: 'cancel', jobId });
}
export function retryAi(jobId: string): Promise<{ jobId: string }> {
  return invokeAiAnalysis({ action: 'retry', jobId });
}
export function dismissAiSourceUrlCandidate(request: {
  productId: string;
  url: string;
}): Promise<unknown> {
  return invokeAiAnalysis({ action: 'dismiss_source_url', ...request });
}
export function saveAiPreferences(patch: {
  aiEnabled: boolean;
  aiScopes: AiScopes;
}): Promise<unknown> {
  return invokeAction('catalogue', 'preferences', { patch });
}
export interface AiChatSummary {
  id: string;
  title: string;
  presetId: string;
  createdAt: string;
  updatedAt: string;
}
export interface AiStoredTurn {
  id: string;
  role: 'user' | 'assistant';
  content: string;
  images: AiImage[];
  createdAt: string;
  status?: 'pending' | 'sent' | 'failed' | 'cancelled';
  response?: AiSuggestionResponse;
}
export interface AiChatDetail {
  chat: AiChatSummary;
  messages: AiStoredTurn[];
}
export interface AiCustomPreset {
  id: string;
  title: string;
  description: string;
  systemPrompt: string;
}
export interface AiPresetDraft {
  id?: string;
  title: string;
  description: string;
  systemPrompt: string;
}
export interface AiWorkspace {
  chats: AiChatSummary[];
  presets: AiCustomPreset[];
  personalization: import('./ai-personalization').AiPersonalization;
}
async function invokeAiWorkspace<T>(
  action: string,
  args: Record<string, unknown> = {},
): Promise<T> {
  if (!isTauri()) throw new Error('nativeUnavailable');
  return invoke<T>('ai_suggestion_action', { request: { action, ...args } });
}
export function listAiWorkspace(): Promise<AiWorkspace> {
  return invokeAiWorkspace('list_workspace');
}
export function createAiChat(presetId: string): Promise<AiChatDetail> {
  return invokeAiWorkspace('create_chat', { presetId });
}
export function getAiChat(chatId: string): Promise<AiChatDetail> {
  return invokeAiWorkspace('get_chat', { chatId });
}
export async function renameAiChat(chatId: string, title: string): Promise<AiChatSummary> {
  const result = await invokeAiWorkspace<{ chat: AiChatSummary }>('rename_chat', { chatId, title });
  return result.chat;
}
export function deleteAiChat(chatId: string): Promise<unknown> {
  return invokeAiWorkspace('delete_chat', { chatId });
}
export async function saveAiPersonalization(
  personalization: import('./ai-personalization').AiPersonalization,
): Promise<import('./ai-personalization').AiPersonalization> {
  const result = await invokeAiWorkspace<{
    personalization: import('./ai-personalization').AiPersonalization;
  }>('save_personalization', { personalization });
  return result.personalization;
}
export async function saveAiPreset(preset: AiPresetDraft): Promise<AiCustomPreset> {
  const result = await invokeAiWorkspace<{ preset: AiCustomPreset }>('save_preset', { preset });
  return result.preset;
}
export function deleteAiPreset(presetId: string): Promise<unknown> {
  return invokeAiWorkspace('delete_preset', { presetId });
}
export async function importAiPreset(json: string): Promise<AiCustomPreset> {
  const result = await invokeAiWorkspace<{ preset: AiCustomPreset }>('import_preset', { json });
  return result.preset;
}
export function exportAiPreset(presetId: string): Promise<{ json: string }> {
  return invokeAiWorkspace('export_preset', { presetId });
}
export async function readAiDroppedImages(paths: string[]): Promise<AiImage[]> {
  const result = await invokeAiWorkspace<{ images: AiImage[] }>('read_images', { paths });
  return result.images;
}
export async function subscribeAiImageDrop(
  callback: (paths: string[]) => void,
): Promise<() => void> {
  if (!isTauri()) return () => {};
  return getCurrentWebview().onDragDropEvent((event) => {
    if (event.payload.type === 'drop') callback(event.payload.paths);
  });
}
