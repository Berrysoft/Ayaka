use lib_flutter_rust_bridge_codegen::{
    config_parse, frb_codegen, get_symbols_if_no_duplicates, RawOpts,
};

fn main() {
    println!("cargo:rerun-if-changed=src/api.rs");
    let opts = config_parse(RawOpts {
        rust_input: vec!["src/api.rs".to_string()],
        dart_output: vec!["../lib/bridge_generated.dart".to_string()],
        config_file: None,
        dart_decl_output: Some("../lib/bridge_definitions.dart".to_string()),
        c_output: Some(vec!["../ios/Runner/bridge_generated.h".to_string()]),
        extra_c_output_path: Some(vec!["../macos/Runner/".to_string()]),
        rust_crate_dir: None,
        rust_output: None,
        class_name: None,
        dart_format_line_length: 80,
        dart_enums_style: false,
        skip_add_mod_to_lib: true,
        llvm_path: None,
        llvm_compiler_opts: None,
        dart_root: None,
        no_build_runner: false,
        no_use_bridge_in_method: false,
        extra_headers: None,
        verbose: false,
        wasm: false,
        inline_rust: false,
        skip_deps_check: false,
        dump: None,
        dart3: true,
    });
    let all_symbols = get_symbols_if_no_duplicates(&opts).unwrap();
    // run flutter_rust_bridge_codegen
    for opt in opts {
        frb_codegen(&opt, &all_symbols).unwrap();
    }
}
