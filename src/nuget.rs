use std::borrow::Cow;

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct NugetV3Resource {
    #[serde(rename = "@id")]
    pub id: String,

    #[serde(rename = "@type")]
    pub res_type: Cow<'static, str>,
}

#[derive(Serialize, Deserialize)]
pub struct NugetV3Index {
    pub version: Cow<'static, str>,
    pub resources: Vec<NugetV3Resource>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct NugetPackageVersionList {
    pub versions: Vec<String>
}