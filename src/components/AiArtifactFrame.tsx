import { useEffect, useId, useRef, useState } from "react";

const RESIZE_TYPE = "feedcairn-resize";
const MIN_H = 320;

function buildArtifactDocument(rawHtml: string, frameId: string): string {
  const t = (rawHtml || "").trim();
  const baseCss = [
    "html,body{margin:0;padding:8px;background:#fff;color:#111;box-sizing:border-box;}",
    "*,*:before,*:after{box-sizing:border-box;}",
    "img,video,canvas,svg{max-width:100%;height:auto;}",
    "body{min-height:1px;}",
  ].join("");

  const resizeScript = `(function(){
  var FID=${JSON.stringify(frameId)};
  function measure(){
    try{
      var doc=document;
      var h=Math.max(
        doc.documentElement?doc.documentElement.scrollHeight:0,
        doc.body?doc.body.scrollHeight:0,
        doc.documentElement?doc.documentElement.offsetHeight:0,
        doc.body?doc.body.offsetHeight:0
      );
      if(window.parent&&window.parent!==window){
        window.parent.postMessage({type:${JSON.stringify(RESIZE_TYPE)},id:FID,h:h},"*");
      }
    }catch(e){}
  }
  function boot(){
    measure();
    requestAnimationFrame(function(){
      measure();
      requestAnimationFrame(measure);
    });
    if(document.fonts&&document.fonts.ready){
      document.fonts.ready.then(measure).catch(function(){});
    }
    try{
      var ro=new ResizeObserver(function(){measure();});
      if(document.documentElement) ro.observe(document.documentElement);
      if(document.body) ro.observe(document.body);
    }catch(e){}
    window.addEventListener("load",measure);
    setTimeout(measure,50);
    setTimeout(measure,250);
    setTimeout(measure,1000);
  }
  if(document.readyState==="loading"){
    document.addEventListener("DOMContentLoaded",boot);
  }else{
    boot();
  }
})();`;

  const injectIntoFullDoc = (html: string): string => {
    let out = html;
    if (!/<meta\s+charset=/i.test(out)) {
      if (/<head[^>]*>/i.test(out)) {
        out = out.replace(/<head([^>]*)>/i, `<head$1><meta charset="utf-8">`);
      }
    }
    if (/<head[^>]*>/i.test(out)) {
      out = out.replace(
        /<head([^>]*)>/i,
        `<head$1><style>${baseCss}</style>`,
      );
    } else {
      out = `<!DOCTYPE html><html><head><meta charset="utf-8"><style>${baseCss}</style></head><body>${out}</body></html>`;
      return appendScript(out);
    }
    return appendScript(out);
  };

  const appendScript = (html: string): string => {
    if (/<\/body>/i.test(html)) {
      return html.replace(/<\/body>/i, `<script>${resizeScript}</script></body>`);
    }
    return `${html}<script>${resizeScript}</script>`;
  };

  if (/^<!DOCTYPE\s+html\b/i.test(t) || /^<html[\s>]/i.test(t)) {
    return injectIntoFullDoc(t);
  }
  return `<!DOCTYPE html><html><head><meta charset="utf-8"><style>${baseCss}</style></head><body>${t}<script>${resizeScript}</script></body></html>`;
}

/**
 * Dedicated sandboxed iframe for AI HTML/SVG artifacts.
 * Uses blob: URL + postMessage height — never marked/DOMPurify/srcdoc patching.
 */
export function AiArtifactFrame({
  artifactId,
  html,
}: {
  artifactId: string;
  html: string;
}) {
  const reactId = useId();
  const frameId = `${artifactId}:${reactId}`;
  const [blobUrl, setBlobUrl] = useState<string | null>(null);
  const [height, setHeight] = useState(MIN_H);
  const urlRef = useRef<string | null>(null);

  useEffect(() => {
    const doc = buildArtifactDocument(html, frameId);
    const blob = new Blob([doc], { type: "text/html;charset=utf-8" });
    const url = URL.createObjectURL(blob);
    if (urlRef.current) URL.revokeObjectURL(urlRef.current);
    urlRef.current = url;
    setBlobUrl(url);
    setHeight(MIN_H);
    return () => {
      if (urlRef.current) {
        URL.revokeObjectURL(urlRef.current);
        urlRef.current = null;
      }
    };
  }, [html, frameId]);

  useEffect(() => {
    function onMsg(e: MessageEvent) {
      const data = e.data;
      if (!data || typeof data !== "object") return;
      if (data.type !== RESIZE_TYPE) return;
      if (data.id !== frameId) return;
      const h = Number(data.h);
      if (!Number.isFinite(h) || h <= 0) return;
      // Grow with content; cap viewport but keep iframe scrollable (no cream clip).
      const cap = Math.floor(window.innerHeight * 0.85);
      const next = Math.max(MIN_H, Math.min(Math.ceil(h + 8), cap));
      setHeight(next);
    }
    window.addEventListener("message", onMsg);
    return () => window.removeEventListener("message", onMsg);
  }, [frameId]);

  if (!blobUrl) {
    return (
      <div
        className="ai-html-preview"
        data-ai-artifact={artifactId}
        style={{ minHeight: MIN_H }}
        aria-busy="true"
      />
    );
  }

  return (
    <div className="ai-html-preview" data-ai-artifact={artifactId}>
      <iframe
        className="ai-html-iframe"
        title="AI HTML preview"
        sandbox="allow-scripts allow-same-origin"
        src={blobUrl}
        style={{
          width: "100%",
          minHeight: MIN_H,
          height,
          border: 0,
          display: "block",
          background: "#ffffff",
        }}
      />
    </div>
  );
}
