use std::{
    env, io,
    path::PathBuf,
    time::Duration,
};

use anyhow::Context;
use axum::body::Bytes;
use futures_util::TryStreamExt;
use moka::future::CacheBuilder;
use reqwest::StatusCode;
use tokio::io::AsyncRead;
use tokio_util::io::StreamReader;

use crate::nuget::{NugetPackageVersionList, NugetV3Index};

pub struct NugetState {
    pub remote_package_base_address: String,
    pub remote_registrations_base_address: String,

    data_path: &'static str,

    versions_cache: moka::future::Cache<String, Option<NugetPackageVersionList>>,
    manifest_cache: moka::future::Cache<(String, String), Option<Bytes>>,
    contents_path_cache: moka::future::Cache<(String, String), Option<String>>,
}

impl NugetState {
    async fn package_versions_inner(
        &self,
        package_id: &str,
    ) -> anyhow::Result<Option<NugetPackageVersionList>> {
        let req = reqwest::get(format!(
            "{}{}/index.json",
            &self.remote_package_base_address, package_id
        ))
        .await?;

        if req.status() == StatusCode::NOT_FOUND {
            return Ok(None);
        }

        Ok(Some(req.json().await?))
    }

    pub async fn package_versions(&self, package_id: &str) -> Option<NugetPackageVersionList> {
        self.versions_cache
            .get_with_by_ref(package_id, async {
                self.package_versions_inner(package_id)
                    .await
                    .unwrap_or_else(|x| {
                        tracing::error!(
                            "Could not load package versions of package {package_id}: {x}"
                        );
                        None
                    })
            })
            .await
    }

    async fn package_manifest_inner(
        &self,
        package_id: &str,
        version: &str,
    ) -> anyhow::Result<Option<Bytes>> {
        let req = reqwest::get(format!(
            "{}{package_id}/{version}/{package_id}.nuspec",
            &self.remote_package_base_address
        ))
        .await?;

        if req.status() == StatusCode::NOT_FOUND {
            return Ok(None);
        }

        Ok(Some(req.bytes().await?))
    }

    pub async fn package_manifest(&self, package_id: &str, version: &str) -> Option<Bytes> {
        self.manifest_cache
            // TODO: this is a needless allocation. fix this.
            .get_with_by_ref(&(package_id.to_string(), version.to_string()), async {
                self.package_manifest_inner(package_id, version).await.unwrap_or_else(|x| {
                    tracing::error!("Could not load package manifest for version {version} of package {package_id}: {x}");
                    None
                })
            })
            .await
    }

    async fn load_package(
        &self,
        package_id: &str,
        version: &str,
    ) -> anyhow::Result<Option<String>> {
        let mut path = PathBuf::from(self.data_path);
        path.push(format!("{package_id}.{version}.nupkg"));

        if let Ok(true) = tokio::fs::try_exists(&path).await {
            return Ok(Some(format!("{package_id}.{version}.nupkg")));
        }

        let res = reqwest::get(format!(
            "{}{package_id}/{version}/{package_id}.{version}.nupkg",
            &self.remote_package_base_address
        ))
        .await?;

        if res.status() == StatusCode::NOT_FOUND {
            return Ok(None);
        }

        tracing::info!(package_id, version, "downloading package");

        let mut file = tokio::fs::File::create(&path).await?;

        tokio::io::copy(
            &mut StreamReader::new(
                res.bytes_stream()
                    .map_err(|e| io::Error::new(io::ErrorKind::Other, e)),
            ),
            &mut file,
        )
        .await?;

        return Ok(Some(format!("{package_id}.{version}.nupkg")));
    }

    pub async fn package_contents(
        &self,
        package_id: &str,
        version: &str,
    ) -> Option<impl AsyncRead + 'static> {
        let mut path = PathBuf::from(self.data_path);
        path.push(format!("{package_id}.{version}.nupkg"));

        if let Ok(file) = tokio::fs::File::open(&path).await {
            return Some(file);
        }

        let opath = self.contents_path_cache
            .get_with_by_ref(&(package_id.to_string(), version.to_string()), async {
                self.load_package(package_id, version).await.unwrap_or_else(|x| {
                    tracing::error!("Could not load package manifest for version {version} of package {package_id}: {x}");
                    None
                })
            })
            .await;

        if let Some(_) = opath
            && let Ok(file) = tokio::fs::File::open(path).await
        {
            return Some(file);
        }

        None
    }
}

pub async fn maybe_init(data_path: &'static str) -> anyhow::Result<Option<NugetState>> {
    let remote_index_url =
        env::var("NUGET_REMOTE").with_context(|| "reading NUGET_REMOTE variable")?;

    let index: NugetV3Index = reqwest::get(remote_index_url)
        .await
        .with_context(|| "requesting NuGet remote's index")?
        .json()
        .await
        .with_context(|| "parsing NuGet remote's index")?;

    let Some(remote_package_base_address) = index
        .resources
        .iter()
        .filter(|x| x.res_type == "PackageBaseAddress/3.0.0")
        .map(|x| x.id.clone())
        .next()
    else {
        anyhow::bail!("couldn't find PackageBaseAddress in remote NuGet index");
    };

    let Some(remote_registrations_base_address) = index
        .resources
        .iter()
        .filter(|x| x.res_type == "RegistrationsBaseUrl")
        .map(|x| x.id.clone())
        .next()
    else {
        anyhow::bail!("couldn't find PackageBaseAddress in remote NuGet index");
    };

    Ok(Some(NugetState {
        remote_package_base_address,
        remote_registrations_base_address,
        data_path,
        versions_cache: CacheBuilder::new(1000)
            .time_to_live(Duration::from_mins(5))
            .build(),
        manifest_cache: CacheBuilder::new(500)
            .time_to_idle(Duration::from_hours(1))
            .build(),
        contents_path_cache: CacheBuilder::new(100)
            .time_to_idle(Duration::from_hours(1))
            .build(),
    }))
}
