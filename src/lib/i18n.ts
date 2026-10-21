import { useSyncExternalStore } from "react";

/**
 * Bilingual UI (TR/EN). Same model as Tabulect's src/locale/ui.ts:
 * one key union and a Record<Lang, Record<Key, string>> dictionary.
 * The choice is persisted in localStorage and defaults to the OS language.
 */
export type Lang = "tr" | "en";
export const LANG_KEY = "feedcairn:uiLang";

const STRINGS = {
  // Settings
  settings: ["Ayarlar", "Settings"],
  language: ["Arayüz dili", "Interface language"],
  appearance: ["Görünüm", "Appearance"],
  dark: ["Koyu", "Dark"],
  light: ["Açık", "Light"],
  markReadOnOpen: ["Açınca okundu say", "Mark read when opening"],
  markReadOnOpenHint: ["Klasik RSS okuyucu davranışı", "Classic RSS reader behaviour"],
  aiProvider: ["AI sağlayıcı", "AI provider"],
  chatgptOption: ["ChatGPT (Codex aboneliği)", "ChatGPT (Codex subscription)"],
  aiProviderHint: [
    "Feedcairn, bilgisayarınızdaki resmî CLI'yi kendi abonelik girişinizle çalıştırır (120 sn zaman aşımı). Anahtar veya token bu uygulamada saklanmaz. ChatGPT için codex, Kimi için kimi, Gemini için agy, Cursor için cursor-agent, Claude için claude (Claude Code) kullanılır.",
    "Feedcairn runs the official CLI on your computer with your own subscription login (120 s timeout). No key or token is stored in this app. ChatGPT uses codex, Kimi uses kimi, Gemini uses agy, Cursor uses cursor-agent, Claude uses claude (Claude Code).",
  ],
  aiUsageHint: [
    "AI: arama çubuğuna /… yazıp Enter — örn. /bu makaleyi özetle, /etiket öner, /bu klasörde ne değişti. Yanıt okuma panelinde görünür. Normal arama için / olmadan yazın; Esc AI modunu temizler.",
    "AI: type /… in the search bar and press Enter — e.g. /summarize this article, /suggest tags, /what changed in this folder. The answer appears in the reader panel. Type without / for a normal search; Esc clears AI mode.",
  ],
  chatgptModel: ["ChatGPT model / düşünce", "ChatGPT model / reasoning"],
  claudeOption: ["Claude (Claude Code aboneliği)", "Claude (Claude Code subscription)"],
  claudeModel: ["Claude model / efor", "Claude model / effort"],
  claudeDefault: ["CLI varsayılanı", "CLI default"],
  thinking: ["düşünce", "reasoning"],
  thinkingOn: ["thinking: açık", "thinking: on"],
  thinkingOff: ["thinking: kapalı", "thinking: off"],
  cursorModelPlaceholder: ["auto (hesabın varsayılanı) veya Cursor model kimliği", "auto (account default) or a Cursor model id"],
  effortNone: ["effort: yok (modele dokunma)", "effort: none (leave model default)"],
  advancedTemplate: ["İleri: komut şablonu (boş bırakılabilir)", "Advanced: command template (optional)"],
  advancedDesktopOnly: [
    "Komut şablonu yalnızca masaüstü uygulamasında değiştirilebilir.",
    "The command template can only be changed in the desktop app.",
  ],
  argsOptional: ["args (opsiyonel)", "args (optional)"],
  checkCli: ["CLI'yi denetle", "Check CLI"],
  close: ["Kapat", "Close"],
  deleteAction: ["Sil", "Delete"],
  feedSubscribedRefreshed: ["Feed abone olundu — liste yenilendi", "Feed subscribed — list refreshed"],
  errorLabel: ["Hata", "Error"],
  save: ["Kaydet", "Save"],
  // Subscriptions
  subscriptions: ["Abonelikler", "Subscriptions"],
  subscriptionsIntro: [
    "Her kullanıcı kendi aboneliğini bağlar. Feedcairn yalnızca resmî CLI'nin giriş komutunu açar ve durumu denetler; parola, token veya API anahtarı görmez ve saklamaz.",
    "Every user connects their own subscription. Feedcairn only opens the official CLI's login command and checks the status; it never sees or stores passwords, tokens or API keys.",
  ],
  subConnected: ["Bağlı", "Connected"],
  subLoginRequired: ["Giriş gerekli", "Sign-in required"],
  subApiKey: ["API anahtarı, token veya bulut hesabıyla giriş yapılmış — abonelik girişi gerekli", "Signed in with an API key, token or cloud account — subscription login required"],
  subUnknown: ["Durum doğrulanamadı", "Status could not be confirmed"],
  subMissing: ["Kurulu değil", "Not installed"],
  subSignIn: ["Giriş yap", "Sign in"],
  subRecheck: ["Yeniden denetle", "Re-check"],
  subChecking: ["Denetleniyor…", "Checking…"],
  subInstall: ["Kurulum", "Install"],
  subDocs: ["Resmî belgeler", "Official docs"],
  aiPrompts: ["AI istemleri", "AI prompts"],
  aiPromptsHint: [
    "Yerleşik istemler varsayılan olarak İngilizcedir; arayüz Türkçe ise Türkçe çevirisi kullanılır. Kendi isteminizi yazabilirsiniz (bu bilgisayarda saklanır). Yer tutucular: {title}, {url}, {body}, {name}.",
    "Built-in prompts are English by default; the Turkish translation is used when the interface is Turkish. You can write your own prompt (stored on this computer). Placeholders: {title}, {url}, {body}, {name}.",
  ],
  promptSummary: ["Özet istemi", "Summary prompt"],
  promptTableSummary: ["Tablo özet istemi", "Table summary prompt"],
  promptTags: ["Etiket istemi", "Tags prompt"],
  promptFolderBrief: ["Klasör brifingi istemi", "Folder briefing prompt"],
  promptCustom: ["özel", "custom"],
  promptReset: ["Varsayılana dön", "Reset to default"],
  // Keyboard help
  keyboard: ["Klavye", "Keyboard"],
  kbNext: ["Sonraki makale", "Next article"],
  kbPrev: ["Önceki makale", "Previous article"],
  kbMark: ["Okundu işaretini değiştir", "Toggle mark read"],
  kbStar: ["Yıldızla / sonra oku", "Star / read later"],
  kbOpen: ["Orijinali tarayıcıda aç", "Open original in browser"],
  kbRefresh: ["Geçerli görünümü yenile", "Refresh current view"],
  kbSearch: ["Aramaya odaklan (boşsa AI modu için / ekler)", "Focus search (empty → seeds / for AI mode)"],
  kbAi: ["AI talimatı → yanıt okuma panelinde", "AI instruction → answer in the reader"],
  kbEsc: ["AI komut modunu temizle", "Clear AI command mode"],
  kbHelp: ["Bu kısayol listesi", "This cheat sheet"],
  kbNotebook: ["Defter panelini aç/kapat", "Toggle Notebook panel"],
  // App
  fullTextNoLonger: [
    "Sayfadan daha uzun metin çıkmadı; mevcut içerik korundu. Gerekirse kaynağı yenileyin.",
    "The page gave no longer text; the existing content was kept. Refresh the source if needed.",
  ],
  fullTextRestored: ["Tam metin yüklendi / içerik geri yüklendi", "Full text loaded / content restored"],
  fullTextLoaded: ["Tam metin yüklendi", "Full text loaded"],
  cmdSummary: ["özet", "summary"],
  cmdTableSummary: ["tablo özet", "table summary"],
  emptyAiInstruction: ["Boş AI talimatı. Örnek: /bu makaleyi özetle", "Empty AI instruction. Example: /summarize this article"],
  searchPlaceholderAi: ["AI talimatı…", "AI instruction…"],
  searchPlaceholder: ["Ara · veya /AI talimatı", "Search · or /AI instruction"],
  searchTitleAi: ["AI komut modu — Enter ile gönder, Esc ile çık", "AI command mode — Enter to send, Esc to exit"],
  searchTitle: ["Makale ara; / ile AI talimatı (Enter)", "Search articles; / for an AI instruction (Enter)"],
  feedUnfiled: ["Feed klasörsüz yapıldı", "Feed moved out of its folder"],
  feedMoved: ["Feed klasöre taşındı", "Feed moved to folder"],
  newFeedName: ["Yeni feed adı:", "New feed name:"],
  deleteFeedConfirm: ["“{name}” aboneliğini sil?", "Delete the “{name}” subscription?"],
  newFolderName: ["Yeni klasör adı:", "New folder name:"],
  deleteFolderConfirm: ["“{name}” klasörünü sil? Feed’ler klasörsüz kalır.", "Delete the “{name}” folder? Its feeds become unfiled."],
  rename: ["Yeniden adlandır", "Rename"],
  renameFolder: ["Klasörü yeniden adlandır", "Rename folder"],
  moveToFolder: ["Klasöre taşı…", "Move to folder…"],
  unfiled: ["Klasörsüz", "Unfiled"],
  dropUnfiled: ["Buraya bırak → Klasörsüz", "Drop here → Unfiled"],
  move: ["Taşı", "Move"],
  folderSummary: ["Klasör özeti:", "Folder summary:"],
  folderSummaryCmd: ["/bu klasörde ne değişti", "/what changed in this folder"],
  // Reader
  tableSummary: ["Tablo özet", "Table summary"],
  summary: ["Özet", "Summary"],
  article: ["Haber", "Article"],
  aiAnswer: ["AI yanıtı", "AI answer"],
  rssResults: [
    "**İnternet RSS araması** — {n} sonuç (özet ≤2 cümle). Aşağıdan **Abone ol** ile Feedcairn’e ekleyebilirsiniz.",
    "**Web RSS search** — {n} results (summary ≤2 sentences). Use **Subscribe** below to add them to Feedcairn.",
  ],
  rssNoResults: ["İnternet RSS aramasında bu pencerede sonuç bulunamadı.", "The web RSS search found nothing in this window."],
  aiRunning: ["AI çalışıyor… yanıt burada görünecek.", "AI is working… the answer will appear here."],
  aiEmpty: ["Yanıt boş kaldı. Araç çıktısı da yok — lütfen komutu tekrar deneyin.", "The answer was empty and there was no tool output — please try the command again."],
  subscribe: ["Abone ol", "Subscribe"],
  subscribed: ["Abone olundu", "Subscribed"],
  subscribing: ["Abone olunuyor…", "Subscribing…"],
  subscribedTo: ["Abone olundu: {name}", "Subscribed: {name}"],
  googleNewsFeed: ["Bu arama için Google News RSS feed'i", "Google News RSS feed for this search"],
  hideDetails: ["Ayrıntıyı gizle", "Hide details"],
  details: ["Ayrıntı", "Details"],
  downloadFailed: ["İndirme başarısız", "Download failed"],
  fromSearchBar: ["Arama çubuğundan /talimat", "/instruction from the search bar"],
  noArticleSelected: [
    "Makale seçili değil. Özet veya etiket için bir makale açıp /özetle veya /etiket öner yazın.",
    "No article selected. Open an article and type /summarize or /suggest tags.",
  ],
  aiHowTo: [
    "AI için arama çubuğuna / ile talimat yazıp Enter’a basın — yanıt burada görünür.",
    "For AI, type an instruction starting with / in the search bar and press Enter — the answer appears here.",
  ],
  loading: ["Yükleniyor…", "Loading…"],
  loadFullText: ["Metnin tamamını yükle", "Load full text"],
  summarizeTitle: ["Makaleyi seçili dilde özetle", "Summarize the article in the selected language"],
  summarizing: ["Özetleniyor…", "Summarizing…"],
  summarize: ["Özetle", "Summarize"],
  tableSummaryTitle: ["Makaleyi GFM markdown tablo olarak özetle", "Summarize the article as a GFM markdown table"],
  downloadTitle: ["Özet veya haberi indir", "Download the summary or the article"],
  download: ["İndir ▾", "Download ▾"],
  summaryMd: ["Özet → MD", "Summary → MD"],
  summaryPdf: ["Özet → PDF", "Summary → PDF"],
  summaryArticleMd: ["Özet + Haber → MD", "Summary + Article → MD"],
  summaryArticlePdf: ["Özet + Haber → PDF", "Summary + Article → PDF"],
  typographyTitle: ["Okuma yazı tipi ve boyutu", "Reading font and size"],
  type: ["Yazı", "Type"],
  smaller: ["Küçült", "Smaller"],
  smallerAria: ["Yazıyı küçült", "Decrease text size"],
  textSize: ["Yazı boyutu", "Text size"],
  larger: ["Büyült", "Larger"],
  largerAria: ["Yazıyı büyült", "Increase text size"],
  fontFamily: ["Yazı tipi", "Font"],
  fullTextLoading: ["Tam metin yükleniyor… sayfa indirilip ana içerik çıkarılıyor.", "Loading full text… downloading the page and extracting the main content."],
  pdfNoContent: ["PDF için içerik yok", "No content for PDF"],
  // Prompt templates
  templateName: ["Şablon adı:", "Template name:"],
  template: ["Şablon", "Template"],
  deleteTemplateConfirm: ["“{name}” şablonunu sil?", "Delete the “{name}” template?"],
  saveTemplateTitle: ["Mevcut AI komutunu şablon olarak kaydet", "Save the current AI command as a template"],
  saveTemplate: ["Şablon kaydet", "Save template"],
  templates: ["Şablonlar", "Templates"],
  noTemplates: ["Kayıtlı şablon yok. AI modunda (/…) bir komut yazıp Kaydet ile ekleyin.", "No saved templates. Type a command in AI mode (/…) and press Save."],
  emptyAnswer: ["(boş yanıt)", "(empty answer)"],
} as const satisfies Record<string, readonly [string, string]>;

export type UiKey = keyof typeof STRINGS;

export const UI: Record<Lang, Record<UiKey, string>> = {
  tr: Object.fromEntries(Object.entries(STRINGS).map(([k, v]) => [k, v[0]])) as Record<UiKey, string>,
  en: Object.fromEntries(Object.entries(STRINGS).map(([k, v]) => [k, v[1]])) as Record<UiKey, string>,
};

function detect(): Lang {
  try {
    const stored = localStorage.getItem(LANG_KEY);
    if (stored === "tr" || stored === "en") return stored;
  } catch {
    /* storage unavailable */
  }
  const nav = typeof navigator !== "undefined" ? navigator.language || "" : "";
  return nav.toLowerCase().startsWith("tr") ? "tr" : "en";
}

let current: Lang = detect();
const listeners = new Set<() => void>();
if (typeof document !== "undefined") document.documentElement.lang = current;

export function getLang(): Lang {
  return current;
}

export function setLang(lang: Lang): void {
  current = lang;
  try {
    localStorage.setItem(LANG_KEY, lang);
  } catch {
    /* ignore */
  }
  document.documentElement.lang = lang;
  listeners.forEach((fn) => fn());
}

export function useLang(): Lang {
  return useSyncExternalStore(
    (fn) => {
      listeners.add(fn);
      return () => listeners.delete(fn);
    },
    () => current,
    () => current,
  );
}

export function t(key: UiKey, vars?: Record<string, string | number>): string {
  let s = UI[current][key];
  if (vars) for (const [k, v] of Object.entries(vars)) s = s.split(`{${k}}`).join(String(v));
  return s;
}
