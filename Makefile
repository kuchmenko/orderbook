.PHONY: run-engine run-engine-lg


run-engine:
	cargo run --manifest-path ./services/engine/Cargo.toml

run-engine-lg:
	cd ./services/engine-load-generator/ && go run .
