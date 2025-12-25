use std::{
    borrow::Cow,
    num::NonZeroUsize,
    pin::Pin,
    task::{Context, Poll},
    time::Instant,
};

use http::Request;
use metrics::{Unit, describe_histogram, histogram};
use tonic::transport::Body;
use tower::{Layer, Service};

const RPC_SERVER_DURATION: &'static str = "rpc.server.duration";

#[derive(Debug, Clone, Default)]
pub struct ServerMetricsLayer {}

impl<S> Layer<S> for ServerMetricsLayer {
    type Service = ServerMetricsMiddleware<S>;

    fn layer(&self, service: S) -> Self::Service {
        describe_histogram!(
            RPC_SERVER_DURATION,
            Unit::Milliseconds,
            "Measures the duration of inbound RPC"
        );
        ServerMetricsMiddleware { inner: service }
    }
}

#[derive(Debug, Clone)]
pub struct ServerMetricsMiddleware<S> {
    inner: S,
}

type BoxFuture<'a, T> = Pin<Box<dyn std::future::Future<Output = T> + Send + 'a>>;

impl<S, ReqBody, ResBody> Service<http::Request<ReqBody>> for ServerMetricsMiddleware<S>
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
            Some(first_char) if first_char == '/' => path[1..]
                .find('/')
                .map(|p| NonZeroUsize::new(p + 1).unwrap()),
            _ => None,
        };

        let (rpc_service, rpc_method) = match service_method_separator {
            Some(sep) => (
                path[1..(sep).into()].to_string(),
                path[usize::from(sep) + 1..].to_string(),
            ),
            // If unparseable, say service is empty and method is the entire path
            None => ("".to_string(), path.to_string()),
        };

        let version = network_protocol_version(&req);

        Box::pin(async move {
            let response = inner.call(req).await?;

            let duration = Instant::now().duration_since(start);
            let duration_millis = duration.as_millis() as f64;

            let mut labels = Vec::with_capacity(7);
            labels.push(("rpc.system", Cow::Borrowed("grpc")));
            labels.push(("network.protocol.name", Cow::Borrowed("http")));
            // TODO: If grpc eventually adds support for HTTP 3 this will be wrong :)
            labels.push(("network.transport", Cow::Borrowed("tcp")));
            labels.push(("rpc.method", Cow::Owned(rpc_method)));
            labels.push(("rpc.service", Cow::Owned(rpc_service)));

            if let Some(version) = version {
                labels.push(("network.protocol.version", Cow::Borrowed(version)));
            }

            if response.status().is_client_error() || response.status().is_server_error() {
                labels.push(("error.type", Cow::Owned(response.status().to_string())));
            }

            histogram!(RPC_SERVER_DURATION, &labels).record(duration_millis);

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
