//! Built-in AI prompts. English is the default (international use); a Turkish translation
//! is used when the interface language (`ui_lang` setting) is Turkish. Users can replace any
//! built-in prompt with their own text (`prompt_<kind>` setting, stored locally in SQLite);
//! clearing that setting resets it to the default.
//!
//! Placeholders: `{title}`, `{url}`, `{body}` (article prompts) and `{name}`, `{body}` (folder brief).

use serde::Serialize;
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PromptKind {
    Summary,
    TableSummary,
    Tags,
    FolderBrief,
}

impl PromptKind {
    pub const ALL: [PromptKind; 4] = [Self::Summary, Self::TableSummary, Self::Tags, Self::FolderBrief];

    pub fn key(self) -> &'static str {
        match self {
            Self::Summary => "summary",
            Self::TableSummary => "table_summary",
            Self::Tags => "tags",
            Self::FolderBrief => "folder_brief",
        }
    }

    pub fn setting_key(self) -> String {
        format!("prompt_{}", self.key())
    }

    pub fn default_text(self, lang: &str) -> &'static str {
        let tr = lang == "tr";
        match (self, tr) {
            (Self::Summary, false) => SUMMARY_EN,
            (Self::Summary, true) => SUMMARY_TR,
            (Self::TableSummary, false) => TABLE_EN,
            (Self::TableSummary, true) => TABLE_TR,
            (Self::Tags, false) => TAGS_EN,
            (Self::Tags, true) => TAGS_TR,
            (Self::FolderBrief, false) => BRIEF_EN,
            (Self::FolderBrief, true) => BRIEF_TR,
        }
    }
}

const SUMMARY_EN: &str = "Summarize the article below in English.

Format rules:
- 5–8 bullet points; start each with `- `
- Leave a blank line after each bullet
- Short sentences; no walls of text
- Focus on facts and takeaways; do not invent anything

Title: {title}
URL: {url}

{body}";

const SUMMARY_TR: &str = "Aşağıdaki makaleyi Türkçe özetle.

Biçim kuralları:
- 5–8 madde; her madde `- ` ile başlasın
- Her maddeden sonra bir boş satır bırak
- Kısa cümleler; duvar metin yazma
- Olgulara ve çıkarılacak sonuçlara odaklan; uydurma

Başlık: {title}
URL: {url}

{body}";

const TABLE_EN: &str = "Summarize the article below in English, ONLY as a table.

REQUIRED FORMAT (no other format allowed):
- Optional one-line short intro (at most ~20 words), then a blank line.
- Then a REAL GFM pipe table: header row + separator row (|---|---|...) + data rows.
- Do NOT write bullet lists (- / * / 1.). Do NOT write long paragraphs. Do NOT write tool JSON.
- Columns should fit the story, e.g. «Item | Detail» or «Who | What | Where | When | Why | Outcome».
- At least 4 and at most 10 data rows. Short cells; do not invent anything.

Example (keep this shape):
Short intro sentence.

| Item | Detail |
| --- | --- |
| Topic | … |
| Outcome | … |

Title: {title}
URL: {url}

{body}";

const TABLE_TR: &str = "Aşağıdaki makaleyi Türkçe olarak YALNIZCA tablo biçiminde özetle.

ZORUNLU BİÇİM (başka biçim YASAK):
- İsteğe bağlı tek satırlık kısa giriş cümlesi (en fazla ~20 kelime), sonra boş satır.
- Ardından GERÇEK GFM pipe-tablo: başlık satırı + ayırıcı satır (|---|---|...) + veri satırları.
- Madde listesi (- / * / 1.) YAZMA. Uzun paragraf YAZMA. Araç JSON YAZMA.
- Sütunlar habere uygun olsun: örn. «Madde | Detay» veya «Kim | Ne | Nerede | Ne zaman | Neden | Sonuç».
- En az 4, en fazla 10 veri satırı. Kısa hücreler; uydurma.

Örnek (biçimi böyle tut):
Kısa giriş cümlesi.

| Madde | Detay |
| --- | --- |
| Konu | … |
| Sonuç | … |

Başlık: {title}
URL: {url}

{body}";

const TAGS_EN: &str = "Suggest 3–8 short topic tags for this article. Write only a comma-separated list of English tags; add no other explanation.

Title: {title}
URL: {url}

{body}";

const TAGS_TR: &str = "Bu makale için 3–8 kısa konu etiketi öner. Yalnızca virgülle ayrılmış Türkçe etiket listesi yaz; başka açıklama ekleme.

Başlık: {title}
URL: {url}

{body}";

const BRIEF_EN: &str = "These are the unread items in the «{name}» RSS folder. Write a short briefing in English (at most 400 words): what changed and why it matters. Group related stories. Be concrete.

{body}";

const BRIEF_TR: &str = "Bunlar «{name}» RSS klasöründeki okunmamış maddeler. Türkçe kısa bir brifing yaz (en fazla 400 kelime): ne değişti, neden önemli. İlişkili haberleri grupla. Somut ol.

{body}";

/// Interface language from settings; English unless explicitly Turkish.
pub fn ui_lang(settings: &HashMap<String, String>) -> &'static str {
    match settings.get("ui_lang").map(|s| s.trim()) {
        Some("tr") => "tr",
        _ => "en",
    }
}

/// The prompt template in effect: the user's own text if set, else the built-in default.
pub fn template(settings: &HashMap<String, String>, kind: PromptKind) -> String {
    match settings.get(&kind.setting_key()).map(|s| s.trim()) {
        Some(custom) if !custom.is_empty() => custom.to_string(),
        _ => kind.default_text(ui_lang(settings)).to_string(),
    }
}

/// Fill placeholders. If a custom prompt omits `{body}`, the content is appended so the
/// model still sees the article.
pub fn render(settings: &HashMap<String, String>, kind: PromptKind, vars: &[(&str, &str)]) -> String {
    let tpl = template(settings, kind);
    let has_body = tpl.contains("{body}");
    let mut out = tpl;
    for (k, v) in vars {
        out = out.replace(&format!("{{{k}}}"), v);
    }
    if !has_body {
        if let Some((_, body)) = vars.iter().find(|(k, _)| *k == "body") {
            out.push_str("\n\n");
            out.push_str(body);
        }
    }
    out
}

/// Language rule for free-form AI answers: follow the user's language, else the UI language.
pub fn language_rule(settings: &HashMap<String, String>) -> &'static str {
    if ui_lang(settings) == "tr" {
        "Answer in the same language as the user's request. If the language is unclear, answer in Turkish. Tool JSON field names stay English."
    } else {
        "Answer in the same language as the user's request. If the language is unclear, answer in English. Tool JSON field names stay English."
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct PromptDefault {
    pub kind: &'static str,
    pub setting_key: String,
    pub en: &'static str,
    pub tr: &'static str,
}

pub fn defaults() -> Vec<PromptDefault> {
    PromptKind::ALL
        .iter()
        .map(|k| PromptDefault { kind: k.key(), setting_key: k.setting_key(), en: k.default_text("en"), tr: k.default_text("tr") })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn english_by_default_turkish_on_request() {
        let mut s = HashMap::new();
        let p = render(&s, PromptKind::Summary, &[("title", "T"), ("url", "U"), ("body", "B")]);
        assert!(p.starts_with("Summarize the article below in English."));
        assert!(p.contains("Title: T") && p.ends_with("B"));
        s.insert("ui_lang".into(), "tr".into());
        let p = render(&s, PromptKind::Summary, &[("title", "T"), ("url", "U"), ("body", "B")]);
        assert!(p.starts_with("Aşağıdaki makaleyi Türkçe özetle."));
        assert!(language_rule(&s).contains("Turkish"));
    }

    #[test]
    fn custom_prompt_wins_and_reset_restores_default() {
        let mut s = HashMap::new();
        s.insert("prompt_tags".into(), "My tags for {title}".into());
        let p = render(&s, PromptKind::Tags, &[("title", "T"), ("url", "U"), ("body", "B")]);
        assert_eq!(p, "My tags for T\n\nB");
        s.insert("prompt_tags".into(), "  ".into());
        let p = render(&s, PromptKind::Tags, &[("title", "T"), ("url", "U"), ("body", "B")]);
        assert!(p.starts_with("Suggest 3–8 short topic tags"));
    }
}
