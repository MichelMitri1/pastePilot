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
  debugReasoning: "low" | "medium" | "high";
  reviewModel: string;
  debugCompareCommits: boolean;
  debugOneClick: boolean;
  reviewShortcut: string;
  debugIncludeSnippet: boolean;
  debugReview: "always" | "unsure" | "never";
  removeFluff: boolean;
  feedbackLearning: boolean;
  feedbackSaveExamples: boolean;
  clipboardHistory: boolean;
  clipboardWatch: boolean;
  clipboardMaxItems: number;
  clipboardMaxDays: number;
  voiceEnabled: boolean;
  voiceShortcut: string;
  minutesPerReply: number;
  minutesPerDebug: number;
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
  caseMessages: number;
  compareCommitsDefault: boolean;
  includeSnippetDefault: boolean;
  /** A screenshot you copied in the last few minutes, pre-attached. */
  screenshot: string | null;
  /** Set when opened from the rewrite bar's "Diagnosis" button. */
  review: { analysis: Analysis; reply: string; includeSnippet: boolean; repoUrl: string; issue: string; pending: boolean } | null;
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
  before: string | null;
  after: string | null;
  language: string;
}

export interface Check {
  file: string;
  line: number | null;
  kind: string;
  message: string;
}

export interface CommitInfo {
  sha: string;
  short: string;
  message: string;
  date: string;
  url: string;
  files: { path: string; status: string; additions: number; deletions: number }[];
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
  project: { label: string; tags: string[] } | null;
  checks: Check[];
  commits: CommitInfo[];
  similar: { id: number; repo: string; issue: string; summary: string; confidence: string; createdAt: number; sameRepo: boolean }[];
  fixes: { id: number; title: string; used: boolean }[];
  screenshots: number;
  issueType: string;
}

export const debugGetContext = () => invoke<DebugContext>("debug_get_context");
export const debugAnalyze = (request: {
  repoUrl: string;
  issue: string;
  files: string[];
  screenshots: string[];
  compareCommits: boolean;
}) => invoke<Analysis>("debug_analyze", { request });
export const debugGenerateReply = (includeSnippet: boolean) => invoke<string>("debug_generate_reply", { includeSnippet });
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

// ----- Fix library, issue history -----

export interface Fix {
  id?: number | null;
  title: string;
  problem: string;
  solution: string;
  snippet: string;
  tags: string;
  projectType: string;
  uses?: number;
}

export interface IssueRecord {
  id: number;
  repo: string;
  issue: string;
  projectType: string;
  status: string;
  confidence: string;
  summary: string;
  findings: string;
  reply: string;
  createdAt: number;
}

export const listFixes = () => invoke<Fix[]>("list_fixes");
export const saveFix = (fix: Fix) => invoke<number>("save_fix", { fix });
export const deleteFix = (id: number) => invoke<void>("delete_fix", { id });
export const clearFixes = () => invoke<void>("clear_fixes");
export const importFixes = (text: string) => invoke<number>("import_fixes", { text });
export const listIssues = () => invoke<IssueRecord[]>("list_issues");
export const deleteIssue = (id: number) => invoke<void>("delete_issue", { id });
export const clearIssues = () => invoke<void>("clear_issues");

// ----- Clipboard history, cases -----

export interface ClipItem {
  id: number;
  kind: "student" | "reply" | "copied";
  content: string;
  source: string;
  createdAt: number;
}

export const listClipboard = () => invoke<ClipItem[]>("list_clipboard");
export const deleteClipboardItem = (id: number) => invoke<void>("delete_clipboard_item", { id });
export const clearClipboard = () => invoke<void>("clear_clipboard");
export const copyClipboardItem = (id: number) => invoke<void>("copy_clipboard_item", { id });
export const caseAddItems = (ids: number[]) => invoke<number>("case_add_items", { ids });
export const caseStatus = () => invoke<number>("case_status");
export const caseClear = () => invoke<void>("case_clear");

// ----- Analytics, learning -----

export interface Count {
  label: string;
  count: number;
}

export interface Dashboard {
  replies: number;
  debugCases: number;
  rewrites: number;
  sent: number;
  edited: number;
  examplesLearned: number;
  minutesSaved: number;
  modes: Count[];
  issueTypes: Count[];
  projectTypes: Count[];
  rewriteActions: Count[];
  days: { daysAgo: number; replies: number; debug: number }[];
}

export const getDashboard = (rangeDays: number) => invoke<Dashboard>("get_dashboard", { rangeDays });
export const resetAnalytics = () => invoke<void>("reset_analytics");
export const getLearning = () => invoke<{ samples: number; profile: string | null }>("get_learning");
export const resetLearning = () => invoke<void>("reset_learning");
export const takePendingSection = () => invoke<string | null>("take_pending_section");

export const timeAgo = (unixSeconds: number) => {
  const s = Math.max(0, Date.now() / 1000 - unixSeconds);
  if (s < 60) return "just now";
  if (s < 3600) return `${Math.floor(s / 60)}m ago`;
  if (s < 86400) return `${Math.floor(s / 3600)}h ago`;
  return `${Math.floor(s / 86400)}d ago`;
};

// ----- Assignment Review (separate feature) -----

export interface Preset {
  id?: number | null;
  name: string;
  exampleUrl: string;
  requirements: string;
}

export interface ReviewContext {
  studentUrl: string;
  repoUrl: string;
  notes: string;
  hasTarget: boolean;
  conversation: string | null;
  presets: Preset[];
  chrome: boolean;
  screenshot: string | null;
}

export interface SiteSummary {
  url: string;
  title: string;
  ok: boolean;
  rendered: boolean;
  shots: { viewport: string; width: number; dataUrl: string }[];
  consoleErrors: string[];
  brokenLinks: string[];
  error: string | null;
}

export interface ReviewItem {
  area: string;
  status: "complete" | "needs_fix" | "missing" | "broken" | "unable_to_verify";
  severity: "critical" | "needs_fix" | "minor" | "none";
  viewport: string;
  detail: string;
  file: string | null;
  url: string | null;
  lineStart: number | null;
  lineEnd: number | null;
  cause: string;
  fix: string;
  snippet: Snippet | null;
  before: string | null;
  after: string | null;
  language: string;
}

export interface ReviewResult {
  summary: string;
  strengths: string[];
  requirements: { requirement: string; status: "complete" | "needs_fix" | "missing" | "unable_to_verify"; note: string }[];
  items: ReviewItem[];
  example: SiteSummary;
  student: SiteSummary;
  repo: string;
  project: { label: string; tags: string[] } | null;
  checks: Check[];
  examined: string[];
  notes: string[];
}

export const reviewGetContext = () => invoke<ReviewContext>("review_get_context");
export const reviewRun = (request: {
  exampleUrl: string; studentUrl: string; repoUrl: string; requirements: string; notes: string; screenshots: string[]; viewports: string[];
}) => invoke<ReviewResult>("review_run", { request });
export const reviewFeedback = (options: { length: string; includeMinor: boolean }) => invoke<string>("review_feedback", { options });
export const reviewPaste = (reply: string) => invoke<void>("review_paste", { reply });
export const reviewCopy = (reply: string) => invoke<void>("review_copy", { reply });
export const reviewSavePreset = (preset: Preset) => invoke<Preset[]>("review_save_preset", { preset });
export const reviewDeletePreset = (id: number) => invoke<Preset[]>("review_delete_preset", { id });
