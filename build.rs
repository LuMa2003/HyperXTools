fn main() {
    let mut res = winresource::WindowsResource::new();
    res.set_icon("assets/logo.ico");
    res.compile().expect("Failed to compile Windows resources");
}
