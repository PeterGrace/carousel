#[macro_use]
extern crate tracing;

use tracing_subscriber::EnvFilter;
use std::{env, process};
use k8s_openapi::api::core::v1::{Node, Pod};
use lazy_static::lazy_static;
use tokio::sync::OnceCell;
use kube::{Api, Client, Resource};
use kube::runtime::{watcher, WatchStreamExt};
use futures::{StreamExt, TryStreamExt};
use k8s_openapi::chrono::{Days, Utc};
use k8s_openapi::chrono::DateTime;
use kube::api::{DeleteParams, EvictParams, ListParams, Request};
use kube_runtime::reflector::Lookup;
use thiserror::Error;
use tokio::task::{JoinError, JoinSet};
use tokio::time::{Duration, Instant};
use serde_json::Value;

lazy_static! {
    pub static ref SHUTDOWN: OnceCell<bool> = OnceCell::new();
}

const POD_CULL_DAYS: u64 = 7;
const NODE_CULL_DAYS: u64 = 7;


#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .with_file(true)
        .with_line_number(true)
        .init();

    eprintln!("carousel {} {} - set RUST_LOG for log levels",
              env!("CARGO_PKG_VERSION"),
              env!("GIT_HASH"));

    let client = match Client::try_default().await {
        Ok(c) => c,
        Err(e) => {
            error!("Unable to initialize k8s api: {e}");
            process::exit(1);
        }
    };
    let nodes: Api<Node> = Api::all(client.clone());
    let pods: Api<Pod> = Api::all(client.clone());
    let listparams = ListParams::default();


    while !SHUTDOWN.initialized() {
        let nodes_to_check = match nodes.list(&listparams).await {
            Ok(n) => n,
            Err(e) => {
                error!("{e}");
                break;
            }
        };

        let mut notready: bool = false;
        let mut node_cull_list: Vec<(String, DateTime<Utc>)> = vec![];

        //region Node Selection and Culling
        for node in nodes_to_check.items.iter() {
            let node = node.clone();
            let metadata = node.metadata;
            let Some(name) = metadata.name else {
                error!("Node didn't have a name?");
                continue;
            };
            let Some(spec) = node.spec else {
                error!("Node {name} didn't have a spec?");
                continue;
            };
            let Some(status) = node.status else {
                error!("Node {name} didn't have a status?");
                continue;
            };
            let Some(conditions) = status.conditions else {
                error!("Node {name} didn't have status conditions?");
                continue;
            };



            // if node has SchedulingDisabled status, we shouldn't force another node drain or we may
            // put the cluster under strain.
            if spec.unschedulable.is_some_and(|f| f == true) {
                warn!("Node {name} is unschedulable, carousel is skipping this iteration");
                notready = true;
                break;
            };

            // if we have nodes that are in NotReady, they could be in the process of being drained,
            // or there may be an issue with autoscaling, and that should be resolved before we drain
            // more nodes.
            for condition in conditions.iter() {
                if condition.type_ == "NotReady" {
                    warn!("Node {name} is not ready, carousel is skipping this iteration");
                    notready = true;
                    break;
                }
            }

            // We select for nodes to autodelete based on provider id, since autoscaling sets provider
            // id.  Otherwise we might drain an actual static node.
            if spec.provider_id.is_some_and(|s| !s.contains("libvirt")) {
                info!("node {} is not in scope for carousel", &name);
                continue;
            }

            // Grab the node creation timestamp, check if its older, and if it is, add to cull_list.
            // We collect these into an array even if we're only doing one at a time, because we need
            // to iterate the list of nodes completely to ensure none of the cohort are unready or
            // SchedulingDisabled.
            if let Some(create) = metadata.creation_timestamp {
                let dt = create.0;
                if let Some(max_age) = dt.checked_add_days(Days::new(NODE_CULL_DAYS)) {
                    let now = Utc::now();
                    if now >= max_age {
                        debug!("node {} is older than timestamp so we should drain it",&name);
                        node_cull_list.push((name, dt));
                    }
                }
            } else {
                error!("Node did not have a creation timestamp....?");
                continue;
            }
        };
        if node_cull_list.len() > 0 && notready == false {
            // we have nodes to drain, so lets grab the oldest one
            node_cull_list.sort_by_key(|(_, d)| *d);

            let (node_name, date) = node_cull_list.get(0).unwrap();
            // let node = match nodes.get(&node_name).await {
            //     Ok(n) => n,
            //     Err(e) => {
            //         error!("Couldn't get node {node_name}: {e}");
            //         break;
            //     }
            // };

            if let Err(e) = nodes.cordon(node_name).await {
                error!("Attempted to cordon {node_name} but it failed {e}");
            } else {
                info!("Node {node_name}, that was born on {date}, ascends to carousel.");
            };

            let mut plp = ListParams::default();
            plp.field_selector = Some(format!("spec.nodeName={node_name}"));
            let Ok(list) = pods.list(&plp).await else {
                error!("Couldn't execute search for pods on {node_name}");
                break;
            };
            for pod in list {
                let Some(name) = pod.metadata.name else {
                    error!("Couldn't get a pod name on {node_name}");
                    break;
                };
                let Some(namespace) = pod.metadata.namespace else {
                    error!("Couldn't get pod namespace for pod {name}");
                    break;
                };

                let url = Pod::url_path(&(), Some(&namespace));

                // if a pod has an emptydir mount, we have to delete it instead of evicting it.
                let Some(spec) = pod.spec else {
                    error!("Couldn't get pod spec for pod {name}");
                    break;
                };
                let mut deleted: bool = false;
                if let Some(vols) = spec.volumes {
                    for vol in vols.iter() {
                        if vol.empty_dir.is_some() {
                            warn!("Pod {namespace}/{name} has local storage, we must delete instead of evict.");
                            let dp = DeleteParams::default();
                            let req = Request::new(url.clone()).delete(&name, &dp).unwrap();
                            if let Err(e) = client.request::<Value>(req).await {
                                error!("Error deleting {namespace}/{name}: {e}");
                                break;
                            } else {
                                deleted = true;
                            }

                        }
                    }
                }
                // deleted is set if we issued a delete request above, in other words, if we deleted it
                // already we don't want to also evict it.
                if !deleted {
                    info!("evicting pod {namespace}/{name}.");
                    let ep = EvictParams::default();
                    let req = Request::new(url).evict(&name, &ep).unwrap();
                    if let Err(e) = client.request::<Value>(req).await {
                        error!("Couldn't evict {namespace}/{name}: {e}");
                        break;
                    }
                }
            };
        }
        //endregion

        let mut pod_cull_list: Vec<(Pod, DateTime<Utc>)> = vec![];

        //region Pod Selection and Culling
        if let Ok(all_pods) = pods.list(&listparams).await {
            for pod in all_pods {
                if let Some(status) = pod.clone().status {
                    if let Some(start_date) = status.start_time {
                        let name = pod.metadata.name.clone().unwrap().clone();
                        let dt = start_date.0;
                        if let Some(max_age) = dt.checked_add_days(Days::new(POD_CULL_DAYS)) {
                            let now = Utc::now();
                            if now >= max_age {
                                debug!("pod {} is older than timestamp so we should drain it",&name);
                                pod_cull_list.push((pod.clone(), dt));
                            }
                        }
                    } else {
                        info!("Pod {} didn't have a start time", pod.metadata.name.unwrap());
                    }
                } else {
                    info!("Pod {} didn't have a status", pod.metadata.name.unwrap());
                }
            }
        } else {
            error!("Couldn't retrieve pod list to check pod age");
        }
        if pod_cull_list.len() > 0 {
            pod_cull_list.sort_by_key(|( _, d)| *d);
            let (pod , date) = pod_cull_list.get(0).unwrap();
            let namespace = pod.metadata.namespace.clone().unwrap_or_default();
            let name = pod.metadata.name.clone().unwrap_or_default();
            let url = Pod::url_path(&(), Some(&namespace));
            let mut deleted = false;
            let spec = pod.spec.clone().unwrap();
            if let Some(vols) = spec.volumes {
                for vol in vols.iter() {
                    if vol.empty_dir.is_some() {
                        warn!("Pod {namespace}/{name} has local storage, we must delete instead of evict.");
                        let dp = DeleteParams::default();
                        let req = Request::new(url.clone()).delete(&name, &dp).unwrap();
                        if let Err(e) = client.request::<Value>(req).await {
                            error!("Error deleting {namespace}/{name}: {e}");
                            break;
                        } else {
                            deleted = true;
                        }

                    }
                }
            }
            if (!deleted) {
                let ep = EvictParams::default();
                let req = Request::new(&url).evict(&name, &ep).unwrap();
                if let Err(e) = client.request::<Value>(req).await {
                    error!("Couldn't evict {namespace}/{name}: {e}");
                }
            }
        }
        //endregion


        tokio::time::sleep(Duration::from_secs(300)).await;
    }
}
