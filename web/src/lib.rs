#[cfg(target_arch = "wasm32")]
#[path = "worker_agent_api.rs"]
mod agent_api;
#[cfg(target_arch = "wasm32")]
#[path = "d1.rs"]
mod d1;

#[cfg(target_arch = "wasm32")]
mod api;
#[cfg(target_arch = "wasm32")]
mod layout;
#[cfg(target_arch = "wasm32")]
mod pages;
#[cfg(target_arch = "wasm32")]
mod ui;

#[cfg(target_arch = "wasm32")]
use aur_ai_security_db as db;
#[cfg(target_arch = "wasm32")]
use topcoat::{
    asset::{AssetConfig, Manifest, RouterBuilderAssetExt},
    context::{app_context, Cx},
    router::{Body, Router, RouterBuilderDiscoverExt},
};
#[cfg(target_arch = "wasm32")]
use worker::{Context, Env, HttpRequest};

#[cfg(target_arch = "wasm32")]
use crate::d1::D1Backend;

#[cfg(target_arch = "wasm32")]
#[derive(Clone, Debug)]
pub(crate) struct ApiToken(pub(crate) Option<String>);

#[cfg(target_arch = "wasm32")]
fn router(database: D1Backend, token: ApiToken) -> Router {
    let manifest = Manifest::parse(include_str!("../static/_topcoat/assets/manifest.toml"))
        .expect("invalid Topcoat asset manifest");
    Router::builder()
        .discover()
        .assets(AssetConfig::hosted_at("/_topcoat/assets", manifest))
        .app_context(database)
        .app_context(token)
        .build()
}

#[cfg(target_arch = "wasm32")]
pub(crate) fn database(cx: &Cx) -> &dyn db::Database {
    app_context::<D1Backend>(cx)
}

#[cfg(target_arch = "wasm32")]
pub(crate) fn api_token(cx: &Cx) -> Option<&str> {
    app_context::<ApiToken>(cx).0.as_deref()
}

#[cfg(target_arch = "wasm32")]
#[worker::event(fetch)]
async fn fetch(
    request: HttpRequest,
    env: Env,
    _ctx: Context,
) -> worker::Result<http::Response<Body>> {
    let database = D1Backend::new(env.d1("aur_security")?);
    let token = env
        .secret("AUR_SECURITY_API_TOKEN")
        .ok()
        .map(|secret| secret.to_string());
    Ok(router(database, ApiToken(token))
        .handle(request.map(Body::new))
        .await)
}
