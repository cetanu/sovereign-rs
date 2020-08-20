use futures_core::Stream;
use std::pin::Pin;
use std::sync::Arc;
use tokio::sync::mpsc;
use tonic::transport::Server;
use tonic::{Request, Response, Status, Streaming};

mod proto;
use proto::{envoy, google};

use envoy::api::v2::cluster_discovery_service_server::{
    ClusterDiscoveryService, ClusterDiscoveryServiceServer,
};
use envoy::api::v2::core::Node;
use envoy::api::v2::{
    DeltaDiscoveryRequest, DeltaDiscoveryResponse, DiscoveryRequest, DiscoveryResponse,
};

pub struct DiscoveryServer {}

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
        Ok(Response::new(DiscoveryResponse {
            version_info: String::from("1"),
            resources: vec![],
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
    let server = DiscoveryServer {};
    let svc = ClusterDiscoveryServiceServer::new(server);
    Server::builder().add_service(svc).serve(addr).await?;
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
        let mut r = Request::new(d);
        let server = DiscoveryServer {};
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
