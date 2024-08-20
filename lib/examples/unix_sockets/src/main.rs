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

	#[cfg(windows)]
  {
      println!("Unix sockets are not supported on Windows!");
      std::process::exit(1);
  }
  fn cleanup(socket: String) -> std::io::Result<()> {
      // Remove the Unix socket file
      fs::remove_file(&socket)?;
      Ok(())
  }

	// setup db 
	let endpoint = env::var("SURREALDB_ENDPOINT").unwrap_or_else(|_| "memory".to_owned());
	let db = any::connect(endpoint).await?;
	db.use_ns("namespace").use_db("database").await?;

	let router = create_router(db);

	// setup socket
  let socket = "/tmp/surrealdb_ipc";
  ctrlc::set_handler(move || {
      cleanup(socket.to_string()).expect("Error cleaning up");
      process::exit(0);
  })
  .expect("Error setting Ctrl-C handler");

  unix_interface(&socket.to_string(), &router).await;


	Ok(())
}


#[cfg(not(windows))]
pub async fn unix_interface(socket: &String, router: &Router) {
    use tokio::net::UnixListener;

    let listener = UnixListener::bind(socket).expect("Failed to bind unix socket listener");

    println!("Listening on socket {}", socket);
    // Continuously accept new connections.
    loop {
        let (socket, _remote_addr) = listener
            .accept()
            .await
            .expect("Failed to accept unix socket connection");

        let tower_service = router.clone();

        // Spawn a task to handle the connection. That way we can have multiple connections concurrently.
        tokio::spawn(async move {
            // Hyper has its own `AsyncRead` and `AsyncWrite` traits and doesn't use tokio.
            // `TokioIo` converts between them.
            let socket = TokioIo::new(socket);

            // Hyper also has its own `Service` trait and doesn't use  . We can use
            // `hyper::service::service_fn` to create a hyper `Service` that calls our app through
            // `tower::Service::call`.
            let hyper_service = hyper::service::service_fn(move |request: Request<Incoming>| {
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
}
