use axum::extract::Extension;
use axum::routing::post;
use axum::Router;
use dashmap::DashMap;
use minijinja::{Environment, Value as JinjaValue};
use sovereign_rs::app::{discovery, State};
use sovereign_rs::config::{Settings, SourceConfig, TemplateContextConfig};
use sovereign_rs::context::poll_context;
use sovereign_rs::sources::{poll_sources, poll_sources_into_buckets, InstancesPackage};
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::signal::ctrl_c;
use tokio::sync::watch::{self, Receiver};
use tokio::time::{sleep, Duration};

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
    pyo3::prepare_freethreaded_python();

    let settings = match Settings::new() {
        Ok(s) => s,
        Err(e) => {
            panic!("Could not load config: {e}")
        }
    };

    let mut sources_rx = None;
    if let Some(source_conf) = &settings.sources {
        sources_rx = Some(setup_sources_channel(settings.clone(), source_conf.clone()));
    }

    let mut context_rx = None;
    if let Some(context_conf) = &settings.template_context {
        context_rx = Some(setup_context_channel(context_conf.clone()));
    }

    let templates = DashMap::new();
    for template in settings.templates.iter() {
        templates.insert(template.name(), template.clone());
    }

    let state = Arc::new(State {
        instances: sources_rx,
        context: context_rx,
        env: Environment::new(),
        templates,
    });

    let app = Router::new()
        .route("/:version/*resource", post(discovery))
        .layer(Extension(state));

    let addr = SocketAddr::from(([0, 0, 0, 0], 8070));
    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .with_graceful_shutdown(async {
            ctrl_c().await.unwrap();
            println!("Shutting down gracefully")
        })
        .await?;

    Ok(())
}
