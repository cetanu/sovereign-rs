fn main() {
    tonic_build::configure()
        .compile(
            &[
                "envoy/api/v2/discovery.proto",
                "envoy/api/v2/cds.proto",
                "envoy/api/v2/lds.proto",
                "envoy/api/v2/rds.proto",
                "envoy/api/v2/eds.proto",
                "envoy/api/v2/srds.proto",
            ],
            &["proto/"],
        )
        .unwrap_or_else(|e| panic!("Failed to compile protos {:?}", e));
}
