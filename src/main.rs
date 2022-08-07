#[macro_use]
extern crate lazy_static;

mod graphql;
mod panic;
mod plugin;
mod slp;
#[cfg(test)]
mod test;
mod util;

use async_graphql::{
    http::{playground_source, GQLResponse, GraphQLPlaygroundConfig},
    QueryBuilder,
};
use env_logger::Env;
use graphql::{schema, Ctx};
use serde::Serialize;
use slp::UDPServerBuilder;
use std::convert::Infallible;
use std::net::SocketAddr;
use structopt::StructOpt;
use warp::{filters::BoxedFilter, http::Method, Filter};

macro_rules! version_string {
    () => {
        concat!(std::env!("CARGO_PKG_VERSION"), "-", std::env!("GIT_HASH"))
    };
}

#[derive(Debug, StructOpt)]
#[structopt(
    name = "slp-server-rust",
    version = version_string!(),
    author = "imspace <spacemeowx2@gmail.com>",
    about = "switch-lan-play Server written in Rust",
)]
struct Opt {
    /// Sets server listening port
    #[structopt(short, long, default_value = "11451")]
    port: u16,
    /// Token for admin query. If not preset, no one can query admin information.
    #[structopt(long)]
    admin_token: Option<String>,
    /// Don't send broadcast to idle clients
    #[structopt(short, long)]
    ignore_idle: bool,
    /// Block rules
    #[structopt(short, long, default_value = "tcp:5000,tcp:21", use_delimiter = true)]
    block_rules: Vec<plugin::blocker::Rule>,
}

#[derive(Serialize)]
struct Info {
    online: i32,
    version: String,
}

async fn server_info(context: Ctx) -> Result<impl warp::Reply, Infallible> {
    Ok(warp::reply::json(&context.udp_server.server_info().await))
}

fn make_state(context: &Ctx) -> BoxedFilter<(Ctx,)> {
    let ctx = context.clone();
    warp::any().map(move || ctx.clone()).boxed()
}

#[tokio::main]
async fn main() -> std::io::Result<()> {
    env_logger::from_env(Env::default().default_filter_or("slp_server_rust=info")).init();
    panic::set_panic_hook();

    tokio::spawn(async {
        tokio::signal::ctrl_c()
            .await
            .expect("Failed to receive ctrl-c");
        log::info!("Exiting by ctrl-c");
        std::process::exit(0);
    });
    #[cfg(unix)]
    tokio::spawn(async {
        use tokio::signal::unix;
        unix::signal(unix::SignalKind::terminate())
            .expect("Failed to receive SIGTERM")
            .recv()
            .await;
        log::info!("Exiting by SIGTERM");
        std::process::exit(0);
    });

    let opt = Opt::from_args();

    if opt.ignore_idle {
        log::info!("--ignore-idle is not tested, bugs are expected");
    }

    let bind_address = format!("{}:{}", "0.0.0.0", opt.port);
    let socket_addr: &SocketAddr = &bind_address.parse().unwrap();

    let udp_server = UDPServerBuilder::new()
        .ignore_idle(opt.ignore_idle)
        .build(socket_addr)
        .await?;
    plugin::register_plugins(&udp_server).await;
    if opt.block_rules.len() > 0 {
        log::info!("Applying {} rules", opt.block_rules.len());
        log::debug!("rules: {:?}", opt.block_rules);
        udp_server.get_plugin(
            &plugin::blocker::BLOCKER_TYPE,
            |b| b.map(|b| b.set_block_rules(opt.block_rules.clone()))
        ).await;
    }

    let context = Ctx::new(udp_server, opt.admin_token);

    log::info!("Listening on {}", bind_address);

    let graphql_filter = async_graphql_warp::graphql(schema(&context)).and_then(
        |(schema, builder): (_, QueryBuilder)| async move {
            // 执行查询
            let resp = builder.execute(&schema).await;

            // 返回结果
            Ok::<_, Infallible>(warp::reply::json(&GQLResponse(resp)))
        },
    );
    let graphql_ws_filter = async_graphql_warp::graphql_subscription(schema(&context));

    let cors = warp::cors()
        .allow_headers(vec!["content-type", "x-apollo-tracing"])
        .allow_methods(&[Method::POST])
        .allow_any_origin();

    let log = warp::log("warp_server");
    let routes = (warp::path("info")
        .and(make_state(&context))
        .and_then(server_info)
        .or(warp::post().and(graphql_filter))
        .or(warp::get().and(graphql_ws_filter)))
    .or(warp::get().map(|| {
        warp::reply::html(playground_source(
            GraphQLPlaygroundConfig::new("/").subscription_endpoint("/"),
        ))
    }))
    .with(log)
    .with(cors);

    warp::serve(routes).run(*socket_addr).await;

    Ok(())
}
