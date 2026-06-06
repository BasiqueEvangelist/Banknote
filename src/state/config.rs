use std::net::SocketAddr;

use knus::Decode;

#[derive(Decode)]
pub struct BanknoteConfig {
    #[knus(child, unwrap(argument))]
    pub bind_addr: String,
    #[knus(child, unwrap(argument))]
    pub base_url: String,

    #[knus(child)]
    pub cache: CacheConfig,
    #[knus(child)]
    pub repositories: RepositoriesConfig,
}

#[derive(Decode, Default)]
pub struct CacheConfig {
    #[knus(child, unwrap(argument))]
    pub path: String,
}

#[derive(Decode, Default)]
pub struct RepositoriesConfig {
    #[knus(child)]
    pub nuget: Option<NugetConfig>,
}

#[derive(Decode, Default)]
pub struct NugetConfig {
    #[knus(children(name="remote"))]
    pub remotes: Vec<NugetRemote>,
}

#[derive(Decode)]
pub struct NugetRemote {
    #[knus(argument)]
    pub index_url: String,

    #[knus(property)]
    pub name: Option<String>,

    #[knus(property)]
    pub proxy: Option<String>
}

impl NugetRemote {
    pub fn name(&self) -> &str {
        self.name.as_ref().unwrap_or_else(|| &self.index_url)
    }
}
