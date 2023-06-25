.PHONY: test nextest clean update doc book serve-book
test: plugins
	cargo test --all-features
nextest: plugins
	cargo nextest run --all-features
clean:
	cargo clean
	cd plugins && $(MAKE) clean
	cd book && $(MAKE) clean
update:
	cargo update
	cd plugins && $(MAKE) update
doc:
	cargo doc --no-deps
book:
	cd book && $(MAKE) build
serve-book:
	cd book && $(MAKE) serve

.PHONY: plugins debug-cross release release-cross release-android
plugins:
	cd plugins && $(MAKE) plugins

examples/plugins.ayapack: plugins
	(cd -P examples && tar -cf $(abspath $@) -- plugins)

EXAMPLES:=Basic Fibonacci Fibonacci2 Gacha Live2D Orga Pressure Styles

define example-tpl
.PHONY: example-$(1) example-$(1)-gui examples/$(1)/config.tex examples/$(1).ayapack
example-$(1): examples/$(1).ayapack examples/plugins.ayapack
	cargo run -p ayaka-check -- $$(realpath $$^) --auto
examples/$(1)/latex/config.tex: examples/$(1).ayapack examples/plugins.ayapack
	mkdir -p $$(@D)
	cargo run -p ayaka-latex -- $$(realpath $$^) -o $$(abspath $$@)
examples/$(1).ayapack:
	(cd -P examples/$(1) && tar -cf $$(abspath $$@) --exclude=plugins -- *)

endef

$(eval $(foreach ex,$(EXAMPLES),$(call example-tpl,$(ex))))

%.pdf: %.tex
	cd $(dir $<) && latexmk -lualatex $(notdir $<)

.SECONDARY:
