// Typed wrappers around the Rust commands in src-tauri/src/commands.rs.
import { invoke } from "@tauri-apps/api/core";

export type ModeId = "technical" | "mentoring" | "billing" | "career" | "general";
export const MODES: { id: ModeId; label: string }[] = [
  { id: "technical", label: "Technical" },
  { id: "mentoring", label: "Mentoring" },
  { id: "billing", label: "Billing" },
  { id: "career", label: "Career" },
  { id: "general", label: "General Support" },
];

export type ModeInstructions = Record<ModeId, string>;

export interface Settings {
  model: string;
  shortcut: string;
  styleInstructions: string;
  customInstructions: string;
  autoPaste: boolean;
  launchAtLogin: boolean;
  showHud: boolean;
  memoryEnabled: boolean;
  memoryAutoDetect: boolean;
  memoryMaxMessages: number;
  memoryMaxChars: number;
  memoryExpireHours: number;
  modeOverride: "auto" | ModeId;
  modeInstructions: ModeInstructions;
  examplesEnabled: boolean;
  maxExamples: number;
  kbEnabled: boolean;
  maxKbEntries: number;
  rewriteBar: boolean;
  rewriteShortcuts: boolean;
  newConversationShortcut: string;
  debugShortcut: string;
  debugModel: string;
}

export interface Status {
  hasApiKey: boolean;
  accessibilityTrusted: boolean;
  defaultStyle: string;
  defaultModeInstructions: ModeInstructions;
  dbError: string | null;
}

export interface ReplyExample {
  id?: number | null;
  studentMessage: string;
  reply: string;
  category: string;
}

export interface KbEntry {
  id?: number | null;
  title: string;
  content: string;
  category: string;
  tags: string;
  enabled: boolean;
}

export interface Message {
  id: number;
  role: "student" | "agent";
  content: string;
  mode: string | null;
  createdAt: number;
}

export interface MemoryStatus {
  conversations: number;
  messages: number;
  current: { id: number; title: string; updatedAt: number; messages: Message[] } | null;
}

export const getSettings = () => invoke<Settings>("get_settings");
export const saveSettings = (settings: Settings) => invoke<void>("save_settings", { settings });
export const getStatus = () => invoke<Status>("get_status");
export const setApiKey = (key: string) => invoke<void>("set_api_key", { key });
export const clearApiKey = () => invoke<void>("clear_api_key");
export const requestAccessibility = () => invoke<boolean>("request_accessibility");
export const openAccessibilitySettings = () => invoke<void>("open_accessibility_settings");

export const listExamples = () => invoke<ReplyExample[]>("list_examples");
export const saveExample = (example: ReplyExample) => invoke<number>("save_example", { example });
export const deleteExample = (id: number) => invoke<void>("delete_example", { id });
export const clearExamples = () => invoke<void>("clear_examples");
export const importExamples = (text: string) => invoke<number>("import_examples", { text });

export const listKb = () => invoke<KbEntry[]>("list_kb");
export const saveKb = (entry: KbEntry) => invoke<number>("save_kb", { entry });
export const setKbEnabled = (id: number, enabled: boolean) => invoke<void>("set_kb_enabled", { id, enabled });
export const deleteKb = (id: number) => invoke<void>("delete_kb", { id });
export const clearKb = () => invoke<void>("clear_kb");
export const importKb = (text: string) => invoke<number>("import_kb", { text });

export const getMemory = () => invoke<MemoryStatus>("get_memory");
export const newConversation = () => invoke<void>("new_conversation");
export const clearCurrentConversation = () => invoke<void>("clear_current_conversation");
export const clearAllConversations = () => invoke<void>("clear_all_conversations");

// ----- GitHub Debug Mode -----

export interface DebugContext {
  issue: string;
  repoUrl: string;
  hasTarget: boolean;
  conversation: string | null;
  historyCount: number;
}

export interface Snippet {
  startLine: number;
  lines: string[];
  highlightStart: number;
  highlightEnd: number;
}

export interface Finding {
  file: string;
  url: string | null;
  lineStart: number | null;
  lineEnd: number | null;
  cause: string;
  fix: string;
  snippet: Snippet | null;
}

export interface Analysis {
  repo: string;
  status: "found" | "uncertain";
  confidence: "high" | "medium" | "low";
  summary: string;
  findings: Finding[];
  missingInfo: string;
  examined: { path: string; url: string; lines: number; truncated: boolean }[];
  notes: string[];
  mode: string;
}

export const debugGetContext = () => invoke<DebugContext>("debug_get_context");
export const debugAnalyze = (request: { repoUrl: string; issue: string; files: string[] }) =>
  invoke<Analysis>("debug_analyze", { request });
export const debugGenerateReply = () => invoke<string>("debug_generate_reply");
export const debugPaste = (reply: string) => invoke<void>("debug_paste", { reply });
export const debugCopy = (reply: string) => invoke<void>("debug_copy", { reply });
export const openGithubUrl = (url: string) => invoke<void>("open_github_url", { url });

// ----- Updates -----

export interface UpdateInfo {
  current: string;
  available: boolean;
  version: string | null;
  notes: string | null;
}

export const checkForUpdate = () => invoke<UpdateInfo>("check_for_update");
export const installUpdate = () => invoke<void>("install_update");
