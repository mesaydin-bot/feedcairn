import { t } from "@/lib/i18n";
import html2canvas from "html2canvas";
import { jsPDF } from "jspdf";
import TurndownService from "turndown";
import { aiOutputToHtml, stripToolArtifacts } from "@/lib/aiMarkdown";
import { isTauri, saveAiExport } from "@/lib/api";
import type { Article } from "@/lib/types";
import { fullDate } from "@/lib/time";

function stamp(): string {
  const d = new Date();
  const y = d.getFullYear();
  const m = String(d.getMonth() + 1).padStart(2, "0");
  const day = String(d.getDate()).padStart(2, "0");
  return `${y}${m}${day}`;
}

function downloadBlob(blob: Blob, filename: string) {
  const url = URL.createObjectURL(blob);
  const a = document.createElement("a");
  a.href = url;
  a.download = filename;
  a.rel = "noopener";
  document.body.appendChild(a);
  a.click();
  a.remove();
  setTimeout(() => URL.revokeObjectURL(url), 4000);
}

async function persistBytes(filename: string, bytes: Uint8Array): Promise<string | null> {
  if (isTauri()) {
    return saveAiExport(filename, bytes);
  }
  downloadBlob(new Blob([bytes as BlobPart]), filename);
  return null;
}

async function persistText(filename: string, text: string): Promise<string | null> {
  const body = text.endsWith("\n") ? text : text + "\n";
  return persistBytes(filename, new TextEncoder().encode(body));
}

const turndown = new TurndownService({
  headingStyle: "atx",
  codeBlockStyle: "fenced",
  bulletListMarker: "-",
  emDelimiter: "*",
});
turndown.addRule("strikethrough", {
  filter: ["del", "s"] as unknown as TurndownService.Filter,
  replacement: (content) => `~~${content}~~`,
});

/** Prefer content_html; fall back to summary as a paragraph. */
export function articleBodyHtml(article: Article): string {
  const html = (article.content_html || "").trim();
  if (html) return html;
  const summary = (article.summary || "").trim();
  if (summary) {
    // summary may already be plain text or light HTML
    if (/<[a-z][\s\S]*>/i.test(summary)) return summary;
    return `<p>${escapeHtml(summary)}</p>`;
  }
  return "<p>No content provided by this feed.</p>";
}

function escapeHtml(s: string): string {
  return s
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;");
}

function articleHeaderMarkdown(article: Article): string {
  const lines: string[] = [`# ${article.title || "Untitled"}`, ""];
  const meta: string[] = [];
  if (article.feed_title) meta.push(article.feed_title);
  if (article.published) meta.push(fullDate(article.published));
  if (meta.length) lines.push(meta.join(" · "), "");
  if (article.url) {
    lines.push(`[Kaynak](${article.url})`, "");
  }
  return lines.join("\n");
}

function articleHeaderHtml(article: Article): string {
  const parts: string[] = [`<h1>${escapeHtml(article.title || "Untitled")}</h1>`];
  const meta: string[] = [];
  if (article.feed_title) meta.push(escapeHtml(article.feed_title));
  if (article.published) meta.push(escapeHtml(fullDate(article.published)));
  if (meta.length) {
    parts.push(`<p style="color:#6b6458;font-size:0.9em">${meta.join(" · ")}</p>`);
  }
  if (article.url) {
    parts.push(
      `<p><a href="${escapeHtml(article.url)}">${escapeHtml(article.url)}</a></p>`,
    );
  }
  return parts.join("\n");
}

/** Title + meta + url + body as readable markdown (HTML→MD via turndown). */
export function articleToMarkdown(article: Article): string {
  const bodyMd = turndown.turndown(articleBodyHtml(article)).trim();
  return `${articleHeaderMarkdown(article)}${bodyMd}\n`;
}

function ozetMarkdown(raw: string): string {
  const text = stripToolArtifacts(raw || "").trim() || (raw || "").trim();
  return text + (text.endsWith("\n") ? "" : "\n");
}

/** Download cleaned AI markdown as feedcairn-ozet-YYYYMMDD.md */
export async function downloadAiMarkdown(raw: string): Promise<string | null> {
  const filename = `feedcairn-ozet-${stamp()}.md`;
  return persistText(filename, ozetMarkdown(raw));
}

export async function downloadArticleMarkdown(article: Article): Promise<string | null> {
  const filename = `feedcairn-haber-${stamp()}.md`;
  return persistText(filename, articleToMarkdown(article));
}

export async function downloadCombinedMarkdown(
  rawAi: string,
  article: Article,
): Promise<string | null> {
  const ozet = ozetMarkdown(rawAi).trim();
  const haber = articleToMarkdown(article).trim();
  const body = `## ${t("summary")}\n\n${ozet}\n\n---\n\n## ${t("article")}\n\n${haber}\n`;
  const filename = `feedcairn-ozet-haber-${stamp()}.md`;
  return persistText(filename, body);
}

const PRINT_CSS = `
  * { box-sizing: border-box; }
  body {
    margin: 0;
    padding: 24px 28px;
    font-family: "IBM Plex Sans", "Segoe UI", system-ui, sans-serif;
    font-size: 14px;
    line-height: 1.65;
    color: #1c1914;
    background: #ffffff;
  }
  h1, h2, h3, h4 { font-weight: 650; line-height: 1.35; margin: 1.1em 0 0.4em; color: #1c1914; }
  h1 { font-size: 1.55em; margin-top: 0; }
  h2 { font-size: 1.25em; }
  h3 { font-size: 1.12em; }
  p { margin: 0 0 0.85em; }
  ul, ol { margin: 0 0 0.85em; padding-left: 1.35em; }
  li { margin: 0.3em 0; }
  strong { font-weight: 650; }
  a { color: #1a8f5a; }
  hr {
    border: 0;
    border-top: 1px solid #d9d1c3;
    margin: 1.5em 0;
  }
  img { max-width: 100%; height: auto; }
  blockquote {
    border-left: 3px solid #1a8f5a;
    margin: 1rem 0;
    padding: 0.2rem 0 0.2rem 1rem;
    color: #6b6458;
  }
  pre, code { font-family: ui-monospace, SFMono-Regular, Menlo, monospace; font-size: 0.9em; }
  pre {
    overflow-x: auto;
    background: #f4f1ea;
    padding: 0.75rem 1rem;
    border-radius: 0.4rem;
  }
  .table-wrap { width: 100%; overflow-x: auto; margin: 1em 0; }
  table {
    width: 100%;
    border-collapse: collapse;
    font-size: 0.92em;
    line-height: 1.45;
  }
  th, td {
    border: 1px solid #d9d1c3;
    padding: 0.45em 0.7em;
    text-align: left;
    vertical-align: top;
  }
  thead th {
    background: #ebe6db;
    font-weight: 650;
    color: #1c1914;
  }
  tbody tr:nth-child(even) td { background: #faf8f4; }
`;

/**
 * Build print-friendly HTML that mirrors the on-screen markdown preview
 * (tables, lists, headings) on a white background for PDF clarity.
 */
function buildPrintDocument(htmlBody: string): string {
  const wrapped = htmlBody.replace(
    /<table([\s\S]*?)<\/table>/gi,
    '<div class="table-wrap"><table$1</table></div>',
  );
  return `<!DOCTYPE html><html><head><meta charset="utf-8"><style>${PRINT_CSS}</style></head><body class="ai-answer-body">${wrapped}</body></html>`;
}

async function htmlToPdfBytes(sourceHtml: string): Promise<Uint8Array> {
  if (!sourceHtml.trim()) {
    throw new Error(t("pdfNoContent"));
  }

  const docHtml = buildPrintDocument(sourceHtml);
  const iframe = document.createElement("iframe");
  iframe.setAttribute("aria-hidden", "true");
  iframe.style.cssText =
    "position:fixed;left:-10000px;top:0;width:794px;height:1123px;border:0;opacity:0;pointer-events:none;";
  document.body.appendChild(iframe);

  try {
    const idoc = iframe.contentDocument!;
    idoc.open();
    idoc.write(docHtml);
    idoc.close();

    await new Promise((r) => setTimeout(r, 80));

    const body = idoc.body;
    const canvas = await html2canvas(body, {
      scale: 2,
      useCORS: true,
      backgroundColor: "#ffffff",
      logging: false,
      windowWidth: body.scrollWidth,
      windowHeight: body.scrollHeight,
    });

    const imgData = canvas.toDataURL("image/png");
    const pdf = new jsPDF({ orientation: "portrait", unit: "mm", format: "a4" });
    const pageW = pdf.internal.pageSize.getWidth();
    const pageH = pdf.internal.pageSize.getHeight();
    const margin = 10;
    const usableW = pageW - margin * 2;
    const usableH = pageH - margin * 2;

    const imgW = usableW;
    const imgH = (canvas.height * imgW) / canvas.width;

    let heightLeft = imgH;
    let position = margin;

    pdf.addImage(imgData, "PNG", margin, position, imgW, imgH);
    heightLeft -= usableH;

    while (heightLeft > 2) {
      position = margin - (imgH - heightLeft);
      pdf.addPage();
      pdf.addImage(imgData, "PNG", margin, position, imgW, imgH);
      heightLeft -= usableH;
    }

    return new Uint8Array(pdf.output("arraybuffer"));
  } finally {
    iframe.remove();
  }
}

/**
 * Programmatic PDF from the same HTML+CSS used for Reader preview.
 * Uses an offscreen iframe + html2canvas + jsPDF (A4, multi-page).
 * In Tauri, writes to ~/Downloads via save_ai_export after one-time folder grant.
 */
export async function downloadAiPdf(
  raw: string,
  previewEl?: HTMLElement | null,
): Promise<string | null> {
  let sourceHtml: string;
  if (previewEl && previewEl.innerHTML.trim()) {
    sourceHtml = previewEl.innerHTML;
  } else {
    sourceHtml = aiOutputToHtml(raw);
  }
  const bytes = await htmlToPdfBytes(sourceHtml);
  return persistBytes(`feedcairn-ozet-${stamp()}.pdf`, bytes);
}

export async function downloadArticlePdf(
  article: Article,
  articleEl?: HTMLElement | null,
): Promise<string | null> {
  let sourceHtml: string;
  if (articleEl && articleEl.innerHTML.trim()) {
    sourceHtml = `${articleHeaderHtml(article)}${articleEl.innerHTML}`;
  } else {
    sourceHtml = `${articleHeaderHtml(article)}${articleBodyHtml(article)}`;
  }
  const bytes = await htmlToPdfBytes(sourceHtml);
  return persistBytes(`feedcairn-haber-${stamp()}.pdf`, bytes);
}

export async function downloadCombinedPdf(
  rawAi: string,
  article: Article,
  aiPreviewEl?: HTMLElement | null,
  articleEl?: HTMLElement | null,
): Promise<string | null> {
  const ozetHtml =
    aiPreviewEl && aiPreviewEl.innerHTML.trim()
      ? aiPreviewEl.innerHTML
      : aiOutputToHtml(rawAi);
  const haberBody =
    articleEl && articleEl.innerHTML.trim()
      ? articleEl.innerHTML
      : articleBodyHtml(article);
  const sourceHtml = `<h2>${t("summary")}</h2>${ozetHtml}<hr/><h2>${t("article")}</h2>${articleHeaderHtml(article)}${haberBody}`;
  const bytes = await htmlToPdfBytes(sourceHtml);
  return persistBytes(`feedcairn-ozet-haber-${stamp()}.pdf`, bytes);
}
