# tonic-metrics

[![CI Status](https://github.com/ranger-ross/tonic-metrics/workflows/Test/badge.svg)](https://github.com/ranger-ross/tonic-metrics/actions)
[![docs.rs](https://docs.rs/tonic-metrics/badge.svg)](https://docs.rs/tonic-metrics)
[![crates.io](https://img.shields.io/crates/v/tonic-metrics.svg)](https://crates.io/crates/tonic-metrics)
[![MIT licensed](https://img.shields.io/badge/license-MIT-blue.svg)](https://github.com/ranger-ross/tonic-metrics/blob/master/LICENSE)

[Metrics.rs](https://metrics.rs) integration for [tonic](https://github.com/hyperium/tonic).

This crate tries to adhere to [OpenTelemetry Semantic Conventions](https://opentelemetry.io/docs/specs/semconv/rpc/rpc-metrics/)

The following metrics are supported:

- [`rpc.server.duration`](https://opentelemetry.io/docs/specs/semconv/rpc/rpc-metrics/#metric-rpcserverduration)

