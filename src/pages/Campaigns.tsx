import { useState } from "react";
import { Link, useNavigate } from "react-router-dom";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { api, CampaignView } from "../api/tauri";
import { Badge, Button, Card, ConfirmDialog, ErrorBox, Field, Modal, inputClass } from "../components/ui";
import { errorMessage, faDate, num } from "../lib/format";

const STATUS: Record<string, [string, "gray" | "amber" | "green" | "red"]> = {
  DRAFT: ["پیش‌نویس", "gray"], IN_PROGRESS: ["در حال انجام", "amber"], PAUSED: ["متوقف", "amber"],
  COMPLETED: ["تکمیل", "green"], CANCELLED: ["لغو شده", "red"],
};

export default function Campaigns() {
  const qc = useQueryClient();
  const nav = useNavigate();
  const { data } = useQuery({ queryKey: ["campaigns"], queryFn: () => api.listCampaigns() });
  const [creating, setCreating] = useState(false);
  const [deleting, setDeleting] = useState<CampaignView | null>(null);
  const del = useMutation({
    mutationFn: (id: string) => api.deleteCampaign(id),
    onSuccess: () => { setDeleting(null); qc.invalidateQueries({ queryKey: ["campaigns"] }); qc.invalidateQueries({ queryKey: ["stats"] }); },
  });

  return (
    <div className="space-y-6">
      <div className="flex items-center justify-between">
        <h1 className="text-2xl font-bold">کمپین‌ها</h1>
        <Button onClick={() => setCreating(true)}>+ کمپین جدید</Button>
      </div>
      {data?.length === 0 && <Card><p className="text-sm text-slate-600">هنوز کمپینی ندارید. تعداد مقالات هر کمپین را می‌توانید هنگام ایجاد مشخص کنید.</p></Card>}
      <div className="grid gap-4 md:grid-cols-2 xl:grid-cols-3">
        {data?.map((c) => {
          const [label, tone] = STATUS[c.status] ?? [c.status, "gray" as const];
          return (
            <Card key={c.id} className="flex flex-col gap-2">
              <div className="flex items-start justify-between gap-2">
                <h3 className="font-semibold">{c.name}</h3>
                <Badge tone={tone}>{label}</Badge>
              </div>
              <div className="text-xs text-slate-500">{c.projectName} · {faDate(c.createdAt)}</div>
              <div className="text-xs text-slate-600">
                عنوان‌های تأییدشده: {num(c.approvedCount)} / {num(c.targetCount)} · کلمات ثانویه: {num(c.secondaryCount)}
              </div>
              <div className="mt-2 flex gap-2 border-t border-slate-100 pt-3">
                <Link to={`/campaigns/${c.id}`}><Button>ادامه</Button></Link>
                <Button variant="ghost" className="text-red-600" onClick={() => setDeleting(c)}>حذف</Button>
              </div>
            </Card>
          );
        })}
      </div>
      {creating && <CreateDialog onClose={() => setCreating(false)} onCreated={(id) => { qc.invalidateQueries({ queryKey: ["campaigns"] }); nav(`/campaigns/${id}`); }} />}
      {deleting && (
        <ConfirmDialog
          title="حذف کمپین"
          message={`تمام کلمات، برنامه‌ی مقالات و عنوان‌های «${deleting.name}» حذف می‌شود. آیا مطمئن هستید؟`}
          confirmLabel="حذف کمپین" busy={del.isPending}
          onConfirm={() => del.mutate(deleting.id)} onCancel={() => setDeleting(null)}
        />
      )}
    </div>
  );
}

function CreateDialog({ onClose, onCreated }: { onClose: () => void; onCreated: (id: string) => void }) {
  const projects = useQuery({ queryKey: ["projects"], queryFn: api.listProjects });
  const [projectId, setProjectId] = useState("");
  const [name, setName] = useState("");
  const [targetCount, setTargetCount] = useState<number | null>(null);
  const [error, setError] = useState<string | null>(null);
  const create = useMutation({
    mutationFn: () => api.createCampaign({ projectId, name, targetCount: targetCount ?? undefined }),
    onSuccess: (c) => onCreated(c.id),
    onError: (e) => setError(errorMessage(e)),
  });
  return (
    <Modal title="کمپین جدید" onClose={onClose}>
      <div className="space-y-4">
        <Field label="پروژه">
          <select className={inputClass} value={projectId} onChange={(e) => { setProjectId(e.target.value); setTargetCount(projects.data?.find(p => p.id === e.target.value)?.defaultArticleCount ?? 60); }}>
            <option value="">انتخاب کنید…</option>
            {projects.data?.map((p) => <option key={p.id} value={p.id}>{p.name}</option>)}
          </select>
        </Field>
        <Field label="تعداد عنوان و مقاله این کمپین"><input className={`${inputClass} ltr`} type="number" min={1} max={200} value={targetCount ?? 60} onChange={(e) => setTargetCount(Number(e.target.value))} /></Field>
        <Field label="نام کمپین" hint="مثال: لپ تاپ دانشجویی - مهر ۱۴۰۵">
          <input className={inputClass} value={name} onChange={(e) => setName(e.target.value)} />
        </Field>
        <ErrorBox message={error} />
        <div className="flex justify-end gap-2">
          <Button variant="secondary" onClick={onClose}>انصراف</Button>
          <Button disabled={!projectId || !name.trim() || create.isPending} onClick={() => { setError(null); create.mutate(); }}>ایجاد</Button>
        </div>
      </div>
    </Modal>
  );
}
