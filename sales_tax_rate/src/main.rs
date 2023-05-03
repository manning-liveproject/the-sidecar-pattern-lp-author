use std::net::SocketAddr;
use std::convert::Infallible;
use hyper::service::{make_service_fn, service_fn};
use hyper::{Body, Method, Request, Response, Server};
use csv::Reader;
use serde_json::Value;
use serde_json::json;
use serde_json::Value::String;
use tokio::time::{sleep, Duration};

/// This is our service handler. It receives a Request, routes on its
/// path, and returns a Future of a Response.
async fn handle_request(req: Request<Body>) -> Result<Response<Body>, anyhow::Error> {
    match (req.method(), req.uri().path()) {
        // Serve some instructions at /
        (&Method::GET, "/") => Ok(Response::new(Body::from(
            "Try POSTing data to /find_rate such as: `curl http://localhost:8001/find_rate -XPOST -d '{\"zip\":\"78701\"}'`",
        ))),

        (&Method::POST, "/find_rate") => {
            let byte_stream = hyper::body::to_bytes(req).await?;
            let json: Value = serde_json::from_slice(&byte_stream).unwrap();
            let zip = json["zip"].as_str().unwrap();

            let client = dapr::Dapr::new(3501);
            match client.get_state("statestore", zip).await? {
                String(rate) => {
                    dbg!(&rate);
                    Ok(Response::new(Body::from(rate)))
                },
                _ => {
                    dbg!("Returns an error");
                    Ok(Response::new(Body::from("Not Found")))
                }
            }
        }

        // Return Not Found for other routes.
        _ => {
            Ok(Response::new(Body::from("Not Found")))
        }
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    println!("App started. Wait for Dapr sidecar to start ...");
    sleep(Duration::from_millis(1500)).await;

    // Save the sales tax rate for zip code into the Dapr state store.
    let client = dapr::Dapr::new(3501);
    let rates_data: &[u8] = include_bytes!("rates_by_zipcode.csv");
    let mut rdr = Reader::from_reader(rates_data);
    for result in rdr.records() {
        let record = result?;
        let kvs = json!([{
            "key": record[0], "value": record[1]
        }]);
        println!("KVS is {}", serde_json::to_string(&kvs)?);
        client.save_state("statestore", kvs).await?;
    }

    let addr = SocketAddr::from(([0, 0, 0, 0], 8001));
    let make_svc = make_service_fn(|_| {
        async move {
            Ok::<_, Infallible>(service_fn(move |req| {
                handle_request(req)
            }))
        }
    });
    let server = Server::bind(&addr).serve(make_svc);
    dbg!("Server started on port 8001");
    if let Err(e) = server.await {
        eprintln!("server error: {}", e);
    }
    Ok(())
}
