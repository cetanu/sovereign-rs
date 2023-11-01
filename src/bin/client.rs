use clap::Parser;
use reqwest::Client;
use sovereign_rs::envoy_types::DiscoveryRequest;

#[derive(Parser, Debug)]
struct Args {
    #[arg(long, default_value = "clusters")]
    resource_type: String,

    #[arg(long, default_value = "commercial_development_customer_shared_1")]
    service_cluster: String,

    #[arg(long, value_delimiter = ',')]
    resource_names: Option<Vec<String>>,

    #[arg(long, default_value = "1.25.0")]
    envoy_version: String,
}

#[tokio::main]
async fn main() {
    let args = Args::parse();
    let resource_type = args.resource_type;
    let req = DiscoveryRequest::new(
        args.service_cluster,
        args.envoy_version,
        args.resource_names,
    );
    let client = Client::new();
    println!("Getting response");
    let response = client
        .post(format!(
            "http://localhost:8070/v3/discovery:{resource_type}"
        ))
        .json(&req)
        .send()
        .await
        .unwrap();
    println!("{:?}", response);
    println!("{}", response.text().await.unwrap());
}
