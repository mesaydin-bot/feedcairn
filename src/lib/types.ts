export type Folder = {
  id: number;
  name: string;
  parent_id: number | null;
  sort: number;
  unread: number;
};

export type Feed = {
  id: number;
  url: string;
  title: string;
  site_url: string | null;
  folder_id: number | null;
  etag: string | null;
  last_modified: string | null;
  last_fetched: number | null;
  last_error: string | null;
  unread: number;
};

export type FolderNode = {
  folder: Folder;
  feeds: Feed[];
  children: FolderNode[];
};

export type Counts = {
  unread: number;
  saved: number;
  all: number;
};

export type Setting = { key: string; value: string };

export type Bootstrap = {
  folders: FolderNode[];
  root_feeds: Feed[];
  counts: Counts;
  settings: Setting[];
};

export type ArticleListItem = {
  id: number;
  feed_id: number;
  feed_title: string;
  title: string;
  url: string | null;
  published: number | null;
  summary: string;
  is_read: boolean;
  is_saved: boolean;
};

export type Article = ArticleListItem & {
  folder_id: number | null;
  guid: string;
  content_html: string;
  fetched_at: number;
};

export type ArticleFilter = {
  feed_id?: number | null;
  folder_id?: number | null;
  saved_only?: boolean;
  unread_only?: boolean;
  query?: string;
  limit?: number;
  offset?: number;
};

export type ImportResult = {
  folders_created: number;
  feeds_added: number;
  feeds_skipped: number;
};

export type RefreshResult = {
  feed_id: number;
  inserted: number;
  updated: number;
  skipped: number;
  not_modified: boolean;
  error: string | null;
};

export type RefreshAllResult = {
  results: RefreshResult[];
  feeds_ok: number;
  feeds_failed: number;
};

export type AiOutput = {
  provider: string;
  command: string;
  output: string;
};

export type AiProbe = {
  provider: string;
  command: string;
  found: boolean;
  hint: string;
};

export type ApiError = {
  error: string;
  code?: string;
  provider?: string;
  command?: string;
  hint?: string;
};

export type Selection =
  | { kind: "all" }
  | { kind: "unread" }
  | { kind: "saved" }
  | { kind: "folder"; id: number; name: string }
  | { kind: "feed"; id: number; name: string };

export type Note = {
  id: number;
  title: string;
  body_md: string;
  updated_at: number;
};

export type AskAiContext = {
  article_id?: number | null;
  feed_id?: number | null;
  folder_id?: number | null;
  query?: string | null;
  selection_kind?: string | null;
  selection_label?: string | null;
};

export type AskAiRequest = {
  prompt: string;
  context?: AskAiContext;
};

export type ToolCallResult = {
  name: string;
  ok: boolean;
  detail: string;
};

export type AskAiResponse = {
  provider: string;
  command: string;
  output: string;
  tool_results: ToolCallResult[];
};

export type PromptTemplate = {
  id: number;
  name: string;
  body: string;
  created_at: number;
  updated_at: number;
};

export interface PromptDefault {
  kind: "summary" | "table_summary" | "tags" | "folder_brief";
  setting_key: string;
  en: string;
  tr: string;
}

export type SubscriptionState = "connected" | "login_required" | "api_key" | "unknown" | "missing";

export interface SubscriptionStatus {
  id: string;
  name: string;
  installed: boolean;
  command: string | null;
  status: SubscriptionState;
  install: string;
  login_command: string;
  docs: string;
}
