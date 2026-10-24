# اصلاح نمایش تصاویر پیش از آپلود وردپرس

مشکل: تصویرها در دیتابیس و فایل‌های محلی وجود داشتند، ولی پیش‌نمایش فقط تصویر شاخص را در بالای صفحه نمایش می‌داد. درج تصویر میان متن فقط در مرحله آپلود انجام می‌شد.

تغییرات: API جدید get_article_preview_images تمام تصاویر محلی ذخیره‌شده و فاصله تصاویر قالب را به رابط می‌دهد. کامپوننت ArticlePreviewBody تصاویر را در پیش‌نمایش مقاله میان پاراگراف‌ها نشان می‌دهد. تصاویر با data URL در پیش‌نمایش نمایش داده می‌شوند و داخل HTML ذخیره نمی‌شوند؛ بنابراین هنگام انتشار وردپرس آدرس رسانه‌ها جداگانه اضافه می‌شود.

فایل‌های تغییرکرده:
- src-tauri/src/services/generation.rs
- src-tauri/src/commands/generation.rs
- src-tauri/src/lib.rs
- src/api/tauri.ts
- src/components/ArticleEditor.tsx
- src/components/StepGenerate.tsx
- src/components/ArticlePreviewBody.tsx (جدید)

بیلد Windows در این محیط اجرا نشده است.
