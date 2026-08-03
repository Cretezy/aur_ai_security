use topcoat::{
    context::Cx,
    router::{page, query_params},
    view::view,
    Result,
};

use crate::{database, ui::aur_package_url};

#[query_params(error = bad_request)]
struct SearchQuery {
    q: Option<String>,
}

#[page("/search")]
async fn search_page(cx: &Cx) -> Result {
    let query = query_params::<SearchQuery>(cx)?;
    let term = query.q.as_deref().unwrap_or("").trim();
    let packages = if term.is_empty() {
        Vec::new()
    } else {
        database(cx).search_packages(term).await?
    };

    view! {
        <h1 class="text-4xl font-black tracking-tight sm:text-5xl">
            "Search packages"
        </h1>
        <p class="mt-3 text-slate-400">
            "Search current packages from the latest AUR index. Results are limited to the 100 most popular matches."
        </p>
        <form
            class="mt-8 flex flex-col gap-2 sm:flex-row"
            method="get"
            action="/search"
        >
            <input
                class="min-w-0 flex-1 rounded-lg border border-slate-700 bg-slate-900 px-4 py-3 text-white outline-none placeholder:text-slate-500 focus:border-sky-400 focus:ring-2 focus:ring-sky-400/20"
                name="q"
                value=(term)
                placeholder="Search AUR packages…"
                aria-label="Package name"
            >
            <button
                class="rounded-lg bg-sky-400 px-5 py-3 font-bold text-slate-950 transition hover:bg-sky-300"
                type="submit"
            >
                "Search"
            </button>
        </form>
        if !term.is_empty() {
            <h2 class="mb-4 mt-10 text-2xl font-bold">
                "Results for “"
                (term)
                "”"
            </h2>
            if packages.is_empty() {
                <p class="text-slate-400">
                    "No current AUR packages matched this search."
                </p>
            } else {
                <div class="grid gap-3">
                    for package in packages {
                        <article
                            class="flex flex-col gap-4 rounded-xl border border-slate-800 bg-slate-900 p-5 sm:flex-row sm:items-center"
                        >
                            <div class="min-w-0 flex-1">
                                <strong class="break-all text-lg">
                                    (package.package_name.as_str())
                                </strong>
                                <div
                                    class="mt-1 flex flex-wrap gap-x-5 gap-y-1 text-sm text-slate-400"
                                >
                                    <span>
                                        "Version "
                                        (package.version.as_str())
                                    </span>
                                    <span>
                                        "Base "
                                        (package.package_base.as_str())
                                    </span>
                                </div>
                            </div>
                            <div class="flex flex-wrap gap-3">
                                <a
                                    class="text-sky-300 hover:text-sky-200"
                                    href=(format!("/checks/{}", package.package_base))
                                >
                                    "View checks →"
                                </a>
                                <a
                                    class="text-slate-300 hover:text-white"
                                    href=(aur_package_url(&package.package_name))
                                >
                                    "AUR ↗"
                                </a>
                            </div>
                        </article>
                    }
                </div>
            }
        }
    }
}
