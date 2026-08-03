use topcoat::{router::page, view::view, Result};

#[page("/")]
async fn home() -> Result {
    view! {
        <section class="max-w-3xl py-16">
            <p
                class="mb-3 text-sm font-semibold uppercase tracking-[0.2em] text-sky-300"
            >
                "Independent PKGBUILD review"
            </p>
            <h1 class="text-4xl font-black leading-tight tracking-tight sm:text-6xl">
                "Know what your AUR package is doing."
            </h1>
            <p class="mt-5 text-lg leading-8 text-slate-400">
                "Browse AI-assisted security checks, inspect risk assessments, and review the exact PKGBUILD changes behind them."
            </p>
            <form
                class="mt-8 flex flex-col gap-2 sm:flex-row"
                method="get"
                action="/search"
            >
                <input
                    class="min-w-0 flex-1 rounded-lg border border-slate-700 bg-slate-900 px-4 py-3 text-white outline-none placeholder:text-slate-500 focus:border-sky-400 focus:ring-2 focus:ring-sky-400/20"
                    name="q"
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
        </section>
        <section class="border-t border-slate-800 py-14">
            <p class="text-sm font-semibold uppercase tracking-[0.2em] text-sky-300">
                "How it works"
            </p>
            <h2 class="mt-3 text-3xl font-black tracking-tight sm:text-4xl">
                "Security reviews that show their work."
            </h2>
            <div class="mt-8 grid gap-4 md:grid-cols-3">
                <article class="rounded-xl border border-slate-800 bg-slate-900 p-6">
                    <span class="text-sm font-bold text-sky-300">"01 · Detect"</span>
                    <h3 class="mt-3 text-xl font-bold">"Surface risky changes"</h3>
                    <p class="mt-2 leading-7 text-slate-400">
                        "AI-assisted reviews screen AUR PKGBUILDs and repository changes for malware, suspicious behavior, and supply-chain risk."
                    </p>
                </article>
                <article class="rounded-xl border border-slate-800 bg-slate-900 p-6">
                    <span class="text-sm font-bold text-sky-300">"02 · Explain"</span>
                    <h3 class="mt-3 text-xl font-bold">"Show the work"</h3>
                    <p class="mt-2 leading-7 text-slate-400">
                        "Every verdict comes with a plain-language explanation and the exact commit, full PKGBUILD, and diff behind it."
                    </p>
                </article>
                <article class="rounded-xl border border-slate-800 bg-slate-900 p-6">
                    <span class="text-sm font-bold text-sky-300">"03 · Verify"</span>
                    <h3 class="mt-3 text-xl font-bold">"Make the final call"</h3>
                    <p class="mt-2 leading-7 text-slate-400">
                        "Search checks, inspect the evidence, and decide for yourself whether a package change is safe to build and install."
                    </p>
                </article>
            </div>
        </section>
        <section
            class="grid gap-6 border-t border-slate-800 py-14 md:grid-cols-[minmax(0,1fr)_auto] md:items-center"
        >
            <div class="max-w-3xl">
                <p
                    class="text-sm font-semibold uppercase tracking-[0.2em] text-sky-300"
                >
                    "Experimental paru integration"
                </p>
                <h2 class="mt-3 text-3xl font-black tracking-tight">
                    "Review AUR changes before the build starts."
                </h2>
                <p class="mt-3 text-lg leading-8 text-slate-400">
                    "The paru integration checks commits since your accepted baseline against the hosted service, then locally assesses any uncovered range. Safe packages can skip manual review; anything else stays in paru's normal review flow."
                </p>
            </div>
            <a
                class="w-fit rounded-lg border border-slate-700 px-5 py-3 font-bold text-white hover:border-slate-500 hover:bg-slate-900"
                href="https://github.com/Cretezy/aur-security#paru-integration"
            >
                "Read the setup on GitHub ↗"
            </a>
        </section>
        <section
            class="grid gap-8 border-t border-slate-800 py-14 md:grid-cols-[minmax(0,1fr)_auto] md:items-center"
        >
            <div class="max-w-3xl">
                <h2 class="text-3xl font-black tracking-tight">
                    "A review aid, not a trust badge."
                </h2>
                <p class="mt-3 text-lg leading-8 text-slate-400">
                    "AI assessments can miss malicious behavior or flag legitimate packaging patterns. Use the stored source and commit history to verify a package before installing it."
                </p>
            </div>
            <div class="flex flex-wrap gap-3">
                <a
                    class="rounded-lg bg-sky-400 px-5 py-3 font-bold text-slate-950 hover:bg-sky-300"
                    href="/checks"
                >
                    "Browse checks"
                </a>
                <a
                    class="rounded-lg border border-slate-700 px-5 py-3 font-bold text-white hover:border-slate-500 hover:bg-slate-900"
                    href="https://github.com/Cretezy/aur-security"
                >
                    "View on GitHub ↗"
                </a>
            </div>
        </section>
    }
}
