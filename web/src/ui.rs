use aur_security_db as db;
use chrono::{DateTime, Utc};
use topcoat::{view::view, Result};

#[topcoat::view::component]
pub(crate) async fn check_card(check: &db::CheckSummary) -> Result {
    let href = format!("/checks/{}/{}", check.package_base, check.pkgbuild_commit);
    let verdict_class = verdict_class(&check.verdict);
    view! {
        <article
            class="grid gap-4 rounded-xl border border-slate-800 bg-slate-900 p-5 transition hover:border-slate-700 sm:grid-cols-[minmax(0,1fr)_auto]"
        >
            <div>
                <strong>
                    <a class="text-sky-300 hover:text-sky-200" href=(href)>
                        (check.package_name.as_str())
                        " "
                        (check.version.as_str())
                    </a>
                </strong>
                <div class="mt-2 flex flex-wrap gap-x-5 gap-y-1 text-sm text-slate-400">
                    <span>(format_timestamp(check.checked_at))</span>
                    <a
                        class="text-sky-300 hover:text-sky-200"
                        href=(aur_commit_url(
                            &check.package_base,
                            &check.pkgbuild_commit,
                        ))
                    >
                        <code>(short_commit(&check.pkgbuild_commit))</code>
                        " ↗"
                    </a>
                    <a
                        class="text-sky-300 hover:text-sky-200"
                        href=(aur_package_url(&check.package_name))
                    >
                        "AUR package ↗"
                    </a>
                </div>
            </div>
            <div class="flex flex-col items-start sm:items-end">
                <strong class=(verdict_class)>(check.verdict.as_str())</strong>
            </div>
        </article>
    }
}

pub(crate) fn verdict_class(verdict: &str) -> &'static str {
    match verdict {
        "safe" => "text-xl font-extrabold capitalize text-emerald-400",
        "suspicious" => "text-xl font-extrabold capitalize text-amber-400",
        "dangerous" => "text-xl font-extrabold capitalize text-rose-400",
        _ => "text-xl font-extrabold capitalize text-slate-300",
    }
}

fn short_commit(commit: &str) -> &str {
    commit.get(..commit.len().min(10)).unwrap_or(commit)
}

pub(crate) fn aur_package_url(package: &str) -> String {
    format!("https://aur.archlinux.org/packages/{package}")
}

pub(crate) fn aur_commit_url(package_base: &str, commit: &str) -> String {
    format!("https://aur.archlinux.org/cgit/aur.git/commit/?h={package_base}&id={commit}")
}

pub(crate) fn format_timestamp(timestamp: i64) -> String {
    DateTime::<Utc>::from_timestamp(timestamp, 0)
        .map(|date| date.format("%Y-%m-%d %H:%M UTC").to_string())
        .unwrap_or_else(|| timestamp.to_string())
}
