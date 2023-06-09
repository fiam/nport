use std::{net::SocketAddr, str::FromStr, sync::Arc, time::SystemTime};

use httptest::Expectation;
use np::client::Client;
use tokio::sync::mpsc;

const CLIENT_REQUEST_TIMEOUT_SECS: u16 = 1;

async fn start_server(server_port: u16) -> (tokio::task::JoinHandle<()>, mpsc::Sender<()>) {
    let server = nport_server::server::Builder::default()
        .http_port(server_port)
        .domain("localhost")
        .client_request_timeout_secs(CLIENT_REQUEST_TIMEOUT_SECS)
        .server()
        .await
        .unwrap();
    let (stop_tx, mut stop_rx) = mpsc::channel::<()>(1);

    let task = tokio::spawn(async move {
        tokio::select! {
                _ = server.run() => {
                },
                _ = stop_rx.recv() => {
                    server.stop().await;
                    tokio::time::sleep(tokio::time::Duration::from_millis(10000)).await;
                }
        }
    });
    tokio::time::sleep(tokio::time::Duration::from_millis(1000)).await;
    (task, stop_tx)
}

async fn connected_client(server_port: u16) -> Arc<np::client::Client> {
    let client = Arc::new(np::client::Client::new());
    client
        .connect(&format!("localhost:{}", server_port))
        .await
        .unwrap();
    client
}

async fn client_message(client: Arc<Client>) -> np::error::Result<()> {
    let msg = client.recv().await?;
    np::dispatch::message(client.clone(), msg).await
}

fn request_for_hostname(server_port: u16, client_hostname: &str) -> hyper::http::request::Builder {
    hyper::http::Request::builder()
        .uri(format!("http://localhost:{}", server_port))
        .header("Host", client_hostname)
}

async fn response_for_hostname(
    server_port: u16,
    client_hostname: &str,
) -> hyper::Response<hyper::Body> {
    let http_client = hyper::Client::new();
    let req = request_for_hostname(server_port, client_hostname)
        .body(hyper::Body::from(""))
        .unwrap();
    http_client.request(req).await.unwrap()
}

fn post_json_expectation() -> httptest::Expectation {
    use httptest::matchers::{contains, request};
    use httptest::responders::status_code;

    let method = request::method("POST");
    let path = request::path("/foo/bar");
    let headers = request::headers(contains(("content-type", "application/json")));
    let body = request::body(r#"{hello":"world"}"#);

    Expectation::matching(httptest::all_of![method, path, headers, body])
        .respond_with(status_code(200).body("hello").append_header("x-foo", "Bar"))
}

#[tokio::test]
async fn it_connects() {
    const SERVER_PORT: u16 = 4000;
    let (server_task, server_stop) = start_server(SERVER_PORT).await;
    let client = np::client::Client::new();
    client
        .connect(&format!("localhost:{}", SERVER_PORT))
        .await
        .unwrap();
    drop(client);
    server_stop.send(()).await.unwrap();
    server_task.await.unwrap();
}

#[tokio::test]
async fn it_forwards_http_requests() {
    const SERVER_PORT: u16 = 4001;
    // tracing_subscriber::fmt().with_env_filter("trace").init();
    let client_subdomain = "something";
    let client_hostname = "something.localhost";
    let client_http_port = 11234;
    let (server_task, server_stop) = start_server(SERVER_PORT).await;
    let client = connected_client(SERVER_PORT).await;

    // This should be refused, since . is not allowed in the hostname
    client.http_open("foo.bar", client_http_port).await.unwrap();
    client_message(client.clone()).await.unwrap();
    assert_eq!(0, client.http_forwardings().await.len());

    // Forwarding is not set up, we should get a 404
    let resp = response_for_hostname(SERVER_PORT, client_hostname).await;
    assert_eq!(404, resp.status());

    client
        .http_open(client_subdomain, client_http_port)
        .await
        .unwrap();
    // Receive opening message
    client_message(client.clone()).await.unwrap();
    let forwardings = client.http_forwardings().await;
    assert_eq!(1, forwardings.len());
    assert_eq!(client_hostname, forwardings[0].hostname());
    assert_eq!(client_http_port, forwardings[0].local_port());

    // If another client tries to register the same hostname, we should get an error.
    // Use a different local port intentionally
    let client2 = connected_client(SERVER_PORT).await;
    client2
        .http_open(client_subdomain, client_http_port + 100)
        .await
        .unwrap();
    client_message(client2.clone()).await.unwrap();
    assert_eq!(0, client2.http_forwardings().await.len());
    // If the client is not running, we should get a 504 in ~CLIENT_REQUEST_TIMEOUT_SECS

    let before = SystemTime::now();
    let resp = response_for_hostname(SERVER_PORT, client_hostname).await;
    let delta = SystemTime::now()
        .duration_since(before)
        .unwrap()
        .as_millis();
    assert!((1000..1250).contains(&delta));
    assert_eq!(504, resp.status());

    let (client_stop_tx, mut client_stop_rx) = mpsc::channel::<()>(1);

    let client_task = tokio::spawn(async move {
        loop {
            tokio::select! {
                msg = client_message(client.clone()) => {
                    msg.unwrap();
                }
                _ = client_stop_rx.recv() => {
                    return client
                }
            }
        }
    });

    // If the local port is closed, we should get a 502
    let resp = response_for_hostname(SERVER_PORT, client_hostname).await;
    assert_eq!(502, resp.status());

    let http_server = httptest::ServerBuilder::new()
        .bind_addr(SocketAddr::from_str(&format!("127.0.0.1:{}", client_http_port)).unwrap())
        .run()
        .unwrap();

    http_server.expect(
        Expectation::matching(httptest::matchers::request::method_path("GET", "/"))
            .respond_with(httptest::responders::status_code(206)),
    );

    // Once the local port is open, we should get the forwarded response
    let resp = response_for_hostname(SERVER_PORT, client_hostname).await;
    assert_eq!(206, resp.status());

    http_server.expect(post_json_expectation());

    let http_client = hyper::Client::new();
    let req = request_for_hostname(SERVER_PORT, client_hostname)
        .uri(format!("http://localhost:{}/foo/bar", SERVER_PORT))
        .header("content-type", "application/json")
        .method("POST")
        .body(hyper::Body::from(r#"{hello":"world"}"#))
        .unwrap();
    let resp = http_client.request(req).await.unwrap();
    assert_eq!(200, resp.status());
    assert_eq!(
        Some("Bar"),
        resp.headers().get("x-foo").map(|h| h.to_str().unwrap())
    );
    assert_eq!(
        "hello",
        hyper::body::to_bytes(resp.into_body()).await.unwrap()
    );

    client_stop_tx.send(()).await.unwrap();
    let client = client_task.await.unwrap();
    drop(client);

    // Disconnecting the client should unregister the hostname
    let resp = response_for_hostname(SERVER_PORT, client_hostname).await;
    assert_eq!(404, resp.status());

    server_stop.send(()).await.unwrap();
    server_task.await.unwrap();
}
