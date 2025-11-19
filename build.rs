use std::{env, path::PathBuf, process::Command};

fn main() {
    println!("cargo::rustc-check-cfg=cfg(jetson_cuda)");
    println!("cargo:rerun-if-changed=src/cuda");
    println!("cargo:rerun-if-changed=npp_wrapper.h");

    let target = std::env::var("TARGET").unwrap();
    
    if !target.contains("aarch64-unknown-linux") {
        println!("cargo:warning=Building without CUDA/NPP (not on Jetson)");
        return;
    }

    println!("cargo:rustc-cfg=jetson_cuda");

    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());

    // Link NPP libraries
    println!("cargo:rustc-link-search=native=/usr/local/cuda/lib64");
    println!("cargo:rustc-link-lib=dylib=nppicc"); // NPP Image Color Conversion library
    println!("cargo:rustc-link-lib=dylib=nppial"); // NPP Image Arithmetic and Logical Operations
    println!("cargo:rustc-link-lib=dylib=nppidei"); // NPP Image Data Exchange and Initialization

    //
    // ---- Generate NPP bindings ----
    //
    let bindings = bindgen::Builder::default()
        .header_contents("npp_wrapper.h", r#"
            #include <nppi_color_conversion.h>
            #include <nppi_data_exchange_and_initialization.h>
            #include <nppi_arithmetic_and_logical_operations.h>
            #include <nppdefs.h>
        "#)
        .clang_arg("-I/usr/local/cuda/include")
        // Debayer
        .allowlist_function("nppiCFAToRGB_16u_C1C3R")
        // Data type conversion
        .allowlist_function("nppiConvert_16u32f_C3R")
        // Per-channel arithmetic (32f C3)
        .allowlist_function("nppiSubC_32f_C3R")
        .allowlist_function("nppiSubC_32f_C3IR")
        .allowlist_function("nppiMulC_32f_C3R")
        .allowlist_function("nppiMulC_32f_C3IR")
        .allowlist_function("nppiAddC_32f_C3R")
        .allowlist_function("nppiAddC_32f_C3IR")
        .allowlist_function("nppiDivC_32f_C3R")
        .allowlist_function("nppiDivC_32f_C3IR")
        // Color matrix transformation
        .allowlist_function("nppiColorTwist_32f_C3R")
        .allowlist_function("nppiColorTwist32f_32f_C3R")
        .allowlist_function("nppiColorTwist32f_32f_C3IR")
        // Types
        .allowlist_type("NppStatus")
        .allowlist_type("NppiSize")
        .allowlist_type("NppiRect")
        .allowlist_type("NppiBayerGridPosition")
        .allowlist_type("NppiInterpolationMode")
        .allowlist_var("NPPI_BAYER_.*")
        .allowlist_var("NPPI_INTER_.*")
        .raw_line("// Mark extern blocks as unsafe for Rust 2024")
        .generate()
        .expect("Unable to generate NPP bindings");

    let out_path = out_dir.join("npp_bindings.rs");
    bindings
        .write_to_file(&out_path)
        .expect("Couldn't write NPP bindings!");
    
    // Read the generated file and add unsafe to extern blocks
    let contents = std::fs::read_to_string(&out_path).unwrap();
    let fixed = contents.replace("extern \"C\" {", "unsafe extern \"C\" {");
    std::fs::write(&out_path, fixed).unwrap();

    //
    // ---- Jetson Orin Nano architecture ----
    //
    let arch = "compute_87";
    let code = "sm_87";

    //
    // ---- Compile each .cu file into PTX ----
    //
    let kernels = [
        "src/cuda/kernels/debayer_rggb_bilinear.cu",
        "src/cuda/kernels/color_pipeline.cu",
    ];

    for kernel in kernels {
        let kpath = PathBuf::from(kernel);
        let name = kpath.file_stem().unwrap().to_str().unwrap();
        let ptx_file = out_dir.join(format!("{name}.ptx"));

        println!("cargo:warning=Compiling {kernel} → {name}.ptx");

        let status = Command::new("nvcc")
            .arg("-ptx")
            .arg("-o")
            .arg(&ptx_file)
            .arg(&kpath)
            .arg(format!("-arch={}", arch))
            .arg(format!("-code={}", code))
            .status()
            .expect("Failed to run nvcc");

        assert!(status.success(), "Failed to compile {kernel} to PTX");
    }
}
