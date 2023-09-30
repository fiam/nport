use std::str::FromStr;
use std::sync::Arc;

use hyper::{
    client::Client as HyperClient,
    http::{HeaderName, HeaderValue},
};
use libnp::messages::{
    client::{
        http_response, http_response_error, payload::Message, HttpResponse, HttpResponseData,
        HttpResponseError, HttpResponseErrorCreatingRequest, HttpResponseErrorFetchingResponse,
        HttpResponseErrorForwardingNotRegistered, HttpResponseErrorInvalidHeaderName,
        HttpResponseErrorInvalidHeaderValue, HttpResponseErrorInvalidMethod,
        HttpResponseErrorReadingResponse,
    },
    server::HttpRequest,
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
    req: &HttpRequest,
) -> std::result::Result<HttpResponseData, http_response_error::Error> {
    use hyper::{http::Request, Method};

    let Some(addr) = client.http_port(&req.hostname).await else {
        return Err(http_response_error::Error::ForwardingNotRegistered(
            HttpResponseErrorForwardingNotRegistered {
                hostname: req.hostname.clone(),
            },
        ));
    };
    // TODO: Support https
    let uri = format!(
        "http://{}:{}{}",
        addr.host().unwrap_or("127.0.0.1"),
        addr.port(),
        req.uri
    );
    let method = Method::from_bytes(req.method.as_bytes()).map_err(|e| {
        http_response_error::Error::InvalidMethod(HttpResponseErrorInvalidMethod {
            method: req.method.clone(),
            error: e.to_string(),
        })
    })?;

    let body = if let Some(body) = &req.body {
        hyper::Body::from(body.clone())
    } else {
        hyper::Body::empty()
    };
    let mut request = Request::builder()
        .uri(uri)
        .method(method)
        .body(body)
        .map_err(|e| {
            http_response_error::Error::CreatingRequest(HttpResponseErrorCreatingRequest {
                error: e.to_string(),
            })
        })?;

    req.headers.iter().try_for_each(|(key, value)| {
        let name = HeaderName::from_str(key).map_err(|e| {
            http_response_error::Error::InvalidHeaderName(HttpResponseErrorInvalidHeaderName {
                header_name: key.clone(),
                error: e.to_string(),
            })
        })?;
        let value = HeaderValue::from_bytes(value).map_err(|e| {
            http_response_error::Error::InvalidHeaderValue(HttpResponseErrorInvalidHeaderValue {
                header_name: key.clone(),
                header_value: String::from_utf8_lossy(value).to_string(),
                error: e.to_string(),
            })
        })?;
        request.headers_mut().insert(name, value);
        Ok(())
    })?;

    let http_client = new_http_client();
    let http_response = http_client.request(request).await.map_err(|e| {
        http_response_error::Error::FetchingResponse(HttpResponseErrorFetchingResponse {
            error: e.to_string(),
        })
    })?;
    let resp_status_code = http_response.status().as_u16();
    let resp_headers = http_response
        .headers()
        .into_iter()
        .map(|(key, value)| (key.as_str().to_owned(), value.as_bytes().to_owned()))
        .collect();
    let resp_body = hyper::body::to_bytes(http_response.into_body())
        .await
        .map_err(|e| {
            http_response_error::Error::ReadingResponse(HttpResponseErrorReadingResponse {
                error: e.to_string(),
            })
        })?;
    Ok(HttpResponseData {
        headers: resp_headers,
        body: resp_body.to_vec(),
        status_code: resp_status_code as u32,
    })
}

pub async fn request(client: Arc<Client>, req: &HttpRequest) -> Result<()> {
    let payload = send_request(client.clone(), req)
        .await
        .map(http_response::Payload::Data)
        .map_err(|error| http_response::Payload::Error(HttpResponseError { error: Some(error) }));
    // TODO: Find a better way to expand this
    let payload = match payload {
        Ok(payload) => payload,
        Err(payload) => payload,
    };
    tracing::debug!(payload = ?payload, "HTTP response payload");
    let response = HttpResponse {
        uuid: req.uuid.clone(),
        payload: Some(payload),
    };
    client.send(Message::HttpResponse(response)).await
}
