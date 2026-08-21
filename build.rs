fn main() {
    glib_build_tools::compile_resources(
        &["data"],
        "data/com.chrisdaggas.datacleaner.gresource.xml",
        "data-cleaner.gresource",
    );
}
