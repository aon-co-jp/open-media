# یادداشت‌های تحویل بین جلسه‌ها (make-disk) — خلاصه

**زبان‌ها**: [日本語 (متن کامل، مرجع)](../PORTING.md) | [English](PORTING.en.md) | [简体中文](PORTING.zh-CN.md) | [繁體中文(台灣)](PORTING.zh-TW.md) | [한국어](PORTING.ko.md) | [Deutsch](PORTING.de.md) | [Français](PORTING.fr.md) | [Русский](PORTING.ru.md) | [Українська](PORTING.uk.md) | فارسی | [العربية](PORTING.ar.md)

<div dir="rtl">

> خلاصهٔ وضعیت فعلی کار و گام‌های بعدی. تاریخچهٔ کامل یادداشت‌های ازسرگیری فایل ژاپنی [`PORTING.md`](../PORTING.md) است؛ تصمیم‌های فنی در [`CLAUDE.md`](../CLAUDE.md) آمده‌اند.

## وضعیت فعلی (2026-09-23)

- انجام‌شده تاکنون: رایت با IMAPI2 (با CD-R واقعی تأیید شده)، AV1/Opus، حفظ Dolby/صدای فراگیر، کاهش نویز با هوش مصنوعی (RNNoise)، DSD64 تا 1024 (DSF) و WAV از نوع DoP،
  PCM با وضوح بالا، ابرتفکیک ویدئو با هوش مصنوعی (GPU/CPU)، گسترش آزمایشی پهنای باند صوت، استخراج CD صوتی (با دیسک واقعی تأیید شده)، چند ترک صوتی/زیرنویس در MKV.
- 2026-09-23: برش بازه‌ای که از بخش 8 قابل استفاده نبود اصلاح شد؛ هنگام ساخت DSD دیگر PCM ساخته نمی‌شود؛ بله/خیر انحصاری «برش بر اساس اندازه / بر اساس زمان» (پیش‌فرض اندازه)؛
  چک‌باکس‌های پس‌پردازش «پر کردن دیسک» و «برش سکوت با هوش مصنوعی»؛ سقف استانداردهای پخش (CD / DVD-Video / DVD-Audio / Blu-ray / UHD Blu-ray / فقط رایانه)؛ مستندات چندزبانه.

## گام‌های بعدی

1. آزمون E2E با فایل‌های واقعی برای قابلیت‌های 2026-09-23: موقعیت برش بر اساس اندازه/زمان، سقف `-ar` / عمق بیت / نرخ بیت برای هر استاندارد، نرخ بیت «پر کردن دیسک».
2. رایت CD صوتی (CD-DA) (IMAPI2 TrackAtOnce؛ فعلاً فقط دیسک داده).
3. حذف ابزارهای `rs-*` از نصب‌کننده و دانلود درخواستی آن‌ها از انتشارهای مخزن‌های مرتبط.
4. خروجی گرفتن «ویدئو + صوت DSD» به‌صورت یک مجموعه (فایل ویدئو + `.dsf` + `.obar.json` برای `open-bar`) و جدا کردن مدولاتور دلتا-سیگما به `open-mqa-dsd`.
5. استخراج BD/DVD بدون حفاظت (دور زدن حفاظت کپی پیاده‌سازی نخواهد شد).

## مخزن‌های مرتبط

- [aon-co-jp/make-disk](https://github.com/aon-co-jp/make-disk) — همین برنامه
- [aon-co-jp/rs-FFmpeg](https://github.com/aon-co-jp/rs-FFmpeg) / [aon-co-jp/rs-xorriso](https://github.com/aon-co-jp/rs-xorriso) — نسخه‌های Rust از FFmpeg / xorriso
- [aon-co-jp/open-cpu](https://github.com/aon-co-jp/open-cpu) — تشخیص مجموعه‌دستورهای CPU
- [aon-co-jp/open-cuda](https://github.com/aon-co-jp/open-cuda) — لایهٔ انتزاعی محاسبات GPU
- [aon-co-jp/open-mqa](https://github.com/aon-co-jp/open-mqa) / [aon-co-jp/open-mqa-dsd](https://github.com/aon-co-jp/open-mqa-dsd) / [aon-co-jp/open-bar](https://github.com/aon-co-jp/open-bar) — زنجیرهٔ صوت با وضوح بالا، ابزارهای DSD، پخش‌کننده

</div>
