use std::net::SocketAddr;
use std::result::Result;
use std::convert::Infallible;
use std::str;
use hyper::service::{make_service_fn, service_fn};
use hyper::{Body, Method, Request, Response, StatusCode, Server};
pub use mysql_async::prelude::*;
pub use mysql_async::*;
use serde::{Deserialize, Serialize};
use tokio::time::{sleep, Duration};

#[derive(Serialize, Deserialize, Debug)]
struct Order {
    order_id: i32,
    product_id: i32,
    quantity: i32,
    subtotal: f32,
    shipping_address: String,
    shipping_zip: String,
    shipping_cost: f32,
    total: f32,
}

impl Order {
    fn new(
        order_id: i32,
        product_id: i32,
        quantity: i32,
        subtotal: f32,
        shipping_address: String,
        shipping_zip: String,
        shipping_cost: f32,
        total: f32,
    ) -> Self {
        Self {
            order_id,
            product_id,
            quantity,
            subtotal,
            shipping_address,
            shipping_zip,
            shipping_cost,
            total,
        }
    }
}

/// This is our service handler. It receives a Request, routes on its
/// path, and returns a Future of a Response.
async fn handle_request(req: Request<Body>, pool: Pool) -> Result<Response<Body>, anyhow::Error> {
    match (req.method(), req.uri().path()) {
        // CORS OPTIONS
        (&Method::OPTIONS, "/init") => Ok(response_build(&String::from(""))),
        (&Method::OPTIONS, "/create_order") => Ok(response_build(&String::from(""))),
        (&Method::OPTIONS, "/orders") => Ok(response_build(&String::from(""))),

        // Serve some instructions at /
        (&Method::GET, "/") => Ok(Response::new(Body::from(
            "Try to GET /init such as: `curl http://localhost:8003/init`",
        ))),

        (&Method::GET, "/init") => {
            let mut conn = pool.get_conn().await.unwrap();
            // "DROP TABLE IF EXISTS orders;".ignore(&mut conn).await?;
            "CREATE TABLE IF NOT EXISTS orders (order_id INT NOT NULL AUTO_INCREMENT, product_id INT, quantity INT, subtotal FLOAT, shipping_address VARCHAR(1024), shipping_zip VARCHAR(32), shipping_cost FLOAT, total FLOAT, PRIMARY KEY (order_id));".ignore(&mut conn).await?;
            drop(conn);
            Ok(response_build("{\"status\":true}"))
        }

        (&Method::POST, "/create_order") => {
            let client = dapr::Dapr::new(3503, "http://localhost".to_string());
            let v = client.get_secret("local-store", "APP_URL:SALES_TAX_RATE_SERVICE").await?;
            let rate_url = v["APP_URL:SALES_TAX_RATE_SERVICE"].as_str().unwrap();

            let mut conn = pool.get_conn().await.unwrap();
            let byte_stream = hyper::body::to_bytes(req).await?;
            let mut order: Order = serde_json::from_slice(&byte_stream).unwrap();

            let client = reqwest::Client::new();
            let rate_resp = client.post(&*rate_url)
                .body(order.shipping_zip.clone())
                .send()
                .await?;

            if rate_resp.status().is_success() {
                let rate = rate_resp.text()
                    .await?
                    .parse::<f32>()?;
                order.total = order.subtotal * (1.0 + rate) + order.shipping_cost;
                
                "INSERT INTO orders (product_id, quantity, subtotal, shipping_address, shipping_zip, shipping_cost, total) VALUES (:product_id, :quantity, :subtotal, :shipping_address, :shipping_zip, :shipping_cost, :total)"
                    .with(params! {
                        "product_id" => order.product_id,
                        "quantity" => order.quantity,
                        "subtotal" => order.subtotal,
                        "shipping_address" => &order.shipping_address,
                        "shipping_zip" => &order.shipping_zip,
                        "shipping_cost" => order.shipping_cost,
                        "total" => order.total,
                    })
                    .ignore(&mut conn)
                    .await?;

                drop(conn);
                Ok(response_build(&serde_json::to_string_pretty(&order)?))
            } else {
                if rate_resp.status() == StatusCode::NOT_FOUND {
                    Ok(response_build(&String::from("{\"status\":\"error\", \"message\":\"The zip code in the order does not have a corresponding sales tax rate.\"}")))
                } else {
                    Ok(response_build(&String::from("{\"status\":\"error\", \"message\":\"There is an unknown error from the sales tax rate lookup service.\"}")))
                }
            }
        }

        (&Method::GET, "/orders") => {
            let mut conn = pool.get_conn().await.unwrap();

            let orders = "SELECT * FROM orders"
                .with(())
                .map(&mut conn, |(order_id, product_id, quantity, subtotal, shipping_address, shipping_zip, shipping_cost, total)| {
                    Order::new(
                        order_id,
                        product_id,
                        quantity,
                        subtotal,
                        shipping_address,
                        shipping_zip,
                        shipping_cost,
                        total,
                    )},
                ).await?;

            drop(conn);
            Ok(response_build(serde_json::to_string(&orders)?.as_str()))
        }

        // Return the 404 Not Found for other routes.
        _ => {
            let mut not_found = Response::default();
            *not_found.status_mut() = StatusCode::NOT_FOUND;
            Ok(not_found)
        }
    }
}

// CORS headers
fn response_build(body: &str) -> Response<Body> {
    Response::builder()
        .header("Access-Control-Allow-Origin", "*")
        .header("Access-Control-Allow-Methods", "GET, POST, OPTIONS")
        .header("Access-Control-Allow-Headers", "api,Keep-Alive,User-Agent,Content-Type")
        .body(Body::from(body.to_owned()))
        .unwrap()
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    println!("App started. Wait for Dapr sidecar to start ...");
    sleep(Duration::from_millis(1500)).await;

    let client = dapr::Dapr::new(3503, "http://localhost".to_string());
    let v = client.get_secret("local-store", "APP_URL:DATABASE").await?;
    let db_url = v["APP_URL:DATABASE"].as_str().unwrap();
    println!("DATABASE URL is {}", db_url);

    let opts = Opts::from_url(db_url).unwrap();
    let builder = OptsBuilder::from_opts(opts);
    // The connection pool will have a min of 5 and max of 10 connections.
    let constraints = PoolConstraints::new(5, 10).unwrap();
    let pool_opts = PoolOpts::default().with_constraints(constraints);
    let pool = Pool::new(builder.pool_opts(pool_opts));

    let addr = SocketAddr::from(([0, 0, 0, 0], 8003));
    let make_svc = make_service_fn(|_| {
        let pool = pool.clone();
        async move {
            Ok::<_, Infallible>(service_fn(move |req| {
                let pool = pool.clone();
                handle_request(req, pool)
            }))
        }
    });
    let server = Server::bind(&addr).serve(make_svc);
    dbg!("Server started on port 8003");
    if let Err(e) = server.await {
        eprintln!("server error: {}", e);
    }
    Ok(())
}
