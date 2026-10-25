import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { api } from "../api/tauri";
import { Button, Modal } from "./ui";
import { WHATS_NEW } from "../lib/whatsNew";

const KEY = "seoMaster.lastSeenVersion";

function readSeen(): string | null {
  try { return localStorage.getItem(KEY); } catch { return null; }
}
function writeSeen(v: string) {
  try { localStorage.setItem(KEY, v); } catch { /* storage may be unavailable */ }
}

/** Shows the changes of the current version once, the first time the new version starts. */
export default function WhatsNew() {
  const info = useQuery({ queryKey: ["appInfo"], queryFn: api.getAppInfo, staleTime: Infinity });
  const version = info.data?.version;
  const [dismissed, setDismissed] = useState(false);
  if (!version || dismissed || readSeen() === version) return null;
  const items = WHATS_NEW[version] ?? [];
  const close = () => { writeSeen(version); setDismissed(true); };
  return (
    <Modal title={`تغییرات نسخه‌ی ${version}`} onClose={close}>
      <div className="space-y-3">
        {items.length ? (
          <ul className="list-disc space-y-2 ps-5 text-sm leading-7 text-slate-700">
            {items.map((t) => <li key={t}>{t}</li>)}
          </ul>
        ) : <p className="text-sm text-slate-600">این نسخه بهبودهای جزئی دارد.</p>}
        <div className="flex justify-end"><Button onClick={close}>متوجه شدم</Button></div>
      </div>
    </Modal>
  );
}
