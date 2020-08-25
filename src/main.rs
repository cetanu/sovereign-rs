use futures_core::Stream;
use std::pin::Pin;
use std::sync::Arc;
use tokio::sync::mpsc;
use tonic::transport::Server;
use tonic::{Request, Response, Status, Streaming};

mod proto;

use prost::Message;
use prost_types::{Any, Duration};
use proto::envoy;

use envoy::api::v2::cluster_discovery_service_server::{
    ClusterDiscoveryService, ClusterDiscoveryServiceServer,
};
use envoy::api::v2::core::{address, Address, Node};
use envoy::api::v2::listener_discovery_service_server::{
    ListenerDiscoveryService, ListenerDiscoveryServiceServer,
};
use envoy::api::v2::route_discovery_service_server::{
    RouteDiscoveryService, RouteDiscoveryServiceServer,
};
use envoy::api::v2::{
    cluster, Cluster, DeltaDiscoveryRequest, DeltaDiscoveryResponse, DiscoveryRequest,
    DiscoveryResponse,
};
use std::collections::HashMap;

const CLUSTER_TYPE_URL: &str = "envoy.api.v2.Cluster";

enum DiscoveryType {
    Static = 0,
    StrictDns = 1,
    LogicalDns = 2,
    Eds = 3,
    OriginalDst = 4,
}

enum LbPolicy {
    RoundRobin = 0,
    LeastRequest = 1,
    RingHash = 2,
    Random = 3,
    OriginalDstLb = 4,
    Maglev = 5,
    ClusterProvided = 6,
    LoadBalancingPolicyConfig = 7,
}

enum DnsLookupFamily {
    Auto = 0,
    V4Only = 1,
    V6Only = 2,
}

enum ClusterProtocolSelection {
    UseConfiguredProtocol = 0,
    UseDownstreamProtocol = 1,
}

pub struct Instance {
    name: String,
}

impl From<&Instance> for Cluster {
    fn from(instance: &Instance) -> Self {
        Cluster {
            name: instance.name.clone(),
            alt_stat_name: instance.name.clone(),
            connect_timeout: Some(Duration {
                seconds: 0,
                nanos: 250_000,
            }),
            per_connection_buffer_limit_bytes: None,
            lb_policy: LbPolicy::Random as i32,
            load_assignment: None,
            eds_cluster_config: None,
            transport_socket_matches: vec![],
            hosts: vec![],
            health_checks: vec![],
            max_requests_per_connection: None,
            circuit_breakers: None,
            tls_context: None,
            upstream_http_protocol_options: None,
            common_http_protocol_options: None,
            http_protocol_options: None,
            http2_protocol_options: None,
            extension_protocol_options: HashMap::new(),
            typed_extension_protocol_options: HashMap::new(),
            dns_refresh_rate: None,
            dns_failure_refresh_rate: None,
            respect_dns_ttl: true,
            dns_lookup_family: DnsLookupFamily::V4Only as i32,
            dns_resolvers: vec![],
            use_tcp_for_dns_lookups: false,
            outlier_detection: None,
            cleanup_interval: None,
            upstream_bind_config: None,
            lb_subset_config: None,
            common_lb_config: None,
            transport_socket: None,
            metadata: None,
            protocol_selection: ClusterProtocolSelection::UseDownstreamProtocol as i32,
            upstream_connection_options: None,
            close_connections_on_host_health_failure: false,
            drain_connections_on_host_removal: false,
            filters: vec![],
            load_balancing_policy: None,
            lrs_server: None,
            track_timeout_budgets: false,
            cluster_discovery_type: Some(cluster::ClusterDiscoveryType::Type(1)),
            lb_config: None,
        }
    }
}

pub struct DiscoveryServer {
    sources: Vec<Option<Instance>>,
}

#[tonic::async_trait]
impl ClusterDiscoveryService for DiscoveryServer {
    type StreamClustersStream =
        Pin<Box<dyn Stream<Item = Result<DiscoveryResponse, Status>> + Send + Sync + 'static>>;
    type DeltaClustersStream =
        Pin<Box<dyn Stream<Item = Result<DeltaDiscoveryResponse, Status>> + Send + Sync + 'static>>;

    async fn fetch_clusters(
        &self,
        request: Request<DiscoveryRequest>,
    ) -> Result<Response<DiscoveryResponse>, Status> {
        let mut buf: Vec<u8> = Vec::new();
        self.sources.iter().for_each(|cluster| match cluster {
            Some(c) => Cluster::from(c).encode(&mut buf).unwrap(),
            _ => (),
        });

        let resources = vec![Any {
            type_url: String::from("envoy.api.v2.Cluster"),
            value: buf,
        }];

        Ok(Response::new(DiscoveryResponse {
            version_info: String::from("1"),
            resources,
            canary: false,
            type_url: String::from("bullshit"),
            nonce: String::from("1"),
            control_plane: None,
        }))
    }

    async fn stream_clusters(
        &self,
        request: Request<Streaming<DiscoveryRequest>>,
    ) -> Result<Response<Self::StreamClustersStream>, Status> {
        unimplemented!()
    }

    async fn delta_clusters(
        &self,
        request: Request<Streaming<DeltaDiscoveryRequest>>,
    ) -> Result<Response<Self::DeltaClustersStream>, Status> {
        unimplemented!()
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let addr = "[::1]:10000".parse().unwrap();
    let cds = DiscoveryServer {
        sources: vec![Some(Instance {
            name: "helloworld".to_string(),
        })],
    };
    //let rds = DiscoveryServer {};
    //let lds = DiscoveryServer {};
    let clusters = ClusterDiscoveryServiceServer::new(cds);
    //let routes = RouteDiscoveryServiceServer::new(rds);
    //let listeners = ListenerDiscoveryServiceServer::new(lds);
    Server::builder().add_service(clusters).serve(addr).await?;
    //Server::builder().add_service(routes).serve(addr).await?;
    //Server::builder().add_service(listeners).serve(addr).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_discovery_request(cluster: String, resource_names: Vec<String>) -> DiscoveryRequest {
        DiscoveryRequest {
            version_info: String::from("0"),
            node: Some(Node {
                id: String::from("envoy"),
                cluster,
                build_version: String::from("whatever"),
                user_agent_name: String::from("envoy"),
                user_agent_version_type: None,
                extensions: vec![],
                client_features: vec![],
                listening_addresses: vec![],
                locality: None,
                metadata: None,
            }),
            resource_names,
            type_url: String::from("envoy.api.v2.Cluster"),
            error_detail: None,
            response_nonce: String::from("0"),
        }
    }

    #[tokio::test]
    async fn fetch_discovery_request() {
        let d = make_discovery_request(String::from("T1"), vec![String::from("hello")]);
        let r = Request::new(d);
        let server = DiscoveryServer {
            sources: vec![Some(Instance {
                name: "helloworld".to_string(),
            })],
        };
        let result = server.fetch_clusters(r).await.unwrap();
        let expected = DiscoveryResponse {
            version_info: String::from("1"),
            resources: vec![],
            canary: false,
            type_url: String::from("bullshit"),
            nonce: String::from("1"),
            control_plane: None,
        };
        assert_eq!(result.into_inner(), expected)
    }
}
