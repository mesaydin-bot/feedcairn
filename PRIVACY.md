# Privacy / Gizlilik

## English

- Feedcairn runs on your computer. There is no Feedcairn account, server, analytics or telemetry.
- Your data (your subscriptions, articles, notes, prompt templates and settings in a local SQLite database in the OS app-data folder) stays in local files on your computer.
- AI features send the text you choose (for example a selection, an article or a draft) to the AI provider whose CLI you selected, under your own subscription and that provider's terms and privacy policy. Nothing is sent to any AI provider unless you run an AI action.
- Feedcairn never stores AI passwords, tokens or API keys; each provider's official CLI keeps its own login.
- Feedcairn fetches only the RSS/Atom feeds you subscribe to (and article pages when you ask for the full text or a web RSS search). It starts with an empty library; no feeds are preloaded.
- The local HTTP interface (if any) listens only on 127.0.0.1, checks the Host and Origin headers and requires a random per-launch token, so web pages you visit cannot use it.

## Türkçe

- Feedcairn bilgisayarınızda çalışır. Feedcairn hesabı, sunucusu, analitiği veya telemetrisi yoktur.
- Verileriniz (abonelikleriniz, makaleler, notlar, istem şablonları ve ayarlar; işletim sisteminin uygulama veri klasöründeki yerel SQLite veritabanında) bilgisayarınızdaki yerel dosyalarda kalır.
- AI özellikleri seçtiğiniz metni (ör. seçim, makale veya taslak) yalnızca sizin seçtiğiniz sağlayıcının CLI'sine, kendi aboneliğinizle ve o sağlayıcının koşullarıyla gönderir. Bir AI işlemi başlatmadıkça hiçbir şey gönderilmez.
- Feedcairn AI parolası, token veya API anahtarı saklamaz; giriş her sağlayıcının resmî CLI'sinde kalır.
- Feedcairn yalnızca abone olduğunuz RSS/Atom kaynaklarını (ve tam metin ya da web RSS araması istediğinizde makale sayfalarını) getirir. Boş bir kitaplıkla başlar; hazır yüklenmiş kaynak yoktur.
- Yerel HTTP arayüzü yalnızca 127.0.0.1 üzerinde dinler, Host ve Origin başlıklarını denetler ve her açılışta üretilen rastgele bir token ister; ziyaret ettiğiniz web sayfaları onu kullanamaz.
