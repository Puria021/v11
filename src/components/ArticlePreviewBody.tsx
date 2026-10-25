import { useMemo } from "react";
import { useQuery } from "@tanstack/react-query";
import { api } from "../api/tauri";

type Image = { src: string; alt: string; isFeatured: boolean };

/** Pre-upload article preview. Media is stored locally and is NOT embedded in article HTML.
 * WordPress URLs do not exist yet, so use data URLs only in this read-only view.
 */
export default function ArticlePreviewBody({ planId, html }: { planId: string; html: string }) {
  const media = useQuery({ queryKey: ["articlePreviewImages", planId], queryFn: () => api.getArticlePreviewImages(planId) });
  const sanitized = useQuery({ queryKey: ["safeHtml", html], queryFn: () => api.previewHtml(html), staleTime: Infinity });
  // The featured image is shown at the top (as WordPress does); the rest are placed inside the text.
  const featured = (media.data?.images ?? []).find((i) => i.isFeatured);
  const images = (media.data?.images ?? []).filter((i) => !i.isFeatured);
  const interval = Math.max(1, media.data?.paragraphInterval ?? 3);

  const pieces = useMemo(() => {
    const safe = sanitized.data ?? "";
    // Splitting retains paragraph closing tags, which are where WordPress also inserts images.
    const parts = safe.split(/(<\/p\s*>)/i);
    const blocks: string[] = [];
    for (let i = 0; i < parts.length; i += 2) {
      const block = (parts[i] ?? "") + (parts[i + 1] ?? "");
      if (block) blocks.push(block);
    }
    return blocks;
  }, [sanitized.data]);

  if (sanitized.isPending) return <p className="text-sm text-slate-500">در حال آماده‌سازی پیش‌نمایش…</p>;
  if (sanitized.isError) return <p className="text-sm text-red-600">خطا در پیش‌نمایش متن مقاله.</p>;

  let shown = 0;
  let paragraphs = 0;
  const elements = pieces.map((piece, i) => {
    if (/<\/p\s*>$/i.test(piece)) paragraphs++;
    const shouldInsert = shown < images.length && paragraphs > 0 && (paragraphs % interval === 0 || (i === pieces.length - 1 && shown === 0)) && /<\/p\s*>$/i.test(piece);
    const image: Image | undefined = shouldInsert ? images[shown++] : undefined;
    return <div key={i} className="contents">
      <div className="contents" dangerouslySetInnerHTML={{ __html: piece }} />
      {image && <figure className="my-4"><img src={image.src} alt={image.alt} className="mx-auto max-h-[500px] max-w-full rounded-lg object-contain" /><figcaption className="mt-1 text-center text-xs text-slate-500">{image.alt || (image.isFeatured ? "تصویر شاخص" : "تصویر مقاله")}</figcaption></figure>}
    </div>;
  });
  const remaining = images.slice(shown);
  return <div className="prose-fa space-y-3 text-sm leading-8 [&_.mam-cta-card]:rounded-2xl [&_.mam-cta-card]:bg-gradient-to-l [&_.mam-cta-card]:from-blue-900 [&_.mam-cta-card]:to-slate-900 [&_.mam-cta-card]:p-6 [&_.mam-cta-card]:text-center [&_.mam-cta-card]:text-white [&_.mam-cta-card_a]:rounded-full [&_.mam-cta-card_a]:bg-white [&_.mam-cta-card_a]:px-6 [&_.mam-cta-card_a]:py-3 [&_.mam-cta-card_a]:font-bold [&_.mam-cta-card_a]:text-blue-900 [&_h2]:mt-4 [&_h2]:text-lg [&_h2]:font-bold [&_h3]:font-semibold [&_li]:ms-5 [&_ol]:list-decimal [&_table]:border [&_td]:border [&_td]:p-1 [&_th]:border [&_th]:p-1 [&_ul]:list-disc">
    {featured && <figure className="my-4"><img src={featured.src} alt={featured.alt} className="mx-auto max-h-[500px] max-w-full rounded-lg object-contain" /><figcaption className="mt-1 text-center text-xs text-slate-500">{featured.alt || "تصویر شاخص"}</figcaption></figure>}
    {elements}
    {remaining.map((image, i) => <figure key={`remaining-${i}`} className="my-4"><img src={image.src} alt={image.alt} className="mx-auto max-h-[500px] max-w-full rounded-lg object-contain" /><figcaption className="text-center text-xs text-slate-500">{image.alt || "تصویر مقاله"}</figcaption></figure>)}
    {media.isError && <p className="text-xs text-red-600">خطا در بارگذاری تصاویر محلی.</p>}
  </div>;
}
