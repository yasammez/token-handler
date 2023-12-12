mod error;
mod endpoints;
mod systems;
mod components;

use std::sync::Arc;
use actix_cors::Cors;
use actix_web::{App, HttpServer, web};
use actix_web::body::{BoxBody, EitherBody, MessageBody};
use actix_web::dev::{Service, ServiceResponse};
use clap::Parser;
use itertools::Itertools;
use regex::{Captures, Regex};
use crate::components::config::{Api, Bridge, Config};
use crate::components::substitutions::Substitutions;
use crate::error::ErrorResponse;
use futures_util::FutureExt;
use log::info;
use crate::components::spec::Spec;

#[derive(Parser, Debug)]
#[command(about = "Token Handler")]
#[command(long_about = "Proxy between a web frontend and an OAuth2-enabled API that supports
Authorization Code Grant with PKCE. Manages OAuth-Tokens and keeps them safely away from the browser.
The browser gets configured with good old-fashioned home-baked cookies.")]
#[command(version, author)]
struct Args {
    /// Path of the configuration file
    #[arg(short = 'f', long, value_name = "FILE", default_value = "config.hcl")]
    pub config_file: String,
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    let args = Args::parse();

    let config_file = std::fs::read_to_string(&args.config_file).unwrap_or_else(|e| {
        eprintln!("Unable to read configuration file `{}`: {e}", &args.config_file);
        std::process::exit(1);
    });

    let re = Regex::new("\\$\\{([a-zA-Z_0-9]+)\\}").unwrap();
    let vars = re.captures_iter(&config_file).fold(Substitutions::new(), |acc: Substitutions, caps: Captures| {
        let key = &caps[1];
        match std::env::var(key) {
            Ok(val) => acc.ok(key.into(), val),
            Err(_) => acc.err(key.into())
        }
    });
    let vars = match vars {
        Substitutions::Ok(m) => m,
        Substitutions::Err(v) => {
            eprintln!("Unable to find environment variables:\n{}", v.iter().map(|s| format!("- {s}\n")).join(""));
            std::process::exit(2);
        }
    };
    let config_file = re.replace_all(&config_file, |caps: &Captures| { &vars[&caps[1]] });

    let spec: Spec = hcl::from_str(&config_file).unwrap_or_else(|e| {
        eprintln!("Unable to parse configuration file: {e}");
        std::process::exit(3)
    });

    let config: web::Data<Config> = <&Spec as TryInto<Arc<Config>>>::try_into(&spec).unwrap_or_else(|e| {
        eprintln!("Unable to understand configuration file: {e}");
        std::process::exit(4)
    }).into();

    env_logger::builder()
        .format_target(false)
        .parse_env(env_logger::Env::new().default_filter_or("info"))
        .init();

    let _ = hcl::to_string(&config).map(|c| info!("Loaded config\n{}", c));

    let port = config.port;
    HttpServer::new(move || {
        let config = config.clone();
        let expose_errors = config.expose_errors;
        let cors = Cors::default()
            .allow_any_origin()
            .allow_any_method()
            .allow_any_header()
            .supports_credentials()
            .max_age(3600);
        let builder = App::new()
            .wrap(cors)
            .app_data(config.clone())
            .service(endpoints::health)
            .wrap_fn(move |req, srv| {
                srv.call(req).map(move |res| {
                    res.map(|mut res: ServiceResponse<EitherBody<BoxBody>>| {
                        let error = res.response_mut().extensions_mut().remove::<ErrorResponse>();
                        match (error, expose_errors) {
                            (Some(body), true) => {
                                match serde_json::to_string(&body) {
                                    Ok(body) => res.map_body(|_, _| MessageBody::boxed(body)),
                                    Err(_) => res.map_body(|_, _| MessageBody::boxed("Error while serializing error message")),
                                }
                            },
                            _ => res.map_into_boxed_body(),
                        }
                    })
                })
            });
        config.bridges.iter()
            .map(|(id, bridge)| {
                let scope = web::scope(&format!("/bridge/{}", id))
                    .app_data::<web::Data<Bridge>>(bridge.clone().into())
                    .service(endpoints::me)
                    .service(endpoints::login)
                    .service(endpoints::login2)
                    .service(endpoints::logout);
                bridge.apis.iter().map(|(id, api)| {
                    web::scope(&format!("/proxy/{}", id))
                        .app_data::<web::Data<Api>>(api.clone().into())
                        .route("/{tail:.*}", web::to(endpoints::proxy))
                }).fold(scope, |scope, api| scope.service(api))
            }).fold(builder, |builder, bridge| builder.service(bridge))
    })
        .bind(("0.0.0.0", port))?
        .run()
        .await
}

