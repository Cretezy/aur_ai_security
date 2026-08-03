use topcoat::{
    asset::{asset, Asset},
    context::Cx,
    router::{header, layout, route, uri, HeaderMap, HeaderValue},
    view::{view, Unescaped},
    Result,
};

const BRAND: &str = "AUR Security";
const SITE_URL: &str = "https://aur-security.cretezy.com";
const DESCRIPTION: &str =
    "AI-assisted security reviews for Arch User Repository packages, with the exact PKGBUILD and Git diff behind every assessment.";
const STRUCTURED_DATA: &str = r#"{"@context":"https://schema.org","@type":"WebSite","name":"AUR Security","url":"https://aur-security.cretezy.com","description":"AI-assisted security reviews for Arch User Repository packages, with the exact PKGBUILD and Git diff behind every assessment."}"#;

const HIGHLIGHT_JS: Asset = asset!(
    "https://cdn.jsdelivr.net/gh/highlightjs/cdn-release@11.11.1/build/highlight.min.js",
    rename: "highlight",
    checksum: "sha256:c4a399dd6f488bc97a3546e3476747b3e714c99c57b9473154c6fb8d259b9381",
);
const HIGHLIGHT_THEME: Asset = asset!(
    "https://cdn.jsdelivr.net/gh/highlightjs/cdn-release@11.11.1/build/styles/github-dark.min.css",
    rename: "highlight-github-dark",
    checksum: "sha256:9f208d022102b1d0c7aebfecd8e42ca7997d5de636649d2b31ea63093d809019",
);
const HIGHLIGHT_INIT: Asset = asset!("assets/highlight-init.js");

const STYLESHEET: Asset = asset!("assets/styles.generated.css");

#[layout("/")]
async fn application_layout(cx: &Cx, slot: Result) -> Result {
    let path = uri(cx).path();
    let title = page_title(path);
    let highlight_code = is_check_detail(path);
    let canonical = canonical_url(path);

    view! {
        <!DOCTYPE html>
        <html lang="en">
            <head>
                <meta charset="utf-8">
                <meta name="viewport" content="width=device-width, initial-scale=1">
                <meta name="description" content=(DESCRIPTION)>
                <meta name="robots" content="index, follow">
                <meta name="theme-color" content="#0a0a0a">
                <link rel="canonical" href=(canonical.as_str())>
                <meta property="og:site_name" content=(BRAND)>
                <meta property="og:type" content="website">
                <meta property="og:url" content=(canonical.as_str())>
                <meta property="og:title" content=(title)>
                <meta property="og:description" content=(DESCRIPTION)>
                <meta name="twitter:card" content="summary">
                <meta name="twitter:title" content=(title)>
                <meta name="twitter:description" content=(DESCRIPTION)>
                <title>(title)</title>
                <script type="application/ld+json">
                    (Unescaped::new_unchecked(STRUCTURED_DATA))
                </script>
                <link rel="stylesheet" href=(STYLESHEET)>
                if highlight_code {
                    <link rel="stylesheet" href=(HIGHLIGHT_THEME)>
                    <script src=(HIGHLIGHT_JS) defer=""></script>
                    <script src=(HIGHLIGHT_INIT) defer=""></script>
                }
            </head>
            <body class="min-h-screen bg-neutral-950 text-slate-100 antialiased">
                <nav
                    class="mx-auto flex w-full max-w-6xl items-center gap-5 border-b border-slate-800 px-4 py-5"
                >
                    <a class="mr-auto font-bold text-white hover:text-sky-300" href="/">
                        (BRAND)
                    </a>
                    <a class="text-sky-300 hover:text-sky-200" href="/checks">
                        "Checks"
                    </a>
                    <a
                        class="text-slate-400 hover:text-white"
                        href="https://github.com/Cretezy/aur-security"
                    >
                        "GitHub ↗"
                    </a>
                </nav>
                <main class="mx-auto w-full max-w-6xl px-4 py-12">(slot?)</main>
            </body>
        </html>
    }
}

fn page_title(path: &str) -> &'static str {
    match path {
        "/" => BRAND,
        "/search" => "Search · AUR Security",
        "/checks" => "Checks · AUR Security",
        _ if is_check_detail(path) => "Package check · AUR Security",
        _ if path.starts_with("/checks/") => "Package history · AUR Security",
        _ => BRAND,
    }
}

fn canonical_url(path: &str) -> String {
    format!("{SITE_URL}{path}")
}

#[route(GET "/robots.txt")]
async fn robots_txt() -> Result<&'static str> {
    Ok("User-agent: *\nAllow: /\nDisallow: /api/\nDisallow: /search\n\nSitemap: https://aur-security.cretezy.com/sitemap.xml\n")
}

#[route(GET "/sitemap.xml")]
async fn sitemap_xml() -> Result<(HeaderMap, String)> {
    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/xml; charset=utf-8"),
    );
    let body = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<urlset xmlns=\"http://www.sitemaps.org/schemas/sitemap/0.9\">\n  <url><loc>{SITE_URL}/</loc></url>\n  <url><loc>{SITE_URL}/checks</loc></url>\n</urlset>\n"
    );
    Ok((headers, body))
}

fn is_check_detail(path: &str) -> bool {
    path.strip_prefix("/checks/")
        .is_some_and(|rest| rest.split('/').count() == 2)
}
