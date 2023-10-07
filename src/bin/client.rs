use reqwest::blocking::Client;
use sovereign_rs::types::DiscoveryRequest;

fn main() {
    let req = DiscoveryRequest::new();
    let client = Client::new();
    let response = client
        .post("http://localhost:8070/v3/discovery:clusters")
        .json(&req)
        .send()
        .unwrap();
    println!("{}", response.text().unwrap());
}
