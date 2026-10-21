import { useCallback, useEffect, useState } from "react";
import { Button } from "@/components/ui/button";
import { api } from "@/lib/api";
import { t, useLang, type UiKey } from "@/lib/i18n";
import type { SubscriptionState, SubscriptionStatus } from "@/lib/types";

const STATE_KEY: Record<SubscriptionState, UiKey> = {
  connected: "subConnected",
  login_required: "subLoginRequired",
  api_key: "subApiKey",
  unknown: "subUnknown",
  missing: "subMissing",
};

/**
 * Abonelikler / Subscriptions: each user connects their own CLI account.
 * Feedcairn detects the CLI, opens its official login command and re-checks
 * the real status. It never sees or stores credentials.
 */
export function SubscriptionsSection({ open }: { open: boolean }) {
  useLang();
  const [items, setItems] = useState<SubscriptionStatus[]>([]);
  const [busy, setBusy] = useState(false);
  const [notice, setNotice] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    setBusy(true);
    try {
      setItems(await api.listSubscriptions());
    } catch (e) {
      setNotice(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  }, []);

  useEffect(() => {
    if (open) void refresh();
  }, [open, refresh]);

  return (
    <section>
      <h3 className="mb-1 font-semibold">{t("subscriptions")}</h3>
      <p className="mb-2 text-xs leading-relaxed text-[var(--muted)]">{t("subscriptionsIntro")}</p>
      <ul className="space-y-2">
        {items.map((s) => (
          <li key={s.id} className="rounded-md border border-[var(--line)] p-2">
            <div className="flex items-center justify-between gap-2">
              <span className="font-medium">{s.name}</span>
              <span
                className={
                  s.status === "connected"
                    ? "text-xs text-[var(--accent)]"
                    : s.status === "missing"
                      ? "text-xs text-[var(--muted)]"
                      : "text-xs text-[var(--danger)]"
                }
              >
                {t(STATE_KEY[s.status] ?? "subUnknown")}
              </span>
            </div>
            {s.status === "missing" ? (
              <div className="mt-1 text-xs text-[var(--muted)]">
                {t("subInstall")}: <code>{s.install}</code>
              </div>
            ) : null}
            <div className="mt-1 flex flex-wrap items-center gap-2 text-xs">
              {s.status !== "missing" && s.status !== "connected" ? (
                <Button
                  type="button"
                  variant="outline"
                  onClick={async () => {
                    try {
                      setNotice(await api.openSubscriptionLogin(s.id));
                    } catch (e) {
                      setNotice(e instanceof Error ? e.message : String(e));
                    }
                  }}
                >
                  {t("subSignIn")} <code className="ml-1">{s.login_command}</code>
                </Button>
              ) : null}
              <a className="underline" href={s.docs} target="_blank" rel="noreferrer noopener">
                {t("subDocs")}
              </a>
            </div>
          </li>
        ))}
      </ul>
      <div className="mt-2 flex flex-wrap items-center gap-2">
        <Button type="button" variant="outline" disabled={busy} onClick={() => void refresh()}>
          {busy ? t("subChecking") : t("subRecheck")}
        </Button>
        {notice ? <span className="text-xs text-[var(--muted)]">{notice}</span> : null}
      </div>
    </section>
  );
}
