use std::{env, io, path::PathBuf, time::Duration};

use anyhow::Context;
use axum::body::Bytes;
use futures_util::TryStreamExt;
use moka::future::CacheBuilder;
use reqwest::{ClientBuilder, Proxy, StatusCode};
use tokio::io::AsyncRead;
use tokio_util::io::StreamReader;

use crate::nuget::{NugetPackageVersionList, NugetV3Index};

struct NugetRemote {
    name: String,
    remote_package_base_address: String,
    remote_registrations_base_address: String,

    client: reqwest::Client,
}

impl NugetRemote {
    async fn package_versions(
        &self,
        package_id: &str,
    ) -> anyhow::Result<Option<NugetPackageVersionList>> {
        let req = self
            .client
            .get(format!(
                "{}{}/index.json",
                &self.remote_package_base_address, package_id
            ))
            .send()
            .await?;

        if req.status() == StatusCode::NOT_FOUND {
            return Ok(None);
        }

        Ok(Some(req.json().await?))
    }

    async fn registration_index(&self, package_id: &str) -> anyhow::Result<Option<String>> {
        let res = self
            .client
            .get(format!(
                "{}{}/index.json",
                self.remote_registrations_base_address, package_id
            ))
            .send()
            .await?;

        if res.status() == StatusCode::NOT_FOUND {
            return Ok(None);
        }

        Ok(Some(res.text().await?))
    }

    async fn package_manifest(
        &self,
        package_id: &str,
        version: &str,
    ) -> anyhow::Result<Option<Bytes>> {
        let req = self
            .client
            .get(format!(
                "{}{package_id}/{version}/{package_id}.nuspec",
                &self.remote_package_base_address
            ))
            .send()
            .await?;

        if req.status() == StatusCode::NOT_FOUND {
            return Ok(None);
        }

        Ok(Some(req.bytes().await?))
    }

    async fn load_package(
        &self,
        package_id: &str,
        version: &str,
    ) -> anyhow::Result<Option<impl AsyncRead>> {
        let res = self
            .client
            .get(format!(
                "{}{package_id}/{version}/{package_id}.{version}.nupkg",
                &self.remote_package_base_address
            ))
            .send()
            .await?;

        if res.status() == StatusCode::NOT_FOUND {
            return Ok(None);
        }

        tracing::info!(
            package_id,
            version,
            remote_name = &self.name,
            "downloading package"
        );

        Ok(Some(StreamReader::new(
            res.bytes_stream()
                .map_err(|e| io::Error::new(io::ErrorKind::Other, e)),
        )))
    }
}

pub struct NugetState {
    remotes: Vec<NugetRemote>,

    cache_path: &'static str,

    versions_cache: moka::future::Cache<String, Option<NugetPackageVersionList>>,
    index_cache: moka::future::Cache<String, Option<String>>,
    manifest_cache: moka::future::Cache<(String, String), Option<Bytes>>,
    contents_path_cache: moka::future::Cache<(String, String), Option<String>>,
}

impl NugetState {
    // reqwest::get(format!(
    //     "{}{}/index.json",
    //     state.nuget.unwrap().remote_registrations_base_address,
    //     id
    // ))
    // .await?
    // .text()
    // .await?

    async fn registration_index_inner(&self, package_id: &str) -> anyhow::Result<Option<String>> {
        for remote in &self.remotes {
            if let Some(index) = remote.registration_index(package_id).await? {
                return Ok(Some(index));
            }
        }

        Ok(None)
    }

    pub async fn registration_index(&self, package_id: &str) -> Option<String> {
        self.index_cache
            .get_with_by_ref(package_id, async {
                self.registration_index_inner(package_id)
                    .await
                    .unwrap_or_else(|x| {
                        tracing::error!(
                            "Could not load registration index for package {package_id}: {x}"
                        );
                        None
                    })
            })
            .await
    }

    async fn package_versions_inner(
        &self,
        package_id: &str,
    ) -> anyhow::Result<Option<NugetPackageVersionList>> {
        for remote in &self.remotes {
            if let Some(versions) = remote.package_versions(package_id).await? {
                return Ok(Some(versions));
            }
        }

        Ok(None)
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
        for remote in &self.remotes {
            if let Some(manifest) = remote.package_manifest(package_id, version).await? {
                return Ok(Some(manifest));
            }
        }

        Ok(None)
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
        let mut path = PathBuf::from(self.cache_path);
        path.push(format!("{package_id}.{version}.nupkg"));

        if let Ok(true) = tokio::fs::try_exists(&path).await {
            return Ok(Some(format!("{package_id}.{version}.nupkg")));
        }

        let mut read = 'outer: loop {
            for remote in &self.remotes {
                if let Some(read) = remote.load_package(package_id, version).await? {
                    break 'outer read;
                }
            }

            return Ok(None);
        };

        let mut file = tokio::fs::File::create(&path).await?;

        tokio::io::copy(&mut read, &mut file).await?;

        return Ok(Some(format!("{package_id}.{version}.nupkg")));
    }

    pub async fn package_contents(
        &self,
        package_id: &str,
        version: &str,
    ) -> Option<impl AsyncRead + 'static> {
        let mut path = PathBuf::from(self.cache_path);
        path.push(format!("{package_id}.{version}.nupkg"));

        if let Ok(file) = tokio::fs::File::open(&path).await {
            return Some(file);
        }

        let opath = self.contents_path_cache
            .get_with_by_ref(&(package_id.to_string(), version.to_string()), async {
                self.load_package(package_id, version).await.unwrap_or_else(|x| {
                    tracing::error!("Could not load package contents for version {version} of package {package_id}: {x}");
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

async fn init_remote(config: &super::config::NugetRemote) -> anyhow::Result<NugetRemote> {
    let client = {
        let mut builder = ClientBuilder::new();

        if let Some(proxy) = &config.proxy {
            builder = builder
                .proxy(Proxy::all(proxy).with_context(|| format!("setting up proxy {proxy}"))?);
        }

        builder.build().with_context(|| "creating http client")?
    };

    let index: NugetV3Index = client
        .get(&config.index_url)
        .send()
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

    Ok(NugetRemote {
        name: config.name().to_string(),
        client,
        remote_package_base_address,
        remote_registrations_base_address,
    })
}

pub async fn init(
    cache_path: &'static str,
    config: &super::config::NugetConfig,
) -> anyhow::Result<NugetState> {
    let mut remotes = vec![];

    for remote in &config.remotes {
        remotes.push(
            init_remote(&remote)
                .await
                .with_context(|| format!("loading data for remote {}", remote.name()))?,
        );
    }

    Ok(NugetState {
        remotes,
        cache_path,
        versions_cache: CacheBuilder::new(1000)
            .time_to_live(Duration::from_mins(5))
            .build(),
        index_cache: CacheBuilder::new(1000)
            .time_to_live(Duration::from_mins(5))
            .build(),
        manifest_cache: CacheBuilder::new(500)
            .time_to_idle(Duration::from_hours(1))
            .build(),
        contents_path_cache: CacheBuilder::new(100)
            .time_to_idle(Duration::from_hours(1))
            .build(),
    })
}
