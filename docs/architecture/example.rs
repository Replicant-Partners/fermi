[package]
name = "index_evidence_viz"
version = "0.1.0"
edition = "2021"

[dependencies]
plotters = { version = "0.3", features = ["svg_backend"] }
rand = { version = "0.8", features = ["small_rng"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"