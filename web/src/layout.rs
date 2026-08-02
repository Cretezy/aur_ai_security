use topcoat::{
    asset::{asset, Asset},
    context::Cx,
    router::{layout, uri},
    tailwind,
    view::view,
    Result,
};

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

#[layout("/")]
async fn application_layout(cx: &Cx, slot: Result) -> Result {
    let path = uri(cx).path();
    let title = page_title(path);
    let highlight_code = is_check_detail(path);

    view! {
        <!DOCTYPE html>
        <html lang="en">
            <head>
                <meta charset="utf-8">
                <meta name="viewport" content="width=device-width, initial-scale=1">
                <title>(title)</title>
                <link rel="stylesheet" href=(tailwind::stylesheet!())>
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
                        "AUR AI Security"
                    </a>
                    <a class="text-sky-300 hover:text-sky-200" href="/checks">
                        "Checks"
                    </a>
                    <a
                        class="text-slate-400 hover:text-white"
                        href="https://github.com/Cretezy/aur_ai_security"
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
        "/" => "AUR AI Security",
        "/search" => "Search · AUR AI Security",
        "/checks" => "Checks · AUR AI Security",
        _ if is_check_detail(path) => "Package check · AUR AI Security",
        _ if path.starts_with("/checks/") => "Package history · AUR AI Security",
        _ => "AUR AI Security",
    }
}

fn is_check_detail(path: &str) -> bool {
    path.strip_prefix("/checks/")
        .is_some_and(|rest| rest.split('/').count() == 2)
}
