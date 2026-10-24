import { useState } from "react";
import { Link } from "react-router-dom";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { api, ProjectCard, ProjectInput } from "../api/tauri";
import { Button, Card, ConfirmDialog, ErrorBox } from "../components/ui";
import { errorMessage, faDate, num } from "../lib/format";
import { WpBadge } from "./Dashboard";
import ProjectForm from "../components/ProjectForm";

export default function Projects() {
  const qc = useQueryClient();
  const { data, isLoading, error } = useQuery({ queryKey: ["projects"], queryFn: api.listProjects });
  const [editing, setEditing] = useState<ProjectCard | "new" | null>(null);
  const [deleting, setDeleting] = useState<ProjectCard | null>(null);
  const [delError, setDelError] = useState<string | null>(null);

  const del = useMutation({
    mutationFn: (id: string) => api.deleteProject(id),
    onSuccess: () => {
      setDeleting(null);
      qc.invalidateQueries({ queryKey: ["projects"] });
      qc.invalidateQueries({ queryKey: ["stats"] });
    },
    onError: (e) => setDelError(errorMessage(e)),
  });

  return (
    <div className="space-y-6">
      <div className="flex items-center justify-between">
        <h1 className="text-2xl font-bold">پروژه‌ها</h1>
        <Button onClick={() => setEditing("new")}>+ افزودن پروژه</Button>
      </div>

      <ErrorBox message={error ? errorMessage(error) : null} />
      {isLoading && <p className="text-sm text-slate-500">در حال بارگذاری…</p>}
      {data?.length === 0 && (
        <Card>
          <p className="text-sm text-slate-600">هنوز پروژه‌ای ثبت نشده است. هر پروژه یک سایت وردپرس مستقل است.</p>
        </Card>
      )}

      <div className="grid gap-4 md:grid-cols-2 xl:grid-cols-3">
        {data?.map((p) => (
          <Card key={p.id} className="flex flex-col">
            <div className="flex items-start justify-between gap-2">
              <h3 className="font-semibold">{p.name}</h3>
              <WpBadge status={p.wpStatus} />
            </div>
            <div className="ltr mt-1 truncate text-xs text-slate-500">{p.siteUrl}</div>
            <dl className="mt-4 grid grid-cols-2 gap-2 text-xs text-slate-600">
              <div>کمپین‌ها: {num(p.campaignCount)}</div>
              <div>مقالات: {num(p.articleCount)}</div>
              <div className="col-span-2">آخرین فعالیت: {faDate(p.lastActivityAt ?? p.updatedAt)}</div>
            </dl>
            <div className="mt-4 flex flex-wrap gap-2 border-t border-slate-100 pt-3">
              <Link to={`/projects/${p.id}`}>
                <Button>باز کردن</Button>
              </Link>
              <Button variant="secondary" onClick={() => setEditing(p)}>
                ویرایش
              </Button>
              <Button variant="ghost" className="text-red-600" onClick={() => { setDelError(null); setDeleting(p); }}>
                حذف
              </Button>
            </div>
          </Card>
        ))}
      </div>

      {editing && (
        <ProjectForm
          project={editing === "new" ? null : editing}
          onClose={() => setEditing(null)}
          onSaved={() => {
            setEditing(null);
            qc.invalidateQueries({ queryKey: ["projects"] });
            qc.invalidateQueries({ queryKey: ["stats"] });
          }}
        />
      )}

      {deleting && (
        <ConfirmDialog
          title="حذف پروژه"
          message={`تمام اطلاعات مرتبط با «${deleting.name}» حذف خواهد شد. آیا مطمئن هستید؟${delError ? "\n" + delError : ""}`}
          confirmLabel="حذف پروژه"
          busy={del.isPending}
          onConfirm={() => del.mutate(deleting.id)}
          onCancel={() => setDeleting(null)}
        />
      )}
    </div>
  );
}

export type { ProjectInput };
