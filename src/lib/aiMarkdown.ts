import { marked } from "marked";
import DOMPurify from "dompurify";

/** Remove tool-call fences / bare tool JSON so the Reader only shows prose. */
export function stripToolArtifacts(raw: string): string {
  let s = (raw || "").replace(/\r\n/g, "\n");

  // Fenced blocks: drop ```json … ``` always; drop other fences that look like tool calls.
  // Keep mermaid / svg / html for splitAiAnswer.
  s = s.replace(/```([^\n`]*)\n?([\s\S]*?)```/g, (_full, lang: string, body: string) => {
    const l = (lang || "").trim().toLowerCase();
    const inner = (body || "").trim();
    if (l === "mermaid" || l === "svg" || l === "html" || l === "htm") return _full;
    if (l === "json" || l === "javascript" || l === "js") {
      if (looksLikeToolJson(inner) || l === "json") return "\n";
    }
    if (looksLikeToolJson(inner)) return "\n";
    return _full;
  });

  s = stripBareToolJson(s);

  const lines = s.split("\n");
  let start = 0;
  while (start < lines.length) {
    const t = lines[start].trim();
    if (!t) {
      start++;
      continue;
    }
    if (
      /^\{?\s*"tools"\s*:/.test(t) ||
      /^\{?\s*"name"\s*:\s*"[a-z_]+"/.test(t) ||
      /^\]?\s*,?\s*\{?\s*"name"\s*:/.test(t) ||
      t === "{" ||
      t === "}" ||
      t === "[" ||
      t === "]" ||
      /^"?tools"?\s*:/.test(t)
    ) {
      start++;
      continue;
    }
    break;
  }
  s = lines.slice(start).join("\n");

  return s.replace(/\n{3,}/g, "\n\n").trim();
}

function looksLikeToolJson(inner: string): boolean {
  if (!inner) return false;
  if (/"tools"\s*:\s*\[/.test(inner)) return true;
  if (/^\s*\{\s*"name"\s*:\s*"[a-z_]+"\s*,\s*"args"\s*:/.test(inner)) return true;
  if (/^\s*\[\s*\{\s*"name"\s*:/.test(inner)) return true;
  return false;
}

function stripBareToolJson(text: string): string {
  let out = text;
  for (const marker of ['{"tools"', '{"name"'] as const) {
    let guard = 0;
    while (guard++ < 20) {
      const idx = out.indexOf(marker);
      if (idx < 0) break;
      const end = findBalancedEnd(out, idx);
      if (end < 0) break;
      const slice = out.slice(idx, end + 1);
      if (looksLikeToolJson(slice) || marker === '{"tools"') {
        out = `${out.slice(0, idx)}\n${out.slice(end + 1)}`;
      } else {
        break;
      }
    }
  }
  return out;
}

function findBalancedEnd(s: string, start: number): number {
  let depth = 0;
  let inStr = false;
  let esc = false;
  for (let i = start; i < s.length; i++) {
    const c = s[i];
    if (inStr) {
      if (esc) {
        esc = false;
      } else if (c === "\\") {
        esc = true;
      } else if (c === '"') {
        inStr = false;
      }
      continue;
    }
    if (c === '"') {
      inStr = true;
      continue;
    }
    if (c === "{" || c === "[") depth++;
    else if (c === "}" || c === "]") {
      depth--;
      if (depth === 0) return i;
    }
  }
  return -1;
}

/**
 * Ensure GFM pipe-tables have a blank line before them so marked recognizes them
 * even when the model sticks the table right after a paragraph.
 */
export function normalizeGfmTables(md: string): string {
  const lines = md.replace(/\r\n/g, "\n").split("\n");
  const out: string[] = [];
  const isPipeRow = (line: string) => /^\s*\|(.+\|)+/.test(line);
  const isSepRow = (line: string) =>
    /^\s*\|?\s*:?-{3,}:?\s*(\|\s*:?-{3,}:?\s*)+\|?\s*$/.test(line);

  for (let i = 0; i < lines.length; i++) {
    const line = lines[i];
    const prev = out.length ? out[out.length - 1] : "";
    const next = i + 1 < lines.length ? lines[i + 1] : "";
    if (
      isPipeRow(line) &&
      isSepRow(next) &&
      prev.trim() !== "" &&
      !isPipeRow(prev)
    ) {
      out.push("");
    }
    out.push(line);
  }
  return out.join("\n");
}

function ensureSvgRoot(inner: string): string {
  const t = (inner || "").trim();
  if (!t) return '<svg xmlns="http://www.w3.org/2000/svg"></svg>';
  if (/^<svg[\s>]/i.test(t)) return t;
  return `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 800 600">${t}</svg>`;
}

function escapeMermaidSource(src: string): string {
  return (src || "")
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;");
}

marked.setOptions({
  gfm: true,
  breaks: true,
});

const TABLE_TAGS = [
  "table",
  "thead",
  "tbody",
  "tfoot",
  "tr",
  "th",
  "td",
  "colgroup",
  "col",
  "caption",
];

const SVG_TAGS = [
  "svg",
  "g",
  "path",
  "circle",
  "ellipse",
  "line",
  "polyline",
  "polygon",
  "rect",
  "text",
  "tspan",
  "defs",
  "marker",
  "use",
  "title",
  "desc",
  "foreignObject",
  "clipPath",
  "linearGradient",
  "radialGradient",
  "stop",
  "pattern",
  "symbol",
];

const SVG_ATTRS = [
  "viewBox",
  "xmlns",
  "xmlns:xlink",
  "xlink:href",
  "href",
  "d",
  "cx",
  "cy",
  "r",
  "rx",
  "ry",
  "x",
  "y",
  "x1",
  "y1",
  "x2",
  "y2",
  "points",
  "fill",
  "stroke",
  "stroke-width",
  "stroke-linecap",
  "stroke-linejoin",
  "stroke-dasharray",
  "transform",
  "width",
  "height",
  "font-size",
  "font-family",
  "font-weight",
  "text-anchor",
  "dominant-baseline",
  "opacity",
  "fill-opacity",
  "stroke-opacity",
  "marker-end",
  "marker-start",
  "marker-mid",
  "preserveAspectRatio",
  "role",
  "aria-label",
  "class",
  "id",
  "clip-path",
  "fill-rule",
  "clip-rule",
  "gradientUnits",
  "gradientTransform",
  "offset",
  "stop-color",
  "stop-opacity",
  "fx",
  "fy",
  "spreadMethod",
];

/** Markdown prose only — no HTML/SVG artifact extraction. */
export function renderAiProse(md: string): string {
  const cleaned = normalizeGfmTables((md || "").trim());
  if (!cleaned) return "";
  const html = marked.parse(cleaned, { async: false }) as string;
  const clean = DOMPurify.sanitize(html, {
    USE_PROFILES: { html: true, svg: true, svgFilters: true },
    ADD_TAGS: [
      ...TABLE_TAGS,
      ...SVG_TAGS,
      "div",
      "span",
      "section",
      "article",
      "figure",
      "figcaption",
      "header",
      "footer",
      "main",
      "aside",
      "nav",
      "img",
    ],
    ADD_ATTR: [
      "target",
      "rel",
      "colspan",
      "rowspan",
      "align",
      "scope",
      "style",
      "class",
      "id",
      "src",
      "alt",
      "title",
      "width",
      "height",
      "loading",
      ...SVG_ATTRS,
    ],
    FORBID_TAGS: ["script", "iframe", "object", "embed", "form", "link", "meta", "base"],
    FORBID_ATTR: [
      "onerror",
      "onload",
      "onclick",
      "onmouseover",
      "onfocus",
      "onblur",
      "onmouseenter",
      "onmouseleave",
      "onsubmit",
      "onchange",
      "oninput",
    ],
  });
  return clean.replace(
    /<table(\b[\s\S]*?<\/table>)/gi,
    '<div class="table-wrap"><table$1</div>',
  );
}

export type AiProseBlock = { kind: "prose"; html: string };
export type AiArtifactBlock = { kind: "artifact"; id: string };
export type AiMermaidBlock = { kind: "mermaid"; id: string; source: string };
export type AiBlock = AiProseBlock | AiArtifactBlock | AiMermaidBlock;

export type SplitAiAnswer = {
  blocks: AiBlock[];
  /** id → raw HTML/SVG (never goes through marked). */
  artifacts: Map<string, string>;
};

function nextId(prefix: string, n: { i: number }): string {
  n.i += 1;
  return `${prefix}-${n.i}`;
}

function pushProse(blocks: AiBlock[], buf: string[]) {
  const md = buf.join("").trim();
  buf.length = 0;
  if (!md) return;
  const html = renderAiProse(md);
  if (html) blocks.push({ kind: "prose", html });
}

function looksLikeBareHtmlDocument(t: string): boolean {
  if (/^<!DOCTYPE\s+html\b/i.test(t) || /^<html[\s>]/i.test(t)) return true;
  if (/^<svg[\s>]/i.test(t)) return true;
  if (
    /^<(?:div|section|figure|table|svg)\b/i.test(t) &&
    /<\/(?:div|section|figure|table|svg)>/i.test(t)
  ) {
    const mdSignals =
      (t.match(/^#{1,6}\s/gm) || []).length +
      (t.match(/^\|.+\|/gm) || []).length +
      (t.match(/^[-*+]\s+\S/gm) || []).length +
      (t.match(/```/g) || []).length;
    return mdSignals === 0;
  }
  return false;
}

/**
 * Split CLI output into prose (marked+DOMPurify), HTML/SVG artifacts (raw Map),
 * and mermaid sources. Artifact placeholders are React-owned — no data-ai-b64.
 */
export function splitAiAnswer(raw: string): SplitAiAnswer {
  const cleaned = normalizeGfmTables(stripToolArtifacts(raw));
  const blocks: AiBlock[] = [];
  const artifacts = new Map<string, string>();
  const counter = { i: 0 };

  if (!cleaned) return { blocks, artifacts };

  // Whole-answer bare HTML/SVG → single artifact (never through marked).
  if (looksLikeBareHtmlDocument(cleaned)) {
    const id = nextId("art", counter);
    const html = /^<svg[\s>]/i.test(cleaned.trim())
      ? ensureSvgRoot(cleaned.trim())
      : cleaned;
    artifacts.set(id, html);
    blocks.push({ kind: "artifact", id });
    return { blocks, artifacts };
  }

  const fenceRe = /```([^\n`]*)\n?([\s\S]*?)```/g;
  let last = 0;
  const proseBuf: string[] = [];
  let m: RegExpExecArray | null;
  while ((m = fenceRe.exec(cleaned)) !== null) {
    proseBuf.push(cleaned.slice(last, m.index));
    const lang = (m[1] || "").trim().toLowerCase().split(/\s+/)[0] || "";
    const inner = (m[2] || "").trim();
    if (lang === "html" || lang === "htm") {
      pushProse(blocks, proseBuf);
      const id = nextId("art", counter);
      artifacts.set(id, inner);
      blocks.push({ kind: "artifact", id });
    } else if (lang === "svg") {
      pushProse(blocks, proseBuf);
      const id = nextId("art", counter);
      artifacts.set(id, ensureSvgRoot(inner));
      blocks.push({ kind: "artifact", id });
    } else if (lang === "mermaid") {
      pushProse(blocks, proseBuf);
      const id = nextId("mmd", counter);
      blocks.push({ kind: "mermaid", id, source: inner });
    } else {
      // Keep other fences inside prose markdown.
      proseBuf.push(m[0]);
    }
    last = m.index + m[0].length;
  }
  proseBuf.push(cleaned.slice(last));
  pushProse(blocks, proseBuf);

  return { blocks, artifacts };
}

/**
 * Flatten for PDF/MD export: prose HTML + inlined artifact markup + mermaid divs.
 * Artifacts are NOT sandboxed here (print path).
 */
export function aiOutputToHtml(raw: string): string {
  const { blocks, artifacts } = splitAiAnswer(raw);
  if (!blocks.length) return "";
  return blocks
    .map((b) => {
      if (b.kind === "prose") return b.html;
      if (b.kind === "artifact") {
        const rawHtml = artifacts.get(b.id) || "";
        return `<div class="ai-html-preview" data-ai-artifact="${b.id}">${rawHtml}</div>`;
      }
      return `<div class="mermaid">${escapeMermaidSource(b.source)}</div>`;
    })
    .join("\n");
}

/** @deprecated Prefer splitAiAnswer + AiArtifactFrame. Kept for callers expecting a string. */
export function renderAiMarkdown(raw: string): string {
  return aiOutputToHtml(raw);
}
