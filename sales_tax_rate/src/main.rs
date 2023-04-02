use std::net::SocketAddr;
use std::convert::Infallible;
use hyper::service::{make_service_fn, service_fn};
use hyper::{Body, Method, Request, Response, Server};
use csv::Reader;
use serde_json::Value;

/// This is our service handler. It receives a Request, routes on its
/// path, and returns a Future of a Response.
async fn handle_request(req: Request<Body>) -> Result<Response<Body>, anyhow::Error> {
    match (req.method(), req.uri().path()) {
        // Serve some instructions at /
        (&Method::GET, "/") => Ok(Response::new(Body::from(
            "Try POSTing data to /find_rate such as: `curl http://localhost:8001/get_rate -XPOST -d '{\"zip\":\"78701\"}'`",
        ))),

        (&Method::POST, "/find_rate") => {
            let byte_stream = hyper::body::to_bytes(req).await?;
            let json: Value = serde_json::from_slice(&byte_stream).unwrap();
            let zip = json["zip"].as_str().unwrap();
            dbg!(zip);

            let mut rate = "".to_string();
            let rates_data: &[u8] = include_bytes!("rates_by_zipcode.csv");
            let mut rdr = Reader::from_reader(rates_data);
            for result in rdr.records() {
                let record = result?;
                // dbg!("{:?}", record.clone());
                if zip.eq(&record[0]) {
                    rate = record[1].to_string();
                    break;
                }
            }
            dbg!(&rate);

            if rate.is_empty() {
                Ok(Response::new(Body::from("Not Found")))
            } else {
                Ok(Response::new(Body::from(rate)))
            }
        }

        // Return the 404 Not Found for other routes.
        _ => {
            Ok(Response::new(Body::from("Not Found")))
        }
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
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
