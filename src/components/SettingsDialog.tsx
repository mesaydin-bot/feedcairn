import { useEffect, useState } from "react";
import { Dialog, DialogContent } from "@/components/ui/dialog";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Switch } from "@/components/ui/switch";
import { api } from "@/lib/api";
import type { AiProbe, PromptDefault, Setting } from "@/lib/types";
import { settingMap } from "@/lib/utils";
import { isTauri } from "@/lib/api";
import { setLang, t, useLang, type Lang, type UiKey } from "@/lib/i18n";
import { SubscriptionsSection } from "@/components/SubscriptionsSection";

const KEYS = [
  "theme",
  "ai_provider",
  "ai_cmd_chatgpt",
  "ai_args_chatgpt",
  "ai_model_chatgpt",
  "ai_effort_chatgpt",
  "ai_cmd_kimi",
  "ai_args_kimi",
  "ai_model_kimi",
  "ai_thinking_kimi",
  "ai_cmd_cursor",
  "ai_args_cursor",
  "ai_model_cursor",
  "ai_effort_cursor",
  "ai_cmd_antigravity",
  "ai_args_antigravity",
  "ai_cmd_claude",
  "ai_args_claude",
  "ai_model_claude",
  "ai_effort_claude",
  "mark_read_on_open",
  "unread_only_default",
  "ui_lang",
  "prompt_summary",
  "prompt_table_summary",
  "prompt_tags",
  "prompt_folder_brief",
] as const;

const PROMPT_LABEL: Record<PromptDefault["kind"], UiKey> = {
  summary: "promptSummary",
  table_summary: "promptTableSummary",
  tags: "promptTags",
  folder_brief: "promptFolderBrief",
};

export function SettingsDialog({
  open,
  onOpenChange,
  settings,
  onSave,
}: {
  open: boolean;
  onOpenChange: (v: boolean) => void;
  settings: Setting[];
  onSave: (settings: Setting[]) => Promise<void>;
}) {
  const initial = settingMap(settings);
  const [form, setForm] = useState<Record<string, string>>(initial);
  const [probe, setProbe] = useState<AiProbe | null>(null);
  const [saving, setSaving] = useState(false);
  const [defaults, setDefaults] = useState<PromptDefault[]>([]);

  useEffect(() => {
    if (open && defaults.length === 0) {
      api.promptDefaults().then(setDefaults).catch(() => setDefaults([]));
    }
  }, [open, defaults.length]);

  useEffect(() => {
    if (open) setForm(settingMap(settings));
  }, [open, settings]);

  const lang = useLang();
  const desktop = isTauri();
  // Executable/argument settings are desktop-only; the local web API refuses them.
  const savable = KEYS.filter((key) => desktop || !/^ai_(cmd|args)_/.test(key));

  function set(key: string, value: string) {
    setForm((f) => ({ ...f, [key]: value }));
  }

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent title={t("settings")} className="max-h-[min(40rem,90vh)] overflow-y-auto">
        <div className="space-y-5 text-sm">
          <section>
            <h3 className="mb-2 font-semibold">{t("language")}</h3>
            <div className="flex gap-2">
              {(["tr", "en"] as Lang[]).map((l) => (
                <Button key={l} type="button" variant={lang === l ? "default" : "outline"} onClick={() => {
                    setLang(l);
                    set("ui_lang", l);
                  }}>
                  {l === "tr" ? "Türkçe" : "English"}
                </Button>
              ))}
            </div>
          </section>
          <section>
            <h3 className="mb-2 font-semibold">{t("appearance")}</h3>
            <div className="flex gap-2">
              {(["dark", "light"] as const).map((th) => (
                <Button
                  key={th}
                  type="button"
                  variant={form.theme === th ? "default" : "outline"}
                  onClick={() => set("theme", th)}
                >
                  {th === "dark" ? t("dark") : t("light")}
                </Button>
              ))}
            </div>
          </section>
          <section className="flex items-center justify-between gap-4">
            <div>
              <div className="font-medium">{t("markReadOnOpen")}</div>
              <div className="text-xs text-[var(--muted)]">{t("markReadOnOpenHint")}</div>
            </div>
            <Switch
              checked={form.mark_read_on_open !== "false"}
              onCheckedChange={(v) => set("mark_read_on_open", v ? "true" : "false")}
            />
          </section>
          <section>
            <h3 className="mb-2 font-semibold">{t("aiProvider")}</h3>
            <select
              className="h-8 w-full rounded-md border border-[var(--line)] bg-[var(--bg-elevated)] px-2"
              value={form.ai_provider || "chatgpt"}
              onChange={(e) => set("ai_provider", e.target.value)}
            >
              <option value="chatgpt">{t("chatgptOption")}</option>
              <option value="kimi">Kimi</option>
              <option value="cursor">Cursor</option>
              <option value="antigravity">Antigravity (agy)</option>
              <option value="claude">{t("claudeOption")}</option>
            </select>
            <p className="mt-2 text-xs leading-relaxed text-[var(--muted)]">
              {t("aiProviderHint")}
            </p>
            <p className="mt-2 text-xs leading-relaxed text-[var(--muted)]">
              {t("aiUsageHint")}
            </p>
          </section>
          <SubscriptionsSection open={open} />
          <section>
            <h3 className="mb-1 font-semibold">{t("aiPrompts")}</h3>
            <p className="mb-2 text-xs leading-relaxed text-[var(--muted)]">{t("aiPromptsHint")}</p>
            {defaults.map((d) => {
              const custom = (form[d.setting_key] || "").trim();
              const fallback = lang === "tr" ? d.tr : d.en;
              return (
                <div key={d.kind} className="mb-3">
                  <div className="mb-1 flex items-center justify-between gap-2">
                    <span className="text-xs font-medium">
                      {t(PROMPT_LABEL[d.kind])}
                      {custom ? <span className="ml-2 text-[var(--accent)]">· {t("promptCustom")}</span> : null}
                    </span>
                    <Button
                      type="button"
                      variant="ghost"
                      disabled={!custom}
                      onClick={() => set(d.setting_key, "")}
                    >
                      {t("promptReset")}
                    </Button>
                  </div>
                  <textarea
                    className="h-28 w-full rounded-md border border-[var(--line)] bg-[var(--bg-elevated)] p-2 font-mono text-[12px]"
                    value={custom || fallback}
                    onChange={(e) => {
                      const v = e.target.value;
                      // Matching a built-in default (either language) means "not customized".
                      set(d.setting_key, v.trim() === d.en.trim() || v.trim() === d.tr.trim() ? "" : v);
                    }}
                    aria-label={t(PROMPT_LABEL[d.kind])}
                  />
                </div>
              );
            })}
          </section>
          {(form.ai_provider || "chatgpt") === "chatgpt" ? (
            <section className="grid grid-cols-2 gap-2">
              <label className="col-span-2 text-xs font-medium uppercase tracking-wide text-[var(--muted)]">
                {t("chatgptModel")}
              </label>
              <select
                className="h-8 rounded-md border border-[var(--line)] bg-[var(--bg-elevated)] px-2"
                value={form.ai_model_chatgpt || "gpt-6-astra"}
                onChange={(e) => set("ai_model_chatgpt", e.target.value)}
              >
                <option value="gpt-6-astra">gpt-6-astra</option>
                <option value="gpt-5.6-sol">gpt-5.6-sol</option>
                <option value="gpt-5.6-terra">gpt-5.6-terra</option>
                <option value="gpt-5.6-luna">gpt-5.6-luna</option>
                <option value="gpt-5.5">gpt-5.5</option>
                <option value="gpt-5.3-codex-spark">gpt-5.3-codex-spark</option>
                <option value="gpt-reserve">gpt-reserve</option>
              </select>
              <select
                className="h-8 rounded-md border border-[var(--line)] bg-[var(--bg-elevated)] px-2"
                value={form.ai_effort_chatgpt || "xhigh"}
                onChange={(e) => set("ai_effort_chatgpt", e.target.value)}
              >
                <option value="xhigh">{`${t("thinking")}: xhigh`}</option>
                <option value="high">{`${t("thinking")}: high`}</option>
                <option value="medium">{`${t("thinking")}: medium`}</option>
                <option value="low">{`${t("thinking")}: low`}</option>
                <option value="none">{`${t("thinking")}: none`}</option>
              </select>
            </section>
          ) : null}
          {(form.ai_provider || "chatgpt") === "kimi" ? (
            <section className="grid grid-cols-2 gap-2">
              <label className="col-span-2 text-xs font-medium uppercase tracking-wide text-[var(--muted)]">
                Kimi model / thinking
              </label>
              <select
                className="h-8 rounded-md border border-[var(--line)] bg-[var(--bg-elevated)] px-2"
                value={form.ai_model_kimi || "kimi-code/kimi-for-coding"}
                onChange={(e) => set("ai_model_kimi", e.target.value)}
              >
                <option value="kimi-code/kimi-for-coding">K2.7 Coding</option>
                <option value="kimi-code/kimi-for-coding-highspeed">K2.7 Coding Highspeed</option>
                <option value="kimi-code/k3">K3</option>
                <option value="kimi-code/k3-256k">K3-256k</option>
              </select>
              <select
                className="h-8 rounded-md border border-[var(--line)] bg-[var(--bg-elevated)] px-2"
                value={form.ai_thinking_kimi || "true"}
                onChange={(e) => set("ai_thinking_kimi", e.target.value)}
              >
                <option value="true">{t("thinkingOn")}</option>
                <option value="false">{t("thinkingOff")}</option>
              </select>
            </section>
          ) : null}
          {(form.ai_provider || "chatgpt") === "claude" ? (
            <section className="grid grid-cols-2 gap-2">
              <label className="col-span-2 text-xs font-medium uppercase tracking-wide text-[var(--muted)]">
                {t("claudeModel")}
              </label>
              <select
                className="h-8 rounded-md border border-[var(--line)] bg-[var(--bg-elevated)] px-2"
                value={form.ai_model_claude || "default"}
                onChange={(e) => set("ai_model_claude", e.target.value)}
              >
                <option value="default">{t("claudeDefault")}</option>
                <option value="sonnet">sonnet</option>
                <option value="opus">opus</option>
                <option value="haiku">haiku</option>
              </select>
              <select
                className="h-8 rounded-md border border-[var(--line)] bg-[var(--bg-elevated)] px-2"
                value={form.ai_effort_claude || "default"}
                onChange={(e) => set("ai_effort_claude", e.target.value)}
              >
                <option value="default">{`effort: ${t("claudeDefault")}`}</option>
                <option value="low">effort: low</option>
                <option value="medium">effort: medium</option>
                <option value="high">effort: high</option>
                <option value="xhigh">effort: xhigh</option>
                <option value="max">effort: max</option>
              </select>
            </section>
          ) : null}
          {(form.ai_provider || "chatgpt") === "cursor" ? (
            <section className="grid grid-cols-2 gap-2">
              <label className="col-span-2 text-xs font-medium uppercase tracking-wide text-[var(--muted)]">
                Cursor model
              </label>
              <Input
                className="col-span-2"
                value={form.ai_model_cursor || "auto"}
                onChange={(e) => set("ai_model_cursor", e.target.value)}
                placeholder={t("cursorModelPlaceholder")}
              />
            </section>
          ) : null}
          {desktop ? (
          <section className="grid grid-cols-2 gap-2">
            <label className="col-span-2 text-xs font-medium uppercase tracking-wide text-[var(--muted)]">
              {t("advancedTemplate")}
            </label>
            {(["chatgpt", "kimi", "cursor", "antigravity", "claude"] as const).map((p) => (
              <div key={p} className="contents">
                <Input
                  value={form[`ai_cmd_${p}`] || (p === "chatgpt" ? "codex" : p)}
                  onChange={(e) => set(`ai_cmd_${p}`, e.target.value)}
                  placeholder={`${p} cmd`}
                />
                <Input
                  value={form[`ai_args_${p}`] || ""}
                  onChange={(e) => set(`ai_args_${p}`, e.target.value)}
                  placeholder={`${p} ${t("argsOptional")}`}
                />
              </div>
            ))}
          </section>
          ) : (
            <p className="text-xs text-[var(--muted)]">{t("advancedDesktopOnly")}</p>
          )}
<div className="flex flex-wrap items-center gap-2">
            <Button
              type="button"
              variant="outline"
              onClick={async () => {
                await onSave(savable.map((key) => ({ key, value: form[key] ?? "" })));
                setProbe(await api.probeAi());
              }}
            >
              {t("checkCli")}
            </Button>
            {probe ? (
              <span className={probe.found ? "text-xs text-[var(--accent)]" : "text-xs text-[var(--danger)]"}>
                {probe.hint}
              </span>
            ) : null}
          </div>
          <div className="flex justify-end gap-2">
            <Button variant="ghost" onClick={() => onOpenChange(false)}>
              {t("close")}
            </Button>
            <Button
              disabled={saving}
              onClick={async () => {
                setSaving(true);
                try {
                  await onSave(savable.map((key) => ({ key, value: form[key] ?? "" })));
                  onOpenChange(false);
                } finally {
                  setSaving(false);
                }
              }}
            >
              {t("save")}
            </Button>
          </div>
        </div>
      </DialogContent>
    </Dialog>
  );
}

export function KeyboardHelp({ open, onOpenChange }: { open: boolean; onOpenChange: (v: boolean) => void }) {
  useLang();
  const rows = [
    ["j / n / ↓", t("kbNext")],
    ["k / p / ↑", t("kbPrev")],
    ["m", t("kbMark")],
    ["s", t("kbStar")],
    ["o / v", t("kbOpen")],
    ["r", t("kbRefresh")],
    ["/", t("kbSearch")],
    ["/… + Enter", t("kbAi")],
    ["Esc (/)", t("kbEsc")],
    ["?", t("kbHelp")],
    ["📖", t("kbNotebook")],
  ];
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent title={t("keyboard")}>
        <table className="w-full text-sm">
          <tbody>
            {rows.map(([k, v]) => (
              <tr key={k} className="border-b border-[var(--line)]">
                <td className="py-1.5 font-mono text-[13px]">{k}</td>
                <td className="py-1.5 text-[var(--muted)]">{v}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </DialogContent>
    </Dialog>
  );
}
