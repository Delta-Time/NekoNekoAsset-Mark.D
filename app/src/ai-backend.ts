import { isTauri, invoke } from '@tauri-apps/api/core';
import { invokeAction } from './backend';
import { getCurrentWebview } from '@tauri-apps/api/webview';

export interface AiKeyStatus {
  entryId?: string | null;
  configured?: boolean;
  provider: string;
  endpoint: string;
  model: string;
  keyPresent: boolean;
  visionSupported: boolean;
  keyRequired: boolean;
}
export interface AiProviderPreset {
  id: string;
  label: string;
  endpoint: string;
  model: string;
  wire: string;
  visionSupported: boolean;
  keyRequired: boolean;
}
export interface AiConnectionResult {
  ok: true;
  provider: string;
  model: string;
}
export interface AiEntry {
  id: string;
  provider: string;
  endpoint: string;
  model: string;
  keyPresent: boolean;
  visionSupported: boolean;
  keyRequired: boolean;
}
export interface AiEntryList {
  entries: AiEntry[];
  activeEntryId: string | null;
}
export interface AiDraft {
  provider: string;
  endpoint: string;
  model: string;
  key: string;
}
export interface AiDraftTest {
  ok: true;
  testToken: string;
}
export type AiRequest =
  | { action: 'key_status' | 'test_connection' | 'providers' | 'list_entries' }
  | ({ action: 'test_draft' } & AiDraft)
  | ({ action: 'save_entry'; testToken: string } & AiDraft)
  | { action: 'select_entry' | 'remove_entry' | 'test_entry'; entryId: string };
async function invokeAi<T>(request: AiRequest): Promise<T> {
  if (!isTauri()) throw new Error('nativeUnavailable');
  return invoke<T>('ai_action', { request });
}
export function getAiKeyStatus(): Promise<AiKeyStatus> {
  return invokeAi({ action: 'key_status' });
}
export function testAiConnection(): Promise<AiConnectionResult> {
  return invokeAi({ action: 'test_connection' });
}
export function getAiProviders(): Promise<{ presets: AiProviderPreset[] }> {
  return invokeAi({ action: 'providers' });
}
export function listAiEntries(): Promise<AiEntryList> {
  return invokeAi({ action: 'list_entries' });
}
export function testAiDraft(draft: AiDraft): Promise<AiDraftTest> {
  return invokeAi({ action: 'test_draft', ...draft });
}
export function saveAiEntry(draft: AiDraft & { testToken: string }): Promise<AiEntryList> {
  return invokeAi({ action: 'save_entry', ...draft });
}
export function selectAiEntry(entryId: string): Promise<AiEntryList> {
  return invokeAi({ action: 'select_entry', entryId });
}
export function removeAiEntry(entryId: string): Promise<AiEntryList> {
  return invokeAi({ action: 'remove_entry', entryId });
}
export function testAiEntry(entryId: string): Promise<AiConnectionResult> {
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
export interface AiScopes {
  title: boolean;
  tags: boolean;
  description: boolean;
  files: boolean;
  images: boolean;
}
export type AiMode = 'classify' | 'organize';
type AiAnalysisRequest =
  | { action: 'start'; mode: AiMode; productIds: string[]; scopes: AiScopes }
  | { action: 'cancel' | 'retry'; jobId: string }
  | { action: 'dismiss_source_url'; productId: string; url: string };
async function invokeAiAnalysis<T>(request: AiAnalysisRequest): Promise<T> {
  if (!isTauri()) throw new Error('nativeUnavailable');
  return invoke<T>('ai_analysis_action', { request });
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
