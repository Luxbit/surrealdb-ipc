use ipc_example::create_router;
use std::env;
use surrealdb::engine::any;
use surrealdb::opt::auth::Root;
use tokio::net::TcpListener;
use std::fs;
use axum::{routing, Router};
use std::process;
use hyper::{body::Incoming, Request};
use hyper_util::rt::{TokioExecutor, TokioIo};
use hyper_util::server;
use std::io;
use tower::Service;use surrealdb::sql::Thing;
use serde::{Deserialize, Serialize}; 
  

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {

  #[cfg(not(windows))]
  {
      println!("Named pipes are only supported on Windows!");
      std::process::exit(1);
  }

	// setup db 
	let endpoint = env::var("SURREALDB_ENDPOINT").unwrap_or_else(|_| "memory".to_owned());
	let db = any::connect(endpoint).await?;
	db.use_ns("namespace").use_db("database").await?;

	let router = create_router(db);
    let socket = "\\\\.\\pipe\\surrealdb_ipc";

    #[cfg(windows)]
    {
        interface::named_pipe_interface(&socket.to_string(), &router).await;
    }


  Ok(())
}


#[cfg(windows)]
pub async fn named_pipe_interface(pipe: &String, app: &Router) {
    use hyper_util::rt::tokio::TokioIo;
    use tokio::io::AsyncWriteExt;
    use tokio::io::{AsyncBufReadExt, BufReader};
    use tokio::net::windows::named_pipe::{ClientOptions, ServerOptions};

    let mut server = ServerOptions::new()
        .first_pipe_instance(true)
        .create(pipe)?;

    println!("Listening on pipe {}", pipe);

    // Continuously accept new connections.
    loop {
        // Wait for a client to connect.
        match server.connect().await {
            Ok(conn) => {
                let new_server = ServerOptions::new()
                    .create(pipe)
                    .expect("Failed to create pipe server");

                let tower_service = app.clone();

                tokio::spawn(async move {
                    // Hyper has its own `AsyncRead` and `AsyncWrite` traits and doesn't use tokio.
                    // `TokioIo` converts between them.
                    let old_server = std::mem::replace(&mut server, new_server);
                    let socket = TokioIo::new(old_server);

                    // Hyper also has its own `Service` trait and doesn't use tower. We can use
                    // `hyper::service::service_fn` to create a hyper `Service` that calls our app through
                    // `tower::Service::call`.
                    let hyper_service =
                        hyper::service::service_fn(move |request: Request<Incoming>| {
                            // We don't need to call `poll_ready` since `Router` is always ready.
                            tower_service.clone().call(request)
                        });

                    // `server::conn::auto::Builder` supports both http1 and http2.
                    //
                    // `TokioExecutor` tells hyper to use `tokio::spawn` to spawn tasks.
                    if let Err(err) = server::conn::auto::Builder::new(TokioExecutor::new())
                        // `serve_connection_with_upgrades` is required for websockets. If you don't need
                        // that you can use `serve_connection` instead.
                        .serve_connection(socket, hyper_service)
                        .await
                    {
                        eprintln!("failed to serve connection: {err:#}");
                    }
                });
            }
            Err(err) => println!("Failed to connect pipe: {}", err),
        }
    }
}
