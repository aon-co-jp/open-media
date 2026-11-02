# ملاحظات التسليم بين الجلسات (make-disk) — ملخص

**اللغات**: [日本語 (النص الكامل، المرجع)](../PORTING.md) | [English](PORTING.en.md) | [简体中文](PORTING.zh-CN.md) | [繁體中文(台灣)](PORTING.zh-TW.md) | [한국어](PORTING.ko.md) | [Deutsch](PORTING.de.md) | [Français](PORTING.fr.md) | [Русский](PORTING.ru.md) | [Українська](PORTING.uk.md) | [فارسی](PORTING.iran%28Perusha%29.md) | العربية

<div dir="rtl">

> ملخص لما وصل إليه العمل والخطوات التالية. التاريخ الكامل لملاحظات الاستئناف هو ملف [`PORTING.md`](../PORTING.md) الياباني؛ والقرارات التقنية في [`CLAUDE.md`](../CLAUDE.md).

## الوضع الحالي (2026-09-23)

- المنجز حتى الآن: النسخ عبر IMAPI2 (تم التحقق بقرص CD-R حقيقي)، وAV1/Opus، والحفاظ على Dolby/الصوت المحيطي، وإزالة الضجيج بالذكاء الاصطناعي (RNNoise)، وDSD64 إلى 1024 (DSF) وملفات WAV من نوع DoP،
  وPCM عالي الدقة، والدقة الفائقة للفيديو بالذكاء الاصطناعي (GPU/المعالج)، وتوسيع النطاق الصوتي التجريبي، واستخراج أقراص الصوت (تم التحقق بقرص حقيقي)، ومسارات صوت/ترجمة متعددة في MKV.
- 2026-09-23: إصلاح قص المقاطع الذي تعذّر استخدامه من القسم 8؛ لم يعد PCM يُنشأ مع DSD؛ خياران متنافيان بنعم/لا «القص حسب الحجم / حسب الوقت» (الافتراضي الحجم)؛
  مربعات معالجة لاحقة «ملء القرص» و«قص الصمت بالذكاء الاصطناعي»؛ حدود معايير التشغيل (CD / DVD-Video / DVD-Audio / Blu-ray / UHD Blu-ray / الحاسوب فقط)؛ توثيق متعدد اللغات.

## الخطوات التالية

1. اختبار E2E بملفات حقيقية لميزات 2026-09-23: مواضع القص حسب الحجم/الوقت، وحدود `-ar` وعمق البت ومعدل البت لكل معيار، ومعدل البت لـ«ملء القرص».
2. نسخ أقراص الصوت (CD-DA) (IMAPI2 TrackAtOnce؛ حاليًا أقراص البيانات فقط).
3. إخراج أدوات `rs-*` من برنامج التثبيت وتنزيلها عند الطلب من إصدارات المستودعات الشقيقة.
4. تصدير «فيديو + صوت DSD» كمجموعة واحدة (ملف فيديو + `.dsf` + `.obar.json` لـ `open-bar`)، وفصل معدِّل دلتا-سيغما إلى `open-mqa-dsd`.
5. استخراج أقراص BD/DVD غير المحمية (لن يُنفَّذ تجاوز الحماية من النسخ).

## المستودعات ذات الصلة

- [aon-co-jp/make-disk](https://github.com/aon-co-jp/make-disk) — هذا التطبيق
- [aon-co-jp/rs-FFmpeg](https://github.com/aon-co-jp/rs-FFmpeg) / [aon-co-jp/rs-xorriso](https://github.com/aon-co-jp/rs-xorriso) — إصدارات Rust من FFmpeg / xorriso
- [aon-co-jp/open-cpu](https://github.com/aon-co-jp/open-cpu) — اكتشاف مجموعات تعليمات المعالج
- [aon-co-jp/open-cuda](https://github.com/aon-co-jp/open-cuda) — طبقة تجريد حوسبة GPU
- [aon-co-jp/open-mqa](https://github.com/aon-co-jp/open-mqa) / [aon-co-jp/open-mqa-dsd](https://github.com/aon-co-jp/open-mqa-dsd) / [aon-co-jp/open-bar](https://github.com/aon-co-jp/open-bar) — سلسلة الصوت عالي الدقة، وأدوات DSD، والمشغّل

</div>
