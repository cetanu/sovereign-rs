pub mod envoy {
    pub mod r#type {
        tonic::include_proto!("envoy.r#type");
        pub mod matcher {
            tonic::include_proto!("envoy.r#type.matcher");
        }

        pub mod tracing {
            pub mod v2 {
                tonic::include_proto!("envoy.r#type.tracing.v2");
            }
        }

        pub mod metadata {
            pub mod v2 {
                tonic::include_proto!("envoy.r#type.metadata.v2");
            }
        }
    }

    pub mod api {
        pub mod v2 {
            tonic::include_proto!("envoy.api.v2");

            pub mod listener_ns {
                tonic::include_proto!("envoy.api.v2.listener_ns");
            }

            pub mod cluster_ns {
                tonic::include_proto!("envoy.api.v2.cluster_ns");
            }

            pub mod core {
                tonic::include_proto!("envoy.api.v2.core");
            }

            pub mod auth {
                tonic::include_proto!("envoy.api.v2.auth");
            }

            pub mod endpoint {
                tonic::include_proto!("envoy.api.v2.endpoint");
            }

            pub mod route {
                tonic::include_proto!("envoy.api.v2.route");
            }
        }
    }

    pub mod config {
        pub mod filter {
            pub mod accesslog {
                pub mod v2 {
                    tonic::include_proto!("envoy.config.filter.accesslog.v2");
                }
            }
        }

        pub mod listener {
            pub mod v2 {
                tonic::include_proto!("envoy.config.listener.v2");
            }
        }
    }
}

pub mod google {
    pub mod rpc {
        tonic::include_proto!("google.rpc");
    }
}
