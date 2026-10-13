fn main() {
    let mut build = cc::Build::new();
    build
        .cpp(true)
        .include("vendor/src")
        .file("wrapper.cpp")
        .flag_if_supported("-std=c++11")
        .flag_if_supported("-w");
    for entry in std::fs::read_dir("vendor/src").unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_some_and(|e| e == "cpp") {
            build.file(&path);
        }
    }
    build.compile("world");
    println!("cargo:rerun-if-changed=wrapper.cpp");
    println!("cargo:rerun-if-changed=vendor/src");
}
