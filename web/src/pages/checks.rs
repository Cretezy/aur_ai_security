use aur_ai_security_db as db;
use pulldown_cmark::{html, CowStr, Event, Options, Parser, Tag};
use topcoat::{
    context::Cx,
    router::{error::not_found, page, path_param, query_params},
    view::{view, Unescaped},
    Result,
};
use tracing::debug;

use crate::{
    database,
    ui::{aur_commit_url, aur_package_url, check_card, format_timestamp, verdict_class},
};

#[query_params(error = bad_request)]
struct ChecksQuery {
    page: Option<u32>,
    q: Option<String>,
    verdict: Option<String>,
}

#[query_params(error = bad_request)]
struct CheckDetailQuery {
    view: Option<String>,
}

#[path_param]
struct Repo(str);

#[path_param]
struct Commit(str);

fn markdown_explanation(markdown: &str) -> Unescaped<String> {
    let options =
        Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TABLES | Options::ENABLE_TASKLISTS;
    let events = Parser::new_ext(markdown, options).map(|event| match event {
        // Explanations can contain package-controlled text, so raw HTML must
        // remain visible as text rather than becoming executable markup.
        Event::Html(html) | Event::InlineHtml(html) => Event::Text(html),
        Event::Start(Tag::Link {
            link_type,
            dest_url,
            title,
            id,
        }) => Event::Start(Tag::Link {
            link_type,
            dest_url: safe_markdown_url(dest_url),
            title,
            id,
        }),
        Event::Start(Tag::Image {
            link_type,
            dest_url,
            title,
            id,
        }) => Event::Start(Tag::Image {
            link_type,
            dest_url: safe_markdown_url(dest_url),
            title,
            id,
        }),
        event => event,
    });

    let mut rendered = String::new();
    html::push_html(&mut rendered, events);
    Unescaped::new_unchecked(rendered)
}

fn safe_markdown_url(url: CowStr<'_>) -> CowStr<'_> {
    let normalized = url.trim().to_ascii_lowercase();
    let is_safe = normalized.starts_with("http://")
        || normalized.starts_with("https://")
        || normalized.starts_with("mailto:")
        || normalized.starts_with('/')
        || normalized.starts_with('#')
        || !normalized.contains(':');

    if is_safe {
        url
    } else {
        CowStr::Borrowed("#")
    }
}

#[page("/checks")]
async fn checks_page(cx: &Cx) -> Result {
    let query = query_params::<ChecksQuery>(cx)?;
    let page = i64::from(query.page.unwrap_or(1).max(1));
    let search = query.q.as_deref().unwrap_or("").trim();
    let verdict = match query.verdict.as_deref() {
        Some("safe") => Some("safe"),
        Some("suspicious") => Some("suspicious"),
        Some("dangerous") => Some("dangerous"),
        _ => None,
    };
    let (checks, total) = database(cx).recent_checks(page, search, verdict).await?;
    let pages = ((total + db::PAGE_SIZE - 1) / db::PAGE_SIZE).max(1);

    view! {
        <h1 class="text-4xl font-black tracking-tight sm:text-5xl">"Latest checks"</h1>
        <p class="mb-8 mt-3 text-slate-400">
            (total)
            " matching assessments, newest first."
        </p>
        <form
            class="mb-8 grid gap-3 rounded-xl border border-slate-800 bg-slate-900 p-4 sm:grid-cols-[minmax(0,1fr)_auto_auto]"
            method="get"
            action="/checks"
        >
            <input
                class="min-w-0 rounded-lg border border-slate-700 bg-neutral-950 px-4 py-3 text-white outline-none placeholder:text-slate-500 focus:border-sky-400 focus:ring-2 focus:ring-sky-400/20"
                name="q"
                value=(search)
                placeholder="Filter by package…"
                aria-label="Package name"
            >
            <select
                class="rounded-lg border border-slate-700 bg-neutral-950 px-4 py-3 text-white outline-none focus:border-sky-400"
                name="verdict"
                aria-label="Verdict"
            >
                <option value="" selected=(verdict.is_none())>"All verdicts"</option>
                <option value="safe" selected=(verdict == Some("safe"))>"Safe"</option>
                <option value="suspicious" selected=(verdict == Some("suspicious"))>
                    "Suspicious"
                </option>
                <option value="dangerous" selected=(verdict == Some("dangerous"))>
                    "Dangerous"
                </option>
            </select>
            <button
                class="rounded-lg bg-sky-400 px-5 py-3 font-bold text-slate-950 hover:bg-sky-300"
                type="submit"
            >
                "Filter"
            </button>
        </form>
        <div class="grid gap-3">
            if checks.is_empty() {
                <p
                    class="rounded-xl border border-slate-800 bg-slate-900 p-5 text-slate-400"
                >
                    "No checks matched these filters."
                </p>
            } else {
                for check in checks {
                    check_card(check: &check)
                }
            }
        </div>
        <div class="mt-6 flex items-center justify-between gap-4">
            if page > 1 {
                <form method="get" action="/checks">
                    <input type="hidden" name="page" value=(page - 1)>
                    <input type="hidden" name="q" value=(search)>
                    <input type="hidden" name="verdict" value=(verdict.unwrap_or(""))>
                    <button
                        class="rounded-lg bg-sky-400 px-4 py-3 font-bold text-slate-950 hover:bg-sky-300"
                        type="submit"
                    >
                        "← Newer"
                    </button>
                </form>
            } else {
                <span></span>
            }
            <span class="text-sm text-slate-400">
                "Page "
                (page)
                " of "
                (pages)
            </span>
            if page < pages {
                <form method="get" action="/checks">
                    <input type="hidden" name="page" value=(page + 1)>
                    <input type="hidden" name="q" value=(search)>
                    <input type="hidden" name="verdict" value=(verdict.unwrap_or(""))>
                    <button
                        class="rounded-lg bg-sky-400 px-4 py-3 font-bold text-slate-950 hover:bg-sky-300"
                        type="submit"
                    >
                        "Older →"
                    </button>
                </form>
            } else {
                <span></span>
            }
        </div>
    }
}

#[page("/checks/{repo}")]
async fn repository_page(cx: &Cx) -> Result {
    let repo = path_param::<Repo>(cx);
    let history = database(cx).repository_checks(repo).await?;
    if history.is_empty() && !database(cx).current_package_base_exists(repo).await? {
        return Err(not_found().into());
    }

    view! {
        <p>
            <a class="text-sky-300 hover:text-sky-200" href="/checks">
                "← All checks"
            </a>
        </p>
        <h1 class="mt-5 text-4xl font-black tracking-tight sm:text-5xl">
            <a class="hover:text-sky-300" href=(aur_package_url(repo))>
                (repo)
                " ↗"
            </a>
        </h1>
        if let Some(latest) = history.first() {
            <h2 class="mb-4 mt-10 text-2xl font-bold">"Latest check"</h2>
            check_card(check: latest)
            <h2 class="mb-4 mt-10 text-2xl font-bold">"Check history"</h2>
            <div class="grid gap-3">
                for check in &history {
                    check_card(check: check)
                }
            </div>
        } else {
            <section class="mt-10 rounded-xl border border-slate-800 bg-slate-900 p-8">
                <h2 class="text-2xl font-bold">"No checks yet"</h2>
                <p class="mt-3 text-slate-400">
                    "This package has not been reviewed yet. Its checks will appear here after an assessment is completed."
                </p>
            </section>
        }
    }
}

#[page("/checks/{repo}/{commit}")]
async fn check_detail_page(cx: &Cx) -> Result {
    let repo = path_param::<Repo>(cx);
    let commit = path_param::<Commit>(cx);
    let query = query_params::<CheckDetailQuery>(cx)?;
    let show_pkgbuild = query.view.as_deref() == Some("pkgbuild");
    let Some(check) = database(cx).check_detail(repo, commit).await? else {
        debug!(repository = repo, commit, "check detail was not found");
        return Err(not_found().into());
    };
    let verdict_class = verdict_class(&check.verdict);
    let detail_url = format!("/checks/{repo}/{commit}");

    view! {
        <p>
            <a
                class="text-sky-300 hover:text-sky-200"
                href=(format!("/checks/{}", check.package_base))
            >
                "← "
                (check.package_base.as_str())
            </a>
        </p>
        <h1 class="mt-5 text-4xl font-black tracking-tight sm:text-5xl">
            (check.package_name.as_str())
            " "
            (check.version.as_str())
        </h1>
        <p class="mt-3 text-2xl font-extrabold capitalize">
            <span class=(verdict_class)>(check.verdict.as_str())</span>
        </p>
        if let Some(explanation) = &check.explanation {
            <div
                class="mt-4 max-w-3xl text-lg leading-8 text-slate-300 [&_a]:text-sky-300 [&_a]:underline [&_a]:underline-offset-2 hover:[&_a]:text-sky-200 [&_blockquote]:border-l-4 [&_blockquote]:border-slate-600 [&_blockquote]:pl-4 [&_code]:rounded [&_code]:bg-slate-800 [&_code]:px-1.5 [&_code]:py-0.5 [&_h1]:mt-5 [&_h1]:text-2xl [&_h1]:font-bold [&_h2]:mt-5 [&_h2]:text-xl [&_h2]:font-bold [&_h3]:mt-4 [&_h3]:font-bold [&_ol]:my-3 [&_ol]:list-decimal [&_ol]:pl-7 [&_p]:my-3 [&_pre]:my-4 [&_pre]:overflow-x-auto [&_pre]:rounded-lg [&_pre]:bg-slate-900 [&_pre]:p-4 [&_pre_code]:bg-transparent [&_pre_code]:p-0 [&_strong]:text-slate-100 [&_table]:my-4 [&_table]:w-full [&_table]:border-collapse [&_td]:border [&_td]:border-slate-700 [&_td]:p-2 [&_th]:border [&_th]:border-slate-700 [&_th]:p-2 [&_th]:text-left [&_ul]:my-3 [&_ul]:list-disc [&_ul]:pl-7"
            >
                (markdown_explanation(explanation))
            </div>
        }
        <div class="mt-8 grid gap-3 sm:grid-cols-2 lg:grid-cols-3">
            <div class="rounded-xl border border-slate-800 bg-slate-900 p-4">
                <span class="text-sm text-slate-400">"Checked"</span>
                <strong class="mt-1 block">(format_timestamp(check.checked_at))</strong>
            </div>
            <div class="rounded-xl border border-slate-800 bg-slate-900 p-4">
                <span class="text-sm text-slate-400">"Commit"</span>
                <strong class="mt-1 block break-all text-xs font-medium">
                    <a
                        class="text-sky-300 hover:text-sky-200"
                        href=(aur_commit_url(
                            &check.package_base,
                            &check.pkgbuild_commit,
                        ))
                    >
                        <code>(check.pkgbuild_commit.as_str())</code>
                        " ↗"
                    </a>
                </strong>
            </div>
            <div class="rounded-xl border border-slate-800 bg-slate-900 p-4">
                <span class="text-sm text-slate-400">"Check provider/model"</span>
                <strong class="mt-1 block">
                    (check.provider.as_str())
                    "/"
                    (check.model.as_str())
                </strong>
            </div>
        </div>
        <div class="mb-4 mt-10 flex items-center justify-between gap-4">
            if show_pkgbuild {
                <h2 class="text-2xl font-bold">"PKGBUILD"</h2>
                <div
                    class="flex rounded-lg border border-slate-700 bg-slate-900 p-1 text-sm"
                >
                    <a
                        class="rounded-md px-3 py-2 text-slate-400 hover:text-white"
                        href=(detail_url.as_str())
                    >
                        "Diff"
                    </a>
                    <a
                        class="rounded-md bg-sky-400 px-3 py-2 font-bold text-slate-950"
                        href=(format!("{detail_url}?view=pkgbuild"))
                    >
                        "PKGBUILD"
                    </a>
                </div>
            } else {
                <h2 class="text-2xl font-bold">"Commit diff"</h2>
                <div
                    class="flex rounded-lg border border-slate-700 bg-slate-900 p-1 text-sm"
                >
                    <a
                        class="rounded-md bg-sky-400 px-3 py-2 font-bold text-slate-950"
                        href=(detail_url.as_str())
                    >
                        "Diff"
                    </a>
                    <a
                        class="rounded-md px-3 py-2 text-slate-400 hover:text-white"
                        href=(format!("{detail_url}?view=pkgbuild"))
                    >
                        "PKGBUILD"
                    </a>
                </div>
            }
        </div>
        if show_pkgbuild {
            <pre
                class="overflow-x-auto rounded-xl border border-slate-800 bg-slate-900 text-sm leading-6 text-slate-200"
            >
                <code class="language-bash">(check.pkgbuild.as_str())</code>
            </pre>
        } else {
            <pre
                class="overflow-x-auto rounded-xl border border-slate-800 bg-slate-900 text-sm leading-6 text-slate-200"
            >
                <code class="language-diff">(check.commit_diff.as_str())</code>
            </pre>
        }
    }
}

#[cfg(test)]
mod tests {
    use super::markdown_explanation;

    fn render_markdown(markdown: &str) -> String {
        markdown_explanation(markdown).to_string()
    }

    #[test]
    fn renders_explanation_markdown() {
        let rendered = render_markdown("**Dangerous** because:\n\n- runs `curl`\n- changes files");

        assert!(rendered.contains("<strong>Dangerous</strong>"));
        assert!(rendered.contains("<ul>"));
        assert!(rendered.contains("<code>curl</code>"));
    }

    #[test]
    fn does_not_render_untrusted_html_or_urls() {
        let rendered = render_markdown("<script>alert(1)</script>\n\n[click](javascript:alert(1))");

        assert!(!rendered.contains("<script>"));
        assert!(rendered.contains("&lt;script&gt;alert(1)&lt;/script&gt;"));
        assert!(rendered.contains("href=\"#\""), "{rendered}");
        assert!(!rendered.contains("javascript:"));
    }
}
