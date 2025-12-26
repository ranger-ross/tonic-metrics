use metrics_util::debugging::{DebuggingRecorder, Snapshotter};
use tokio::test;
use tonic::{
    Request, Response, Status, async_trait,
    transport::{Channel, Server},
};
use tonic_metrics::{ServerMetricsLayer, client::ClientMetricsMiddleware};

mod echo;

use echo::{
    EchoRequest, EchoResponse,
    echo_client::EchoClient,
    echo_server::{Echo, EchoServer},
};

const SNAPSHOT_FILTERS: [(&'static str, &'static str); 4] = [
    (
        r"Histogram\(\s*[\s\S]*?\s*\)",
        "Histogram([HISTOGRAM_VALUE])",
    ),
    (
        r#"Label\(\s*"server.port"\s*,\s*[\s\S]*?\s*\)"#,
        r#"Label("server.port", [PORT])"#,
    ),
    (
        r#"Label\(\s*"port"\s*,\s*[\s\S]*?\s*\)"#,
        r#"Label("port", [PORT])"#,
    ),
    (r#"hash: \d*"#, "hash: [HASH]"),
];

#[test]
async fn basic_server_metrics() -> Result<(), Box<dyn std::error::Error>> {
    let snapshotter = install_debug_recorder();

    let addr = "[::1]:50051".parse().unwrap();
    let echo = MyEchoService::default();

    println!("GreeterServer listening on {addr}");

    let handle = tokio::spawn(async move {
        Server::builder()
            .layer(ServerMetricsLayer::default())
            .add_service(EchoServer::new(echo))
            .serve(addr)
            .await
            .unwrap();
    });

    tokio::time::sleep(std::time::Duration::from_millis(150)).await;

    send_request(&addr.to_string(), false).await.unwrap();

    handle.abort();

    let snapshot = snapshotter.snapshot();

    println!("{:#?}", snapshot);
    insta::with_settings!({filters => SNAPSHOT_FILTERS}, {
        insta::assert_debug_snapshot!(snapshot);
    });

    Ok(())
}

#[test]
async fn basic_client_metrics() -> Result<(), Box<dyn std::error::Error>> {
    let snapshotter = install_debug_recorder();

    let addr = "[::1]:50051".parse().unwrap();
    let echo = MyEchoService::default();

    println!("GreeterServer listening on {addr}");

    let handle = tokio::spawn(async move {
        Server::builder()
            .add_service(EchoServer::new(echo))
            .serve(addr)
            .await
            .unwrap();
    });

    tokio::time::sleep(std::time::Duration::from_millis(150)).await;

    send_request(&addr.to_string(), true).await.unwrap();

    handle.abort();

    let snapshot = snapshotter.snapshot();

    println!("{:#?}", snapshot);
    insta::with_settings!({filters => SNAPSHOT_FILTERS}, {
        insta::assert_debug_snapshot!(snapshot);
    });

    Ok(())
}

#[derive(Default)]
pub struct MyEchoService;

#[async_trait]
impl Echo for MyEchoService {
    async fn echo(&self, request: Request<EchoRequest>) -> Result<Response<EchoResponse>, Status> {
        println!("Got a request from {:?}", request.remote_addr());

        let reply = EchoResponse {
            message: request.into_inner().message,
        };
        Ok(Response::new(reply))
    }
}

async fn send_request(
    addr: &str,
    use_client_middleware: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let addr = format!("http://{addr}");

    let request = tonic::Request::new(EchoRequest {
        message: "Hello".into(),
    });

    let response = if use_client_middleware {
        let channel = Channel::from_shared(addr.to_string())?.connect().await?;
        let metrics = ClientMetricsMiddleware::with_server_address(channel, Some(addr));
        EchoClient::new(metrics).echo(request).await?
    } else {
        EchoClient::connect(addr)
            .await
            .unwrap()
            .echo(request)
            .await?
    };

    println!("RESPONSE={response:?}");

    Ok(())
}

fn install_debug_recorder() -> Snapshotter {
    let recorder = DebuggingRecorder::new();
    let snapshotter = recorder.snapshotter();
    recorder.install().unwrap();
    snapshotter
}
