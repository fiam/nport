const SERVER_PORT: u16 = 3000;

#[tokio::test]
async fn it_connects() {
    let server = nport_server::server::Builder::default()
        .http_port(SERVER_PORT)
        .server()
        .await
        .unwrap();
    tokio::spawn(async move { server.run().await });
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    let client = np::client::Client::new();
    client
        .connect(&format!("127.0.0.1:{}", SERVER_PORT))
        .await
        .unwrap();
}
