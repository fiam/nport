use std::{net::SocketAddr, str::FromStr, sync::Arc, time::SystemTime};

use httptest::Expectation;
use libnp::PortProtocol;
use np::client::Client;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    sync::mpsc,
};

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
        .connect(&format!("localhost:{server_port}"), false)
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

fn run_client(client: Arc<Client>) -> (tokio::task::JoinHandle<Arc<Client>>, mpsc::Sender<()>) {
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

    (client_task, client_stop_tx)
}

#[tokio::test]
async fn it_connects() {
    const SERVER_PORT: u16 = 4000;
    let (server_task, server_stop) = start_server(SERVER_PORT).await;
    let client = np::client::Client::new();
    client
        .connect(&format!("localhost:{}", SERVER_PORT), false)
        .await
        .unwrap();
    drop(client);
    server_stop.send(()).await.unwrap();
    server_task.await.unwrap();
}

#[tokio::test]
async fn it_forwards_http_requests() {
    // tracing_subscriber::fmt().with_env_filter("trace").init();
    const SERVER_PORT: u16 = 4001;
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

    let (client_task, client_stop) = run_client(client);

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

    client_stop.send(()).await.unwrap();
    let client = client_task.await.unwrap();
    drop(client);

    // Disconnecting the client should unregister the hostname
    let resp = response_for_hostname(SERVER_PORT, client_hostname).await;
    assert_eq!(404, resp.status());

    server_stop.send(()).await.unwrap();
    server_task.await.unwrap();
}

async fn test_read_write_both_ways(
    server_stream: &mut TcpStream,
    client_stream: &mut TcpStream,
    test_payload: &Vec<u8>,
) {
    let mut buf = vec![0; test_payload.len()];
    server_stream.write_all(test_payload).await.unwrap();
    let n = client_stream.read_exact(&mut buf).await.unwrap();
    assert_eq!(test_payload.len(), n);
    assert_eq!(test_payload[..], buf[0..n]);

    client_stream.write_all(test_payload).await.unwrap();
    let n = server_stream.read_exact(&mut buf).await.unwrap();
    assert_eq!(test_payload.len(), n);
    assert_eq!(test_payload[..], buf[0..n]);
}

#[tokio::test]
async fn it_forwards_tcp_connections() {
    tracing_subscriber::fmt().with_env_filter("trace").init();
    const SERVER_PORT: u16 = 4002;
    // tracing_subscriber::fmt().with_env_filter("trace").init();
    let client_tcp_port = 11235;
    let (server_task, server_stop) = start_server(SERVER_PORT).await;
    let client = connected_client(SERVER_PORT).await;

    // Opening port 0 should assign a random port
    client.tcp_open("", 0, client_tcp_port).await.unwrap();
    client_message(client.clone()).await.unwrap();

    let forwardings = client.port_forwardings().await;
    assert_eq!(1, forwardings.len());
    assert_eq!(PortProtocol::Tcp, forwardings[0].protocol());
    assert_ne!("", forwardings[0].hostname());
    assert_ne!(0, forwardings[0].remote_port());
    assert_eq!(client_tcp_port, forwardings[0].local_port());

    // Opening a second client on the same remote host:port should fail
    let client2 = connected_client(SERVER_PORT).await;
    client2
        .tcp_open(
            forwardings[0].hostname(),
            forwardings[0].remote_port(),
            client_tcp_port,
        )
        .await
        .unwrap();
    client_message(client2.clone()).await.unwrap();
    assert_eq!(0, client2.port_forwardings().await.len());
    drop(client2);

    let (client_task, client_stop) = run_client(client);

    let remote_port = forwardings[0].remote_port();

    let mut buf = vec![0; 32 * 1024];

    // Connecting to the remote end without a local listener should immediately close the connection
    let before = SystemTime::now();
    let mut stream = TcpStream::connect(format!("127.0.0.1:{}", remote_port))
        .await
        .unwrap();

    let n = stream.read(&mut buf).await.unwrap();
    assert_eq!(0, n);
    assert!(
        SystemTime::now()
            .duration_since(before)
            .unwrap()
            .as_millis()
            < 100
    );

    let listener = TcpListener::bind(format!("127.0.0.1:{}", client_tcp_port))
        .await
        .unwrap();

    let mut server_stream = TcpStream::connect(format!("127.0.0.1:{}", remote_port))
        .await
        .unwrap();

    let (mut client_stream, _) = listener.accept().await.unwrap();

    let short_payload = b"test".to_vec();
    test_read_write_both_ways(&mut server_stream, &mut client_stream, &short_payload).await;
    let long_payload = b"test"
        .iter()
        .flat_map(|x| std::iter::repeat(*x).take(1024))
        .collect::<Vec<u8>>();
    test_read_write_both_ways(&mut server_stream, &mut client_stream, &long_payload).await;

    // Closing the server_stream should close the client
    drop(server_stream);
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    let n = client_stream.read(&mut buf).await.unwrap();
    assert_eq!(0, n);
    drop(client_stream);

    let mut server_stream = TcpStream::connect(format!("127.0.0.1:{}", remote_port))
        .await
        .unwrap();

    let (mut client_stream, _) = listener.accept().await.unwrap();
    // Ensure we're connected again
    test_read_write_both_ways(&mut server_stream, &mut client_stream, &short_payload).await;

    // Closing client_stream should close server_stream
    drop(client_stream);
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    let n = server_stream.read(&mut buf).await.unwrap();
    assert_eq!(0, n);
    drop(server_stream);

    client_stop.send(()).await.unwrap();
    let client = client_task.await.unwrap();
    drop(client);
    // Dropping the client should close the remote TCP port
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    assert_eq!(
        std::io::ErrorKind::ConnectionRefused,
        TcpStream::connect(format!("127.0.0.1:{}", remote_port))
            .await
            .err()
            .unwrap()
            .kind()
    );
    server_stop.send(()).await.unwrap();
    server_task.await.unwrap();
}
