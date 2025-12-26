use http::Request;
use metrics::{Unit, describe_histogram, histogram};
use std::{
    borrow::Cow,
    num::NonZeroUsize,
    pin::Pin,
    task::{Context, Poll},
    time::Instant,
};
use tonic::transport::Body;
use tower::Service;

use crate::RPC_CLIENT_DURATION;

#[derive(Debug, Clone)]
pub struct ClientMetricsMiddleware<S> {
    inner: S,
    server_address: Option<String>,
}

impl<S> ClientMetricsMiddleware<S> {
    pub fn new(inner: S) -> Self {
        Self::with_server_address(inner, None::<String>)
    }

    pub fn with_server_address(inner: S, addr: Option<impl Into<String>>) -> Self {
        describe_histogram!(
            RPC_CLIENT_DURATION,
            Unit::Milliseconds,
            "Measures the duration of outbound RPC"
        );

        let addr = if let Some(addr) = addr.map(|v| v.into()) {
            Some(if addr.starts_with("http://") {
                addr[7..].to_string()
            } else if addr.starts_with("https://") {
                addr[8..].to_string()
            } else {
                addr
            })
        } else {
            None
        };
        Self {
            inner,
            server_address: addr,
        }
    }
}

type BoxFuture<'a, T> = Pin<Box<dyn std::future::Future<Output = T> + Send + 'a>>;

impl<S, ReqBody, ResBody> Service<http::Request<ReqBody>> for ClientMetricsMiddleware<S>
where
    S: Service<http::Request<ReqBody>, Response = http::Response<ResBody>> + Clone + Send + 'static,
    S::Future: Send + 'static,
    ReqBody: Body + Send + 'static,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: http::Request<ReqBody>) -> Self::Future {
        // See: https://docs.rs/tower/latest/tower/trait.Service.html#be-careful-when-cloning-inner-services
        let clone = self.inner.clone();
        let mut inner = std::mem::replace(&mut self.inner, clone);

        let start = std::time::Instant::now();
        let path = req.uri().path();

        let service_method_separator: Option<NonZeroUsize> = match path.chars().next() {
            Some('/') => path[1..]
                .find('/')
                .map(|p| NonZeroUsize::new(p + 1).unwrap()),
            _ => None,
        };

        let (rpc_service, rpc_method) = match service_method_separator {
            Some(sep) => (
                path[1..(sep).into()].to_string(),
                path[usize::from(sep) + 1..].to_string(),
            ),
            // If unparsable, say service is empty and method is the entire path
            None => ("".to_string(), path.to_string()),
        };

        let server = match self.server_address.as_ref() {
            Some(addr) => addr.clone(),
            None => req.uri().host().unwrap_or("unknown").to_string(),
        };

        println!("\n\n URI: {:#?}", req.uri());

        let version = network_protocol_version(&req);

        Box::pin(async move {
            let response = inner.call(req).await?;

            let duration = Instant::now().duration_since(start);
            let duration_millis = duration.as_millis() as f64;

            let mut labels = Vec::with_capacity(8);
            labels.push(("rpc.system", Cow::Borrowed("grpc")));
            labels.push(("network.protocol.name", Cow::Borrowed("http")));
            // TODO: If grpc eventually adds support for HTTP 3 this will be wrong :)
            labels.push(("network.transport", Cow::Borrowed("tcp")));
            labels.push(("rpc.method", Cow::Owned(rpc_method)));
            labels.push(("rpc.service", Cow::Owned(rpc_service)));

            labels.push(("server.address", Cow::Owned(server)));

            if let Some(version) = version {
                labels.push(("network.protocol.version", Cow::Borrowed(version)));
            }

            if response.status().is_client_error() || response.status().is_server_error() {
                labels.push(("error.type", Cow::Owned(response.status().to_string())));
            }

            histogram!(RPC_CLIENT_DURATION, &labels).record(duration_millis);

            Ok(response)
        })
    }
}

fn network_protocol_version<T>(req: &Request<T>) -> Option<&'static str> {
    let version = req.version();

    Some(match version {
        http::Version::HTTP_09 => "0.9",
        http::Version::HTTP_10 => "1.0",
        http::Version::HTTP_11 => "1.1",
        http::Version::HTTP_2 => "2",
        http::Version::HTTP_3 => "3",
        _ => return None,
    })
}
