import type { ToolCallResult } from "@/lib/types";

export type SearchRssHit = {
  title: string;
  link: string;
  published: string;
  source: string;
  summary: string;
  feedUrl: string;
};

function field(block: string, key: string): string {
  const re = new RegExp(`^${key}:\\s*(.*)$`, "m");
  const m = block.match(re);
  return (m?.[1] || "").trim();
}

/** Parse structured search_rss_web tool_results into hit cards for the Reader Subscribe cards. */
export function parseSearchRssHits(toolResults: ToolCallResult[] | undefined | null): SearchRssHit[] {
  if (!toolResults?.length) return [];
  const hits: SearchRssHit[] = [];
  let searchFeedUrl = "";

  for (const t of toolResults) {
    if (!t.ok || t.name !== "search_rss_web") continue;
    const detail = t.detail || "";
    const top = detail.match(/^SEARCH_FEED_URL:\s*(\S+)/m);
    if (top?.[1]) searchFeedUrl = top[1].trim();

    const parts = detail.split("---ITEM---").slice(1);
    for (const part of parts) {
      const body = (part.split("---END---")[0] || part).trim();
      if (!body) continue;
      const title = field(body, "TITLE");
      const link = field(body, "LINK");
      const published = field(body, "PUBLISHED");
      const source = field(body, "SOURCE");
      const summary = field(body, "SUMMARY");
      const feedUrl = field(body, "FEED_URL") || searchFeedUrl;
      if (!title || !feedUrl) continue;
      hits.push({ title, link, published, source, summary, feedUrl });
    }
  }

  const seen = new Set<string>();
  return hits.filter((h) => {
    const k = `${h.link}|${h.feedUrl}`;
    if (seen.has(k)) return false;
    seen.add(k);
    return true;
  });
}

export function searchFeedUrlFromTools(
  toolResults: ToolCallResult[] | undefined | null,
): string | null {
  if (!toolResults?.length) return null;
  for (const t of toolResults) {
    if (!t.ok || t.name !== "search_rss_web") continue;
    const m = (t.detail || "").match(/^SEARCH_FEED_URL:\s*(\S+)/m);
    if (m?.[1]) return m[1].trim();
    const hit = parseSearchRssHits([t])[0];
    if (hit?.feedUrl) return hit.feedUrl;
  }
  return null;
}
