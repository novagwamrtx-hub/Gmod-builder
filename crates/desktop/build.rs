//! Gives the Windows program its icon and the name Windows shows for it.

const ICON: &str = "../../assets/icon.ico";

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed={ICON}");
    if std::env::var_os("CARGO_CFG_WINDOWS").is_none() {
        return;
    }
    let mut resource = winresource::WindowsResource::new();
    resource.set_icon(ICON);
    resource.set("ProductName", "GMod Collection Builder");
    resource.set("FileDescription", "GMod Collection Builder");
    // The program works without them: it is only plainer in Explorer.
    if let Err(error) = resource.compile() {
        println!("cargo:warning=the program was built without its icon: {error}");
    }
}
