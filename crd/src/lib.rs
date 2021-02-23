mod error;

pub use crate::error::CrdError;
use derivative::Derivative;
use kube::CustomResource;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use stackable_operator::Crd;
use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::fmt;
use std::hash::{Hash, Hasher};
use std::str::FromStr;

#[derive(Clone, CustomResource, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize)]
#[kube(
    group = "spark.stackable.tech",
    version = "v1",
    kind = "SparkCluster",
    shortname = "sc",
    namespaced
)]
#[kube(status = "SparkClusterStatus")]
pub struct SparkClusterSpec {
    pub master: SparkNode,
    pub worker: SparkNode,
    pub history_server: Option<SparkNode>,
    pub version: SparkVersion,
    pub secret: Option<String>,
    pub log_dir: Option<String>,
}

impl SparkClusterSpec {
    /// collect hashed selectors from master, worker and history server
    pub fn get_hashed_selectors(
        &self,
        cluster_name: &str,
    ) -> HashMap<SparkNodeType, HashMap<String, SparkNodeSelector>> {
        let mut hashed_selectors: HashMap<SparkNodeType, HashMap<String, SparkNodeSelector>> =
            HashMap::new();

        hashed_selectors.insert(
            SparkNodeType::Master,
            self.master
                .get_hashed_selectors(SparkNodeType::Master, cluster_name),
        );

        hashed_selectors.insert(
            SparkNodeType::Worker,
            self.worker
                .get_hashed_selectors(SparkNodeType::Worker, cluster_name),
        );

        if let Some(history_server) = &self.history_server {
            hashed_selectors.insert(
                SparkNodeType::HistoryServer,
                history_server.get_hashed_selectors(SparkNodeType::HistoryServer, cluster_name),
            );
        }

        hashed_selectors
    }
}

#[derive(Clone, Debug, Hash, Deserialize, Eq, JsonSchema, PartialEq, Serialize)]
pub struct SparkNode {
    pub selectors: Vec<SparkNodeSelector>,
    // TODO: options
    // TODO: master -> use Option<T>
    // TODO: worker -> use Option<T>
    // TODO: history_server -> use Option<T>
}

#[derive(Derivative, Clone, Debug, Deserialize, Eq, JsonSchema, Serialize)]
#[derivative(Hash, PartialEq)]
pub struct SparkNodeSelector {
    // common options
    pub node_name: String,
    // we need to ignore instances from hashing -> if we scale up or down, the hash
    // for the selectors changes and the operator removes all nodes and rebuilds them
    #[derivative(Hash = "ignore", PartialEq = "ignore")]
    pub instances: usize,
    pub config: Option<Vec<ConfigOption>>,
    pub env: Option<Vec<ConfigOption>>,

    // master options
    pub master_port: Option<usize>,
    pub master_web_ui_port: Option<usize>,
    // worker options
    pub cores: Option<usize>,
    pub memory: Option<String>,
    pub worker_port: Option<usize>,
    pub worker_web_ui_port: Option<usize>,
    // history-server options
    pub store_path: Option<String>,
    pub history_ui_port: Option<usize>,
}

impl SparkNode {
    /// Collects all selectors provided in a node (master, worker, history-server) and hashes them
    ///
    /// # Arguments
    /// * `node_type` - SparkNodeType (master/worker/history-server)
    /// * `cluster_name` - Name of the cluster they belong to (otherwise different named clusters can have identical selector hashes)
    ///
    pub fn get_hashed_selectors(
        &self,
        node_type: SparkNodeType,
        cluster_name: &str,
    ) -> HashMap<String, SparkNodeSelector> {
        let mut hashed_selectors: HashMap<String, SparkNodeSelector> = HashMap::new();
        for selector in &self.selectors {
            let mut hasher = DefaultHasher::new();
            selector.hash(&mut hasher);
            node_type.as_str().hash(&mut hasher);
            cluster_name.hash(&mut hasher);
            hashed_selectors.insert(hasher.finish().to_string(), selector.clone());
        }
        hashed_selectors
    }

    /// Returns the sum of all requested instance counts across all selectors.
    pub fn get_instances(&self) -> usize {
        let mut instances: usize = 0;
        for selector in &self.selectors {
            instances += selector.instances;
        }
        instances
    }
}

/// A spark node consists of a list of selectors and optional common properties that is shared for every node
#[derive(Clone, Debug, Hash, Deserialize, Eq, JsonSchema, PartialEq, Serialize)]
pub enum SparkNodeType {
    Master,
    Worker,
    HistoryServer,
}

const MASTER: &str = "master";
const WORKER: &str = "slave";
const HISTORY_SERVER: &str = "history-server";

impl SparkNodeType {
    pub fn as_str(&self) -> &'static str {
        match self {
            SparkNodeType::Master { .. } => MASTER,
            SparkNodeType::Worker { .. } => WORKER,
            SparkNodeType::HistoryServer { .. } => HISTORY_SERVER,
        }
    }

    /// Returns the container start command for a spark node
    /// Right now works only for images using hadoop2.7
    /// # Arguments
    /// * `version` - current specified SparkVersion
    ///
    pub fn get_command(&self, version: &SparkVersion) -> String {
        // TODO: remove hardcoded and adapt for versioning
        format!(
            "spark-{}-bin-hadoop2.7/sbin/start-{}.sh",
            version.as_str(),
            self.as_str()
        )
    }
}

impl FromStr for SparkNodeType {
    type Err = CrdError;

    fn from_str(input: &str) -> Result<SparkNodeType, Self::Err> {
        match input {
            MASTER => Ok(SparkNodeType::Master),
            WORKER => Ok(SparkNodeType::Worker),
            HISTORY_SERVER => Ok(SparkNodeType::HistoryServer),
            _ => Err(CrdError::InvalidNodeType {
                node_type: input.to_string(),
            }),
        }
    }
}

impl fmt::Display for SparkNodeType {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

#[derive(Clone, Debug, Hash, Deserialize, Eq, JsonSchema, PartialEq, Serialize)]
pub struct ConfigOption {
    pub name: String,
    pub value: String,
}

#[derive(Clone, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize)]
pub struct SparkClusterStatus {
    pub current_version: SparkVersion,
    pub target_version: SparkVersion,
}

impl Crd for SparkCluster {
    const RESOURCE_NAME: &'static str = "sparkclusters.spark.stackable.tech";
    const CRD_DEFINITION: &'static str = include_str!("../sparkcluster.crd.yaml");
}

#[allow(non_camel_case_types)]
#[derive(Clone, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize)]
pub enum SparkVersion {
    #[serde(rename = "2.4.7")]
    v2_4_7,

    #[serde(rename = "3.0.1")]
    v3_0_1,
}

impl SparkVersion {
    pub fn as_str(&self) -> &'static str {
        match self {
            SparkVersion::v2_4_7 => "2.4.7",
            SparkVersion::v3_0_1 => "3.0.1",
        }
    }
}

impl fmt::Display for SparkVersion {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl Default for SparkVersion {
    fn default() -> Self {
        SparkVersion::v3_0_1
    }
}
