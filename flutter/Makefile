.PHONY: clean run

clean:
	flutter clean
	cd native && cargo clean

run:
	RUST_LOG=warn,tunet=trace,native=trace flutter run

.SECONDARY:
