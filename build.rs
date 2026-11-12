fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").ok().as_deref() != Some("windows") {
        return;
    }
    let mut resource = winresource::WindowsResource::new();
    resource.set_icon("assets/offwork-icon.ico");
    resource.set("FileDescription", "下班倒计时");
    resource.set("ProductName", "下班倒计时");
    resource.set("ProductVersion", "2.1.0");
    resource.set("FileVersion", "2.1.0");
    resource.set("LegalCopyright", "Guyu23");
    resource.compile().expect("failed to embed Windows icon");
}
