fn main() {
    tonic_prost_build::configure()
        .compile_protos(&["tests/protos/echo.proto"], &["proto"])
        .unwrap();
}
