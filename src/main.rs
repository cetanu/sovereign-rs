use futures_core::Stream;
use std::pin::Pin;
use std::sync::Arc;
use tokio::sync::mpsc;
use tonic::{Request, Response, Status, Streaming};
use tonic::transport::Server;
mod proto;
use proto::{envoy, google};
use envoy::api::v2::cluster_discovery_service_server::{ClusterDiscoveryService, ClusterDiscoveryServiceServer};
use envoy::api::v2::{DiscoveryRequest, DiscoveryResponse, DeltaDiscoveryResponse, DeltaDiscoveryRequest};

pub struct DiscoveryServer { }

#[tonic::async_trait]
impl ClusterDiscoveryService for DiscoveryServer {
    async fn fetch_clusters(&self, request: Request<DiscoveryRequest>) -> Result<Response<DiscoveryResponse>, Status> {  }

    type StreamClustersStream = Pin<Box<dyn Stream<Item = Result<DiscoveryResponse, Status>> + Send + Sync + 'static>>;
    async fn stream_clusters(&self, request: Request<Streaming<DiscoveryRequest>>) -> Result<Response<Self::StreamClustersStream>, Status> { }

    type DeltaClustersStream = Pin<Box<dyn Stream<Item = Result<DeltaDiscoveryResponse, Status>> + Send + Sync + 'static>>;
    async fn delta_clusters(&self, request: Request<Streaming<DeltaDiscoveryRequest>>) -> Result<Response<Self::DeltaClustersStream>, Status> { }
}


#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let addr = "[::1]:10000".parse().unwrap();

    let server = DiscoveryServer{ };

    let svc = ClusterDiscoveryServiceServer::new(server);
    Server::builder().add_service(svc).serve(addr).await?;
    Ok(())
}
