// Embeds the app icon and version details in the Windows executable.
fn main() {
    println!("cargo::rerun-if-changed=brand/icon.ico");
    #[cfg(windows)]
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("brand/icon.ico")
            .set("ProductName", "sqlsea")
            .set("FileDescription", "sqlsea");
        res.compile().expect("could not embed the Windows icon");
    }
}
