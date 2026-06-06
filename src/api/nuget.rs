use axum::{Json, Router, extract::Path, response::{IntoResponse, Response}, routing::get};
use axum_extra::body::AsyncReadBody;

use crate::{
    error::ApiError,
    nuget::{NugetPackageVersionList, NugetV3Index, NugetV3Resource},
    state::ServerState,
};

pub fn router() -> Router<ServerState> {
    Router::new()
        .route("/nuget/v3/index.json", get(v3_index))
        .route("/nuget/v3/package/{id}/index.json", get(list_versions))
        .route(
            "/nuget/v3/package/{id}/{version}/{file}",
            get(package_version_file),
        )
        .route(
            "/nuget/v3/registration/{id}/index.json",
            get(registration_index),
        )
}

pub async fn v3_index(state: ServerState) -> Json<NugetV3Index> {
    Json(NugetV3Index {
        version: "3.0.0".into(),
        resources: vec![
            NugetV3Resource {
                id: format!("{}/nuget/v3/package/", state.base_url),
                res_type: "PackageBaseAddress/3.0.0".into(),
            },
            NugetV3Resource {
                id: format!("{}/nuget/v3/registration/", state.base_url),
                res_type: "RegistrationsBaseUrl".into(),
            },
            NugetV3Resource {
                id: format!("{}/nuget/v3/query", state.base_url),
                res_type: "SearchQueryService".into(),
            },
        ],
    })
}

pub async fn list_versions(
    state: ServerState,
    Path(id): Path<String>,
) -> Result<Json<NugetPackageVersionList>, ApiError> {
    let Some(versions) = state.nuget.unwrap().package_versions(&id).await else {
        return Err(ApiError::NotFound)
    };

    Ok(Json(versions))
}

pub async fn package_version_file(
    state: ServerState,
    Path((id, version, file)): Path<(String, String, String)>,
) -> Result<Response, ApiError> {
    // TODO: investigate if it's possible to compare a string against format without allocating a strng.

    if file == format!("{id}.nuspec") {
        let Some(manifest) = state.nuget.unwrap().package_manifest(&id, &version).await else {
            return Err(ApiError::NotFound)
        };

        Ok(manifest.into_response())
    } else if file == format!("{id}.{version}.nupkg") {
        match state.nuget.unwrap().package_contents(&id, &version).await {
            Some(file) => Ok(AsyncReadBody::new(file).into_response()),
            None => Err(ApiError::NotFound),
        }
    } else {
        Err(ApiError::NotFound)
    }
}

pub async fn registration_index(
    state: ServerState,
    Path(id): Path<String>,
) -> Result<String, ApiError> {
    if let Some(index) = state.nuget.unwrap().registration_index(&id).await {
        Ok(index)
    } else {
        Err(ApiError::NotFound)
    }
}
