use std::env;

use aur_ai_security_db as db;
use aur_ai_security_protocol as protocol;
use topcoat::router::content::Json;
use topcoat::{
    context::Cx,
    router::headers,
    router::{
        error::{bad_request, unauthorized},
        route,
    },
    Result,
};

use crate::database;

fn authorize(cx: &Cx) -> Result<()> {
    let Some(expected) = env::var("AUR_SECURITY_API_TOKEN")
        .ok()
        .filter(|value| !value.is_empty())
    else {
        return Err(unauthorized().into());
    };
    let Some(actual) = headers(cx)
        .get("authorization")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
    else {
        return Err(unauthorized().into());
    };
    if !constant_time_eq(actual.as_bytes(), expected.as_bytes()) {
        return Err(unauthorized().into());
    }
    Ok(())
}

fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    let mut difference = left.len() ^ right.len();
    for index in 0..left.len().max(right.len()) {
        difference |= usize::from(left.get(index).copied().unwrap_or_default())
            ^ usize::from(right.get(index).copied().unwrap_or_default());
    }
    difference == 0
}

#[route(POST "/api/v1/agent/index/begin")]
async fn index_begin(
    cx: &Cx,
    Json(request): Json<protocol::IndexBeginRequest>,
) -> Result<Json<protocol::AcceptedResponse>> {
    authorize(cx)?;
    database(cx).begin_index(request.seen_at).await?;
    Ok(Json(protocol::AcceptedResponse { accepted: 0 }))
}

#[route(POST "/api/v1/agent/index/batch")]
async fn index_batch(
    cx: &Cx,
    Json(request): Json<protocol::IndexBatchRequest>,
) -> Result<Json<protocol::AcceptedResponse>> {
    authorize(cx)?;
    if request.packages.is_empty() || request.packages.len() > protocol::MAX_INDEX_BATCH {
        return Err(bad_request(format!(
            "packages must contain between 1 and {} entries",
            protocol::MAX_INDEX_BATCH
        ))
        .into());
    }
    let accepted = database(cx)
        .upsert_index_batch(request.seen_at, &request.packages)
        .await?;
    Ok(Json(protocol::AcceptedResponse { accepted }))
}

#[route(POST "/api/v1/agent/index/finish")]
async fn index_finish(
    cx: &Cx,
    Json(request): Json<protocol::IndexBeginRequest>,
) -> Result<Json<protocol::AcceptedResponse>> {
    authorize(cx)?;
    database(cx).finish_index(request.seen_at).await?;
    Ok(Json(protocol::AcceptedResponse { accepted: 1 }))
}

#[route(POST "/api/v1/agent/candidates")]
async fn candidates(
    cx: &Cx,
    Json(request): Json<protocol::CandidateRequest>,
) -> Result<Json<protocol::CandidateResponse>> {
    authorize(cx)?;
    if request.provider.is_empty() || request.model.is_empty() {
        return Err(bad_request("provider and model are required").into());
    }
    let packages = database(cx).candidates(&request).await?;
    Ok(Json(protocol::CandidateResponse { packages }))
}

#[route(POST "/api/v1/agent/checks")]
async fn checks(
    cx: &Cx,
    Json(request): Json<protocol::CheckBatchRequest>,
) -> Result<Json<protocol::AcceptedResponse>> {
    authorize(cx)?;
    if request.checks.is_empty() || request.checks.len() > protocol::MAX_CHECK_BATCH {
        return Err(bad_request(format!(
            "checks must contain between 1 and {} entries",
            protocol::MAX_CHECK_BATCH
        ))
        .into());
    }
    let accepted = database(cx).upsert_checks(&request.checks).await?;
    Ok(Json(protocol::AcceptedResponse { accepted }))
}

#[route(POST "/api/v1/agent/history/recent")]
async fn recent_checks(
    cx: &Cx,
    Json(request): Json<protocol::RecentChecksRequest>,
) -> Result<Json<(Vec<db::CheckSummary>, i64)>> {
    authorize(cx)?;
    Ok(Json(
        database(cx)
            .recent_checks(request.page, &request.search, request.verdict.as_deref())
            .await?,
    ))
}

#[route(POST "/api/v1/agent/history/repository")]
async fn repository_checks(
    cx: &Cx,
    Json(request): Json<protocol::RepositoryChecksRequest>,
) -> Result<Json<Vec<db::CheckSummary>>> {
    authorize(cx)?;
    Ok(Json(
        database(cx).repository_checks(&request.repository).await?,
    ))
}

#[route(POST "/api/v1/agent/history/detail")]
async fn check_detail(
    cx: &Cx,
    Json(request): Json<protocol::CheckDetailRequest>,
) -> Result<Json<Option<db::CheckDetail>>> {
    authorize(cx)?;
    Ok(Json(
        database(cx)
            .check_detail(&request.repository, &request.commit)
            .await?,
    ))
}

#[route(POST "/api/v1/agent/packages/search")]
async fn search_packages(
    cx: &Cx,
    Json(request): Json<protocol::SearchPackagesRequest>,
) -> Result<Json<Vec<db::PackageSearchResult>>> {
    authorize(cx)?;
    Ok(Json(database(cx).search_packages(&request.query).await?))
}

#[route(POST "/api/v1/agent/packages/exists")]
async fn package_base_exists(
    cx: &Cx,
    Json(request): Json<protocol::PackageBaseExistsRequest>,
) -> Result<Json<bool>> {
    authorize(cx)?;
    Ok(Json(
        database(cx)
            .current_package_base_exists(&request.package_base)
            .await?,
    ))
}
