import { useEffect, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { api, ProductItem } from "../api/tauri";
import { Button, Card, ErrorBox, inputClass } from "../components/ui";
import { errorMessage, num } from "../lib/format";

const QUOTA_DONE_TIP = "سهمیه‌ی توضیحات محصول تمام شده است";

export default function Products() {
  const qc = useQueryClient();
  const projects = useQuery({ queryKey: ["projects"], queryFn: api.listProjects });
  const quota = useQuery({ queryKey: ["quota"], queryFn: api.getQuota });
  const productQuota = quota.data?.products;
  const quotaDone = !!productQuota && productQuota.available <= 0;
  const [projectId, setProjectId] = useState("");
  // Fixed provider created by the license activation (hidden from the provider lists).
  const providerId = "builtin-text";
  const [search, setSearch] = useState("");
  const [keyword, setKeyword] = useState("");
  const [page, setPage] = useState(1);
  const [selected, setSelected] = useState<ProductItem | null>(null);
  const [description, setDescription] = useState("");
  const [msg, setMsg] = useState("");
  const [error, setError] = useState<string | null>(null);
  useEffect(() => { if (!projectId && projects.data?.length) setProjectId(projects.data[0].id); }, [projects.data, projectId]);
  const products = useQuery({ queryKey: ["wc-products", projectId, search, page], queryFn: () => api.listProducts(projectId, search, page), enabled: !!projectId, retry: false });
  const productsError = products.error ? errorMessage(products.error) : null;
  const generate = useMutation({
    mutationFn: () => api.generateProductDescription(providerId, selected!.name, keyword, description, projects.data?.find((p) => p.id === projectId)?.defaultLanguage ?? "fa"),
    onSuccess: (text) => { setDescription(text); setError(null); setMsg("توضیحات تولید شد؛ پیش از ذخیره بررسی کنید."); },
    onError: (e) => setError(errorMessage(e)),
    onSettled: () => qc.invalidateQueries({ queryKey: ["quota"] }),
  });
  const save = useMutation({
    mutationFn: () => api.saveProductDescription(projectId, selected!.id, description),
    onSuccess: () => { setError(null); setMsg("فقط توضیحات تکمیلی این محصول در وردپرس ذخیره شد."); products.refetch(); }, onError: (e) => setError(errorMessage(e)),
  });
  return <div className="max-w-6xl space-y-5">
    <h1 className="text-2xl font-bold">توضیحات محصولات ووکامرس</h1>
    <p className="text-sm leading-7 text-slate-600">فقط فیلد توضیحات بلند محصول (description) تغییر می‌کند؛ عنوان، قیمت، موجودی، SKU و توضیح کوتاه ثابت می‌مانند. برای این بخش افزونه SEO Master Bridge باید در سایت فعال باشد و کاربر وردپرس اجازه ویرایش محصولات داشته باشد.</p>
    <Card className="flex flex-wrap gap-3 items-end">
      <label className="text-sm">پروژه<select className={`${inputClass} block mt-1`} value={projectId} onChange={(e) => { setProjectId(e.target.value); setSelected(null); setPage(1); }}><option value="">انتخاب پروژه</option>{projects.data?.map(p => <option key={p.id} value={p.id}>{p.name}</option>)}</select></label>
      <label className="flex-1 text-sm">جستجوی محصول<input className={`${inputClass} block mt-1`} value={search} onChange={e => {setSearch(e.target.value);setPage(1);}} placeholder="نام محصول" /></label>
    </Card>
    {productsError && (productsError.includes("ووکامرس") ? (
      <div className="rounded-lg border border-amber-200 bg-amber-50 px-4 py-3 text-sm text-amber-800 space-y-1">
        <div className="font-semibold">{productsError}</div>
        <div className="text-xs text-amber-700">ووکامرس را روی سایت نصب و فعال کنید.</div>
      </div>
    ) : <ErrorBox message={`${productsError} — اتصال وردپرس و فعال بودن افزونه رابط را بررسی کنید.`} />)}
    <div className="grid grid-cols-1 lg:grid-cols-2 gap-4">
      <Card className="space-y-3">
        <h2 className="font-semibold">محصولات سایت ({num(products.data?.total ?? 0)})</h2>
        <div className="max-h-[32rem] overflow-auto space-y-1">
          {(products.isLoading || (products.isFetching && !products.data)) && Array.from({ length: 6 }).map((_, i) => (
            <div key={i} className="p-3 rounded-lg border border-slate-100 animate-pulse space-y-2" aria-hidden="true">
              <div className="h-4 w-3/4 rounded bg-slate-200" />
              <div className="h-3 w-1/2 rounded bg-slate-100" />
            </div>
          ))}
          {!products.isLoading && products.data?.items.length === 0 && <p className="text-sm text-slate-500">محصولی یافت نشد.</p>}
          {products.data?.items.map((p) => <button key={p.id} onClick={() => {setSelected(p);setDescription(p.description);setKeyword(p.name);setMsg("");setError(null);}} className={`block w-full p-3 rounded-lg border text-start ${selected?.id===p.id ? "border-brand-500 bg-brand-50" : "border-slate-100 hover:bg-slate-50"}`}><span className="font-medium">{p.name}</span><span className="block text-xs text-slate-400 ltr">SKU: {p.sku || "—"} / ID: {p.id}</span></button>)}
        </div>
        <div className="flex items-center gap-3"><Button variant="secondary" disabled={page <= 1} onClick={() => setPage(p => p-1)}>قبلی</Button><span>{num(page)} / {num(products.data?.pages ?? 1)}</span><Button variant="secondary" disabled={page >= (products.data?.pages ?? 1)} onClick={() => setPage(p => p+1)}>بعدی</Button></div>
      </Card>
      <Card className="space-y-4">
        {selected ? <>
          <h2 className="font-semibold">{selected.name}</h2>
          <label className="text-sm block">کلمه کلیدی برای تولید متن<input className={`${inputClass} block mt-1`} value={keyword} onChange={e=>setKeyword(e.target.value)} /></label>
          <div className="flex flex-wrap items-center gap-3">
            <span title={quotaDone ? QUOTA_DONE_TIP : undefined} className="inline-flex">
              <Button className="ai-btn" disabled={!providerId || !keyword.trim() || generate.isPending || quotaDone} title={quotaDone ? QUOTA_DONE_TIP : undefined} onClick={() => generate.mutate()}>{generate.isPending ? "در حال تولید…" : "تولید توضیحات با AI"}</Button>
            </span>
            <span className="text-xs text-slate-500">سهمیه‌ی باقی‌مانده‌ی توضیحات محصول: <b className="text-brand-700">{productQuota ? `${num(productQuota.remaining)} از ${num(productQuota.total)}` : "…"}</b></span>
          </div>
          <label className="text-sm block">توضیحات تکمیلی<textarea className={`${inputClass} block mt-1 min-h-80`} value={description} onChange={e=>setDescription(e.target.value)} /></label>
          <ErrorBox message={error}/>{msg && <p className="text-sm text-emerald-700">{msg}</p>}
          <Button disabled={save.isPending || !description.trim()} onClick={() => save.mutate()}>{save.isPending ? "در حال ذخیره…" : "ذخیره فقط توضیحات محصول"}</Button>
        </> : <p className="text-slate-500 text-sm">برای ویرایش توضیحات، یک محصول از لیست انتخاب کنید.</p>}
      </Card>
    </div>
  </div>;
}
