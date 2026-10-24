import { Card } from "../components/ui";

export default function Placeholder({ title, phase }: { title: string; phase: string }) {
  return (
    <div>
      <h1 className="mb-6 text-2xl font-bold">{title}</h1>
      <Card>
        <p className="text-sm text-slate-600">این بخش در {phase} پیاده‌سازی می‌شود.</p>
      </Card>
    </div>
  );
}
