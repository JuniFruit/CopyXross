fn main() {
    #[cfg(target_os = "macos")]
    {
        use std::fs;
        use std::path::Path;
        use std::process::Command;

        println!("cargo:rustc-link-lib=framework=Foundation");

        let objc_code = r#"
        #import <Foundation/Foundation.h>

        void catch_and_log_exception(void (*rust_callback)(char*, char*), void (*block)(void)) {
            @try {
                block(); // Execute the Rust function that may trigger an exception
            } @catch (NSException *exception) {
                const NSString *name_s = [exception name];
                const NSString *reason_s = [exception reason];
                const char *name = [name_s UTF8String];
                const char *reason = [reason_s UTF8String];
                // Ensure a C copy of the strings is passed
                char *name_c = strdup(name);
                char *reason_c = strdup(reason);
                rust_callback(name_c, reason_c); // Call the Rust logging function

                free(name_c);
                free(reason_c);
            }
        }
        "#;

        // Define output directory
        let out_dir = std::env::var("OUT_DIR").expect("Failed to get OUT_DIR");
        let objc_file = Path::new(&out_dir).join("objc_exception_wrapper.m");
        let lib_file = Path::new(&out_dir).join("libobjc_exception_wrapper.dylib");

        // Write Objective-C code to file
        fs::write(&objc_file, objc_code).expect("Failed to write Objective-C file");

        // Compile Objective-C file into a shared library
        let status = Command::new("clang")
            .args([
                "-framework",
                "Foundation",
                "-fPIC",
                "-shared",
                objc_file.to_str().unwrap(),
                "-o",
                lib_file.to_str().unwrap(),
            ])
            .status()
            .expect("Failed to compile Objective-C code");

        if !status.success() {
            panic!("Objective-C compilation failed");
        }

        // Tell Rust to link the generated library
        println!("cargo:rustc-link-search=native={}", out_dir);
        println!("cargo:rustc-link-lib=dylib=objc_exception_wrapper");
    }
}
