use reqwest::Client;
use sovereign_rs::types::DiscoveryRequest;

#[tokio::main]
async fn main() {
    let req = DiscoveryRequest::new("commercial_development_customer_shared_1".to_string());
    let client = Client::new();
    let response = client
        .post("http://localhost:8070/v3/discovery:clusters")
        .json(&req)
        .send()
        .await
        .unwrap();
    println!("{}", response.text().await.unwrap());
}
