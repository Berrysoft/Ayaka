.PHONY: test nextest clean update doc
test:
	cargo test --all-features
nextest:
	cargo nextest run --all-features
clean:
	cargo clean
update:
	cargo update
doc:
	cargo doc --no-deps

.PHONY: run run-gui run-latex
run:
	cargo run --package ayaka-check -- $(FILE) --auto
run-latex:
	cargo run --package ayaka-latex -- $(FILE) -o $(TEXOUT)
