use std::str::FromStr;
use std::sync::Arc;

use hyper::{
    client::Client as HyperClient,
    http::{HeaderName, HeaderValue},
};

use crate::client::Client;
use crate::error::Result;

#[cfg(all(not(feature = "native-tls"), not(feature = "rustls")))]
compile_error!("Either feature \"native-tls\" or \"rustls\" must be enabled for this crate.");

#[cfg(feature = "native-tls")]
fn new_http_client() -> HyperClient<hyper_tls::HttpsConnector<hyper::client::HttpConnector>> {
    let https = hyper_tls::HttpsConnector::new();
    HyperClient::builder().build::<_, hyper::Body>(https)
}

// native-tls takes priority
#[cfg(all(feature = "rustls", not(feature = "native-tls")))]
fn new_http_client() -> HyperClient<hyper_rustls::HttpsConnector<hyper::client::HttpConnector>> {
    let https = hyper_rustls::HttpsConnectorBuilder::new()
        .with_native_roots()
        .https_or_http()
        .enable_http1()
        .enable_http2()
        .build();

    HyperClient::builder().build::<_, hyper::Body>(https)
}

async fn send_request(
    client: Arc<Client>,
    req: &libnp::server::HttpRequest,
) -> std::result::Result<libnp::client::HttpResponseData, libnp::client::HttpResponseError> {
    use hyper::{http::Request, Method};
    use libnp::client::HttpResponseError;

    let Some(addr) = client.http_port(&req.hostname).await else {
        return Err(HttpResponseError::NotRegistered);
    };
    let uri = format!("http://{}{}", addr, req.uri);
    let method = Method::from_bytes(req.method.as_bytes())
        .map_err(|e| HttpResponseError::InvalidMethod(e.to_string()))?;
    let body = hyper::Body::from(req.body.clone());
    let mut request = Request::builder()
        .uri(uri)
        .method(method)
        .body(body)
        .map_err(|e| HttpResponseError::Build(e.to_string()))?;

    req.headers.iter().try_for_each(|(key, value)| {
        let name = HeaderName::from_str(key).map_err(|e| {
            HttpResponseError::InvalidHeader(format!("invalid name {}: {}", key, e))
        })?;
        let value = HeaderValue::from_bytes(value).map_err(|e| {
            HttpResponseError::InvalidHeader(format!("invalid value {:?}: {}", value, e))
        })?;
        request.headers_mut().insert(name, value);
        Ok(())
    })?;

    let http_client = new_http_client();
    let http_response = http_client
        .request(request)
        .await
        .map_err(|e| HttpResponseError::Request(e.to_string()))?;
    let resp_status_code = http_response.status().as_u16();
    let resp_headers = http_response
        .headers()
        .into_iter()
        .map(|(key, value)| (key.as_str().to_owned(), value.as_bytes().to_owned()))
        .collect();
    let resp_body = hyper::body::to_bytes(http_response.into_body())
        .await
        .map_err(|e| HttpResponseError::Read(e.to_string()))?;
    Ok(libnp::client::HttpResponseData {
        headers: resp_headers,
        body: resp_body.to_vec(),
        status_code: resp_status_code,
    })
}

pub async fn request(client: Arc<Client>, req: &libnp::server::HttpRequest) -> Result<()> {
    let payload = match send_request(client.clone(), req).await {
        Ok(data) => libnp::client::HttpResponsePayload::Data(data),
        Err(error) => libnp::client::HttpResponsePayload::Error(error),
    };
    tracing::debug!(payload = ?payload, "HTTP response payload");
    let response = libnp::client::HttpResponse {
        uuid: req.uuid.clone(),
        payload,
    };
    client
        .send(&libnp::client::Message::HttpResponse(response))
        .await
}
