//! Typed BIS SDMX-JSON discovery; unknown metadata remains in `extra` maps.
use crate::{Client, Error, Query, Result};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::Value;
use std::collections::BTreeMap;

type Extra = BTreeMap<String, Value>;
const ACCEPT: &str = "application/vnd.sdmx.structure+json;version=1.0.0";

/// An artefact selector. `all`, `latest`, and `+` selections are supported.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StructureRef {
    #[serde(rename = "agencyID")]
    pub agency_id: String,
    pub id: String,
    pub version: String,
}

impl StructureRef {
    pub fn new(
        agency: impl Into<String>,
        id: impl Into<String>,
        version: impl Into<String>,
    ) -> Self {
        Self {
            agency_id: agency.into(),
            id: id.into(),
            version: version.into(),
        }
    }

    /// Resolve a dataflow's DSD or a component's codelist URN.
    pub fn from_urn(urn: &str) -> Result<Self> {
        let invalid = || Error::InvalidQuery(format!("invalid artefact URN: {urn:?}"));
        let (kind, reference) = urn.split_once('=').ok_or_else(invalid)?;
        if !kind.starts_with("urn:sdmx:org.sdmx.infomodel.") {
            return Err(invalid());
        }
        let (agency, resource) = reference.split_once(':').ok_or_else(invalid)?;
        let (id, version) = resource.split_once('(').ok_or_else(invalid)?;
        let version = version.strip_suffix(')').ok_or_else(invalid)?;
        let result = Self::new(agency, id, version);
        result.validate()?;
        Ok(result)
    }

    fn validate(&self) -> Result<()> {
        if !safe_selector(&self.agency_id)
            || !safe_selector(&self.id)
            || !(matches!(self.version.as_str(), "all" | "latest")
                || self.version.split('+').all(|v| {
                    v.split('.')
                        .all(|n| !n.is_empty() && n.bytes().all(|c| c.is_ascii_digit()))
                }))
        {
            return Err(Error::InvalidQuery("invalid artefact selector".into()));
        }
        Ok(())
    }
}

fn safe_selector(value: &str) -> bool {
    value.len() <= 2048
        && value.split('+').all(|part| {
            !part.is_empty()
                && part != "."
                && part != ".."
                && part
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || b"_.@$-".contains(&c))
        })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Dataflow {
    pub id: String,
    pub name: String,
    #[serde(rename = "agencyID")]
    pub agency_id: String,
    pub version: String,
    /// DataStructure URN, resolvable using StructureRef::from_urn.
    pub structure: String,
    #[serde(flatten)]
    pub extra: Extra,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DataStructure {
    pub id: String,
    pub name: String,
    #[serde(rename = "agencyID")]
    pub agency_id: String,
    pub version: String,
    pub data_structure_components: StructureComponents,
    #[serde(flatten)]
    pub extra: Extra,
}

impl DataStructure {
    /// Positional series dimensions, sorted by position; excludes time dimensions.
    pub fn dimensions(&self) -> Vec<&Dimension> {
        let mut dimensions: Vec<_> = self
            .data_structure_components
            .dimension_list
            .dimensions
            .iter()
            .collect();
        dimensions.sort_by_key(|d| d.position);
        dimensions
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StructureComponents {
    pub dimension_list: DimensionList,
    #[serde(default)]
    pub attribute_list: AttributeList,
    /// Includes measures, groups and other unmodelled SDMX components.
    #[serde(flatten)]
    pub extra: Extra,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DimensionList {
    pub dimensions: Vec<Dimension>,
    #[serde(default)]
    pub time_dimensions: Vec<Dimension>,
    #[serde(flatten)]
    pub extra: Extra,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Dimension {
    pub id: String,
    pub position: u32,
    pub local_representation: Option<Representation>,
    #[serde(flatten)]
    pub extra: Extra,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AttributeList {
    #[serde(default)]
    pub attributes: Vec<Attribute>,
    #[serde(flatten)]
    pub extra: Extra,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Attribute {
    pub id: String,
    pub local_representation: Option<Representation>,
    /// Includes assignmentStatus, attributeRelationship and conceptIdentity.
    #[serde(flatten)]
    pub extra: Extra,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Representation {
    /// Codelist URN, when the component is enumerated.
    pub enumeration: Option<String>,
    #[serde(flatten)]
    pub extra: Extra,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CodeList {
    pub id: String,
    pub name: String,
    #[serde(rename = "agencyID")]
    pub agency_id: String,
    pub version: String,
    pub codes: Vec<Code>,
    #[serde(flatten)]
    pub extra: Extra,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Code {
    pub id: String,
    pub name: String,
    /// Includes multilingual names, annotations and parent references.
    #[serde(flatten)]
    pub extra: Extra,
}

#[derive(Debug, Clone, Copy, Default)]
pub enum AvailabilityMode {
    #[default]
    Exact,
    Available,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContentConstraint {
    pub id: String,
    #[serde(default)]
    pub cube_regions: Vec<CubeRegion>,
    #[serde(flatten)]
    pub extra: Extra,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CubeRegion {
    pub is_included: bool,
    #[serde(default)]
    pub key_values: Vec<KeyValue>,
    /// Includes time ranges and other constraint forms.
    #[serde(flatten)]
    pub extra: Extra,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyValue {
    pub id: String,
    pub values: Vec<Value>,
    #[serde(flatten)]
    pub extra: Extra,
}

impl Client {
    /// Discover the latest BIS dataflows.
    pub async fn dataflows(&self) -> Result<Vec<Dataflow>> {
        self.dataflow(&StructureRef::new("BIS", "all", "latest"))
            .await
    }

    pub async fn dataflow(&self, id: &StructureRef) -> Result<Vec<Dataflow>> {
        self.structure_request("dataflow", id, "dataflows").await
    }

    pub async fn datastructure(&self, id: &StructureRef) -> Result<Vec<DataStructure>> {
        let structures: Vec<DataStructure> = self
            .structure_request("datastructure", id, "dataStructures")
            .await?;
        for structure in &structures {
            let mut positions = std::collections::BTreeSet::new();
            let mut ids = std::collections::BTreeSet::new();
            for dimension in structure
                .data_structure_components
                .dimension_list
                .dimensions
                .iter()
                .chain(
                    &structure
                        .data_structure_components
                        .dimension_list
                        .time_dimensions,
                )
            {
                if dimension.position == 0
                    || !positions.insert(dimension.position)
                    || !ids.insert(&dimension.id)
                {
                    return Err(Error::InvalidResponse(
                        "invalid or duplicate dimension position/id".into(),
                    ));
                }
            }
        }
        Ok(structures)
    }

    pub async fn codelist(&self, id: &StructureRef) -> Result<Vec<CodeList>> {
        self.structure_request("codelist", id, "codelists").await
    }

    /// Query available selections. Observation count limits are not applicable.
    pub async fn availability(
        &self,
        query: &Query,
        component: &str,
        mode: AvailabilityMode,
    ) -> Result<Vec<ContentConstraint>> {
        if !safe_selector(component)
            || component.contains(['.', '+'])
            || query.first_n_observations.is_some()
            || query.last_n_observations.is_some()
        {
            return Err(Error::InvalidQuery(
                "invalid component or observation count on availability query".into(),
            ));
        }
        let mut url = query.url_on(self.base_url())?;
        url.set_path(&url.path().replacen("/data/", "/availableconstraint/", 1));
        url.path_segments_mut()
            .map_err(|_| Error::InvalidQuery("invalid base URL".into()))?
            .push(component);
        url.set_query(None);
        {
            let mut parameters = url.query_pairs_mut();
            parameters.append_pair(
                "mode",
                match mode {
                    AvailabilityMode::Exact => "exact",
                    AvailabilityMode::Available => "available",
                },
            );
            parameters.append_pair("references", "none");
            if let Some(period) = &query.start_period {
                parameters.append_pair("startPeriod", period);
            }
            if let Some(period) = &query.end_period {
                parameters.append_pair("endPeriod", period);
            }
        }
        self.metadata_request(url, "contentConstraints").await
    }

    async fn structure_request<T: DeserializeOwned>(
        &self,
        kind: &str,
        id: &StructureRef,
        collection: &str,
    ) -> Result<Vec<T>> {
        id.validate()?;
        let mut url =
            reqwest::Url::parse(self.base_url()).map_err(|e| Error::InvalidQuery(e.to_string()))?;
        url.path_segments_mut()
            .map_err(|_| Error::InvalidQuery("invalid base URL".into()))?
            .pop_if_empty()
            .extend([kind, &id.agency_id, &id.id, &id.version]);
        url.query_pairs_mut().append_pair("references", "none");
        self.metadata_request(url, collection).await
    }

    async fn metadata_request<T: DeserializeOwned>(
        &self,
        url: reqwest::Url,
        collection: &str,
    ) -> Result<Vec<T>> {
        let (outcome, _) = self.request(url, ACCEPT).await?;
        crate::client::ensure_content_type(
            outcome
                .headers
                .get(reqwest::header::CONTENT_TYPE)
                .and_then(|v| v.to_str().ok()),
            &["application/json", "application/vnd.sdmx.structure+json"],
        )?;
        let mut envelope: Value = serde_json::from_slice(&outcome.body)?;
        let values = envelope
            .get_mut("data")
            .and_then(|data| data.get_mut(collection))
            .ok_or_else(|| Error::InvalidResponse(format!("missing data.{collection}")))?;
        Ok(serde_json::from_value(values.take())?)
    }
}
