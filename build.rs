use std::{env, fs, io, path::Path};

fn copy_dir_all(src: impl AsRef<Path>, dst: impl AsRef<Path>) -> io::Result<()> {
    fs::create_dir_all(&dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        if ty.is_dir() {
            copy_dir_all(entry.path(), dst.as_ref().join(entry.file_name()))?;
        } else {
            fs::copy(entry.path(), dst.as_ref().join(entry.file_name()))?;
        }
    }
    Ok(())
}
#[cfg(target_os = "macos")]
fn link_objc(out_dir: &str) {
    use std::process::Command;

    println!("cargo:rustc-link-lib=framework=Foundation");

    let objc_code = r#"
        #import <Foundation/Foundation.h>
        typedef struct {
            void *result;
            void *error;
        } RustResult;

        RustResult catch_and_log_exception(void* (*block)(void*), void* args) {
            RustResult res;
            res.result = NULL;
            res.error = NULL;
            @try {
                res.result = block(args); // Execute the Rust function that may trigger an exception
                return res;
            } @catch (NSException *exception) {
                // const NSString *name_s = [exception name];
                // const NSString *reason_s = [exception reason];
                // const char *name = [name_s UTF8String];
                // const char *reason = [reason_s UTF8String];
                // Ensure a C copy of the strings is passed
                res.error = exception;
                return res;
                // char *name_c = strdup(name);
                // char *reason_c = strdup(reason);
                // rust_callback(name_c, reason_c); // Call the Rust logging function
                //
                // free(name_c);
                // free(reason_c);
                // res.
                // return 
            }
        }
        "#;

    let objc_file = Path::new(&out_dir).join("objc_exception_wrapper.m");
    let object_file = Path::new(&out_dir).join("objc_exception_wrapper.o");
    let static_lib = Path::new(&out_dir).join("libobjc_exception_wrapper.a");

    // Write the Objective-C file
    fs::write(&objc_file, objc_code).expect("Failed to write Objective-C file");

    // Compile .m file into object file
    let status = Command::new("clang")
        .args([
            "-framework",
            "Foundation",
            "-fPIC",
            "-c",
            objc_file.to_str().unwrap(),
            "-o",
            object_file.to_str().unwrap(),
        ])
        .status()
        .expect("Failed to compile Objective-C source");

    if !status.success() {
        panic!("Objective-C compilation failed");
    }

    // Archive the object file into a static library
    let status = Command::new("ar")
        .args([
            "crus",
            static_lib.to_str().unwrap(),
            object_file.to_str().unwrap(),
        ])
        .status()
        .expect("Failed to create static library");

    if !status.success() {
        panic!("Failed to archive object file into static lib");
    }

    // Link statically
    println!("cargo:rustc-link-search=native={}", out_dir);
    println!("cargo:rustc-link-lib=static=objc_exception_wrapper");
}

fn main() {
    let out_dir = env::var("OUT_DIR").unwrap();
    let profile_out = env::var("PROFILE").unwrap();
    let dest_assets = Path::new("target").join(&profile_out).join("assets");
    copy_dir_all("assets", dest_assets).unwrap();

    #[cfg(target_os = "macos")]
    {
        link_objc(&out_dir);
    }
}
