use axum::extract::Extension;
use axum::routing::post;
use axum::Router;
use clap::Parser;
use dashmap::DashMap;
use minijinja::{Environment, Value as JinjaValue};
use sovereign_rs::app::{discovery, State};
use sovereign_rs::config::{Settings, SourceConfig, TemplateContextConfig};
use sovereign_rs::context::poll_context;
use sovereign_rs::sources::{poll_sources, poll_sources_into_buckets, InstancesPackage};
use std::net::IpAddr;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::signal::ctrl_c;
use tokio::sync::watch::{self, Receiver};
use tokio::time::{sleep, Duration};
use tracing::debug;
use tracing_subscriber::{EnvFilter, FmtSubscriber};

#[derive(Parser, Debug)]
struct Args {
    #[clap(long, default_value = "0.0.0.0", env = "SOVEREIGN_HOST")]
    pub listen_address: IpAddr,

    #[clap(long, default_value_t = 8080, env = "SOVEREIGN_PORT")]
    pub listen_port: u16,
}

fn setup_context_channel(config: TemplateContextConfig) -> Receiver<JinjaValue> {
    let (tx, rx) = watch::channel(poll_context(&config.items));
    tokio::spawn(async move {
        loop {
            sleep(config.interval).await;
            let ctx = poll_context(&config.items);
            _ = tx.send(ctx);
        }
    });
    rx
}

fn setup_sources_channel(
    settings: Settings,
    config: SourceConfig,
) -> Receiver<Vec<InstancesPackage>> {
    if let Some(matching) = settings.node_matching {
        let instances = poll_sources_into_buckets(&config.items, &matching.source_key).unwrap();
        let (tx, rx) = watch::channel(instances);
        tokio::spawn(async move {
            loop {
                sleep(config.interval).await;
                if let Ok(sources) = poll_sources_into_buckets(&config.items, &matching.source_key)
                {
                    _ = tx.send(sources);
                }
            }
        });
        rx
    } else {
        let initial = poll_sources(&config.items).unwrap();
        let (tx, rx) = watch::channel(initial);
        tokio::spawn(async move {
            loop {
                sleep(Duration::from_secs(30)).await;
                if let Ok(sources) = poll_sources(&config.items) {
                    _ = tx.send(sources);
                }
            }
        });
        rx
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    pyo3::prepare_freethreaded_python();

    FmtSubscriber::builder()
        .with_env_filter(EnvFilter::from_default_env())
        .event_format(tracing_subscriber::fmt::format())
        .compact()
        .json()
        .init();

    let settings = match Settings::new() {
        Ok(s) => s,
        Err(e) => {
            panic!("Could not load config: {e}")
        }
    };

    debug!(target: "sovereign_rs", "Setting up sources channel");
    let mut sources_rx = None;
    if let Some(source_conf) = &settings.sources {
        sources_rx = Some(setup_sources_channel(settings.clone(), source_conf.clone()));
    }
    debug!(target: "sovereign_rs", "Completed setting up sources channel");

    debug!(target: "sovereign_rs", "Setting up context channel");
    let mut context_rx = None;
    if let Some(context_conf) = &settings.template_context {
        context_rx = Some(setup_context_channel(context_conf.clone()));
    }
    debug!(target: "sovereign_rs", "Completed setting up context channel");

    debug!(target: "sovereign_rs", "Setting up templates");
    let templates = DashMap::new();
    for template in settings.templates.iter() {
        templates.insert(template.name(), template.clone());
    }
    debug!(target: "sovereign_rs", "Completed setting up templates");

    let state = Arc::new(State {
        instances: sources_rx,
        context: context_rx,
        env: Environment::new(),
        templates,
    });

    let app = Router::new()
        .route("/:version/*resource", post(discovery))
        .layer(Extension(state));

    debug!(target: "sovereign_rs", "Starting server");
    let addr = SocketAddr::new(args.listen_address, args.listen_port);
    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .with_graceful_shutdown(async {
            ctrl_c().await.unwrap();
            debug!(target: "sovereign_rs", "Shutting down gracefully")
        })
        .await?;

    Ok(())
}
